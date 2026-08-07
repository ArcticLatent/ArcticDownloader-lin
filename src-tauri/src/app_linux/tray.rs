//! System tray icon, menu, and ComfyUI-status reflection for the Linux backend.
//!
//! Second slice of the `app_linux.rs` directory-module split (see
//! `docs/cross-platform-development.md`, "Consolidation roadmap" step 1/2
//! addendum). Gated behind the `desktop-tray` Cargo feature the same way it
//! was before the move -- `update_tray_comfy_status` and `setup_tray` each
//! keep both their real and no-op `#[cfg(not(feature = "desktop-tray"))]`
//! variants, since collapsing them to one would be the same
//! easy-to-miss-with-a-text-diff trap the roadmap doc already warns about
//! for `kill_python_processes_for_root`.

// Every one of these is used only by the real (`desktop-tray` feature on)
// implementations below -- the `not(feature = "desktop-tray")` variants of
// `update_tray_comfy_status`/`setup_tray` are no-ops, and
// `tray_enabled_for_platform`'s off branch just returns `false`.
#[cfg(feature = "desktop-tray")]
use crate::shared::{
    comfyui_runtime_running, emit_comfyui_runtime_event, resolve_comfyui_instance_name,
    show_main_window, start_comfyui_root_background, AppState,
};
// `stop_comfyui_root_impl` and `running_in_flatpak` live in the parent
// `app_linux` module itself, not `shared.rs`.
#[cfg(feature = "desktop-tray")]
use super::{running_in_flatpak, stop_comfyui_root_impl};
#[cfg(feature = "desktop-tray")]
use std::path::PathBuf;
#[cfg(feature = "desktop-tray")]
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
#[cfg(feature = "desktop-tray")]
use tauri::Manager;
#[cfg(feature = "desktop-tray")]
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

#[cfg(feature = "desktop-tray")]
static TRAY_MENU_ITEMS: OnceLock<Mutex<Option<TrayMenuItems>>> = OnceLock::new();

#[cfg(feature = "desktop-tray")]
struct TrayMenuItems {
    start: MenuItem<tauri::Wry>,
    stop: MenuItem<tauri::Wry>,
}

#[cfg(feature = "desktop-tray")]
fn tray_menu_items() -> &'static Mutex<Option<TrayMenuItems>> {
    TRAY_MENU_ITEMS.get_or_init(|| Mutex::new(None))
}

#[cfg(feature = "desktop-tray")]
fn stopped_tray_icon() -> Option<Image<'static>> {
    static STOPPED_ICON: OnceLock<Option<Image<'static>>> = OnceLock::new();
    STOPPED_ICON
        .get_or_init(|| {
            #[cfg(target_os = "linux")]
            {
                Image::from_bytes(include_bytes!("../../icons/icon-32.png")).ok()
            }

            #[cfg(not(target_os = "linux"))]
            {
                Image::from_bytes(include_bytes!("../../icons/favicon.ico"))
                    .ok()
                    .or_else(|| Image::from_bytes(include_bytes!("../../icons/icon.ico")).ok())
            }
        })
        .clone()
}

#[cfg(feature = "desktop-tray")]
fn started_tray_icon() -> Option<Image<'static>> {
    static STARTED_ICON: OnceLock<Option<Image<'static>>> = OnceLock::new();
    STARTED_ICON
        .get_or_init(|| {
            #[cfg(target_os = "linux")]
            {
                Image::from_bytes(include_bytes!("../../icons/started-32.png"))
                    .ok()
                    .or_else(|| Image::from_bytes(include_bytes!("../../icons/icon-32.png")).ok())
            }

            #[cfg(not(target_os = "linux"))]
            {
                Image::from_bytes(include_bytes!("../../icons/started.ico"))
                    .ok()
                    .or_else(|| Image::from_bytes(include_bytes!("../../icons/icon.ico")).ok())
            }
        })
        .clone()
}

#[cfg(feature = "desktop-tray")]
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
        } else if let Some(icon) =
            stopped_tray_icon().or_else(|| app.default_window_icon().cloned())
        {
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

#[cfg(not(feature = "desktop-tray"))]
pub(crate) fn update_tray_comfy_status(_app: &AppHandle, _running: bool) {}

#[cfg(feature = "desktop-tray")]
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

    #[cfg(target_os = "linux")]
    if running_in_flatpak() {
        if let Some(home) = std::env::var_os("HOME") {
            let tray_dir = PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("ArcticComfyUIHelper")
                .join("tray-icons");
            if std::fs::create_dir_all(&tray_dir).is_ok() {
                builder = builder.temp_dir_path(&tray_dir);
            }
        }
    }

    if let Some(icon) = stopped_tray_icon().or_else(|| app.default_window_icon().cloned()) {
        builder = builder.icon(icon);
    }

    let _tray = builder.build(app)?;
    let state = app.state::<AppState>();
    let running = comfyui_runtime_running(&state);
    update_tray_comfy_status(app, running);
    Ok(())
}

#[cfg(not(feature = "desktop-tray"))]
pub(crate) fn setup_tray(_app: &AppHandle) -> tauri::Result<()> {
    Ok(())
}

pub(crate) fn tray_enabled_for_platform() -> bool {
    #[cfg(not(feature = "desktop-tray"))]
    {
        false
    }

    #[cfg(feature = "desktop-tray")]
    #[cfg(target_os = "linux")]
    {
        match std::env::var("ARCTIC_ENABLE_TRAY") {
            Ok(v) => {
                let normalized = v.trim().to_ascii_lowercase();
                !matches!(normalized.as_str(), "0" | "false" | "no" | "off")
            }
            Err(_) => true,
        }
    }

    #[cfg(feature = "desktop-tray")]
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}
