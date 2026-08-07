//! System tray icon, menu, and ComfyUI-status reflection for the Windows backend.
//!
//! Windows counterpart to `app_linux/tray.rs`. Simpler than Linux's: tray
//! support here isn't behind a Cargo feature or a runtime toggle (Windows
//! always ships it), so there's no `tray_enabled_for_platform` and no
//! `#[cfg(not(feature = "desktop-tray"))]` no-op variants to keep in sync --
//! `setup_tray` is called unconditionally from `run()`. There's also no
//! `stopped_tray_icon`: the "stopped" state and the tray builder's initial
//! icon both just fall back to `app.default_window_icon()` directly, set
//! from `tauri.conf.json` rather than a Rust-side `main_window_icon()`
//! helper (Linux has one; Windows doesn't).

use crate::shared::{
    comfyui_runtime_running, emit_comfyui_runtime_event, resolve_comfyui_instance_name,
    show_main_window, start_comfyui_root_background, AppState,
};
// `stop_comfyui_root_impl` lives in the parent `app_windows` module itself,
// not `shared.rs`.
use super::stop_comfyui_root_impl;
use std::sync::{Mutex, OnceLock};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

static TRAY_MENU_ITEMS: OnceLock<Mutex<Option<TrayMenuItems>>> = OnceLock::new();

struct TrayMenuItems {
    start: MenuItem<tauri::Wry>,
    stop: MenuItem<tauri::Wry>,
}

fn tray_menu_items() -> &'static Mutex<Option<TrayMenuItems>> {
    TRAY_MENU_ITEMS.get_or_init(|| Mutex::new(None))
}

fn started_tray_icon() -> Option<Image<'static>> {
    static STARTED_ICON: OnceLock<Option<Image<'static>>> = OnceLock::new();
    STARTED_ICON
        .get_or_init(|| Image::from_bytes(include_bytes!("../../icons/started.ico")).ok())
        .clone()
}

pub(crate) fn update_tray_comfy_status(app: &AppHandle, running: bool) {
    if let Some(tray) = app.tray_by_id("arctic_tray") {
        let tooltip = if running {
            let state = app.state::<AppState>();
            let name = resolve_comfyui_instance_name(&state.context, None);
            format!("Arctic ComfyUI Helper - Running: {name}")
        } else {
            "Arctic ComfyUI Helper - ComfyUI: Stopped".to_string()
        };
        let _ = tray.set_tooltip(Some(&tooltip));

        if running {
            if let Some(icon) = started_tray_icon() {
                let _ = tray.set_icon(Some(icon));
            }
        } else if let Some(icon) = app.default_window_icon().cloned() {
            let _ = tray.set_icon(Some(icon));
        }
    }

    if let Ok(guard) = tray_menu_items().lock() {
        if let Some(items) = guard.as_ref() {
            let _ = items.start.set_enabled(!running);
            let _ = items.stop.set_enabled(running);
        }
    }
}

pub(crate) fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "tray_show", "Show App", true, None::<&str>)?;
    let start_item = MenuItem::with_id(app, "tray_start", "Start ComfyUI", true, None::<&str>)?;
    let stop_item = MenuItem::with_id(app, "tray_stop", "Stop ComfyUI", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "tray_quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&show_item, &start_item, &stop_item, &separator, &quit_item],
    )?;

    if let Ok(mut guard) = tray_menu_items().lock() {
        *guard = Some(TrayMenuItems {
            start: start_item.clone(),
            stop: stop_item.clone(),
        });
    }

    let mut builder = TrayIconBuilder::with_id("arctic_tray")
        .menu(&menu)
        .tooltip("Arctic ComfyUI Helper")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => {
                let _ = show_main_window(app);
            }
            "tray_start" => {
                let state = app.state::<AppState>();
                if comfyui_runtime_running(&state) {
                    let instance_name = resolve_comfyui_instance_name(&state.context, None);
                    update_tray_comfy_status(app, true);
                    emit_comfyui_runtime_event(
                        app,
                        "started",
                        format!("{instance_name} is already running."),
                    );
                } else {
                    start_comfyui_root_background(app, None);
                }
            }
            "tray_stop" => {
                let state = app.state::<AppState>();
                let instance_name = resolve_comfyui_instance_name(&state.context, None);
                emit_comfyui_runtime_event(app, "stopping", format!("Stopping {instance_name}..."));
                if let Err(err) = stop_comfyui_root_impl(&state) {
                    log::warn!("Tray stop ComfyUI failed: {err}");
                    emit_comfyui_runtime_event(
                        app,
                        "stop_failed",
                        format!("{instance_name} stop failed: {err}"),
                    );
                } else {
                    let running = comfyui_runtime_running(&state);
                    update_tray_comfy_status(app, running);
                    if running {
                        emit_comfyui_runtime_event(
                            app,
                            "stop_failed",
                            format!("{instance_name} stop did not fully complete."),
                        );
                    } else {
                        emit_comfyui_runtime_event(
                            app,
                            "stopped",
                            format!("{instance_name} stopped."),
                        );
                    }
                }
            }
            "tray_quit" => {
                let state = app.state::<AppState>();
                if let Ok(mut quitting) = state.quitting.lock() {
                    *quitting = true;
                }
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.close();
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    let _tray = builder.build(app)?;
    let state = app.state::<AppState>();
    let running = comfyui_runtime_running(&state);
    update_tray_comfy_status(app, running);
    Ok(())
}
