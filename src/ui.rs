use crate::{
    app::AppContext,
    download::{CivitaiPreview, DownloadError, DownloadSignal, DownloadStatus},
    env_flags::auto_update_enabled,
    model::{LoraDefinition, ModelCatalog, ResolvedModel, ResolvedRamTierThresholds},
    ram::RamTier,
    vram::VramTier,
};
use adw::gio;
use adw::gtk::{
    self, gdk, prelude::*, Align, Box as GtkBox, Button, ComboBoxText, CssProvider, Entry,
    FileChooserAction, FileChooserNative, FlowBox, Image, Label, ListBox, MediaFile, Orientation,
    Picture, ResponseType, ScrolledWindow, Separator,
};
use adw::{Application, ApplicationWindow, HeaderBar, Toast, ToastOverlay};
use anyhow::Result;
use gdk_pixbuf::PixbufLoader;
use log::{info, warn};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc::{self, TryRecvError},
    time::Duration,
};

const APP_CSS: &str = include_str!("../assets/style.css");
const PATREON_ICON_BYTES: &[u8] = include_bytes!("../assets/patreon.png");

pub fn bootstrap(app: &Application, context: AppContext) -> Result<()> {
    if let Err(err) = adw::init() {
        adw::glib::g_warning!(crate::app::APP_ID, "failed to initialize Adwaita: {err}");
    }

    register_application_fonts();
    install_application_css();

    let overlay = build_shell(&context);
    kickoff_update_check(&context, &overlay);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Arctic Downloader")
        .default_width(960)
        .default_height(720)
        .content(&overlay)
        .build();

    window.present();
    Ok(())
}

fn install_application_css() {
    if let Some(display) = gdk::Display::default() {
        let provider = CssProvider::new();
        provider.load_from_data(APP_CSS);
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn register_application_fonts() {
    #[cfg(target_family = "unix")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};

        let fonts = discover_font_files();
        if fonts.is_empty() {
            return;
        }

        unsafe {
            if fontconfig::FcInit() == 0 {
                warn!("fontconfig initialization failed; bundled fonts unavailable");
                return;
            }
            let config = fontconfig::FcConfigGetCurrent();
            if config.is_null() {
                warn!("fontconfig returned null config; bundled fonts unavailable");
                return;
            }

            for path in fonts {
                let c_path = match CString::new(path.as_os_str().as_bytes()) {
                    Ok(cstr) => cstr,
                    Err(err) => {
                        warn!(
                            "Failed to convert font path {} to CString: {err}",
                            path.display()
                        );
                        continue;
                    }
                };
                let result = fontconfig::FcConfigAppFontAddFile(
                    config,
                    c_path.as_ptr() as *const fontconfig::FcChar8,
                );
                if result == 0 {
                    warn!("Failed to register font {}", path.display());
                }
            }
        }
    }
}

fn kickoff_update_check(context: &AppContext, overlay: &ToastOverlay) {
    if !auto_update_enabled() {
        info!("Auto-update is disabled via environment toggle.");
        return;
    }

    let updater = context.updater.clone();
    let overlay_for_update = overlay.clone();

    adw::glib::MainContext::default().spawn_local(async move {
        let check_handle = updater.check_for_update();
        let update = match check_handle.await {
            Ok(Ok(Some(update))) => update,
            Ok(Ok(None)) => return,
            Ok(Err(err)) => {
                warn!("Update check failed: {err:#}");
                return;
            }
            Err(join_err) => {
                warn!("Update check task failed: {join_err}");
                return;
            }
        };

        overlay_for_update.add_toast(Toast::new(&format!(
            "Updating Arctic Downloader to v{}…",
            update.version
        )));

        let install_handle = updater.download_and_install(update);
        match install_handle.await {
            Ok(Ok(applied)) => {
                overlay_for_update.add_toast(Toast::new(&format!(
                    "Update to v{} installed. Restart to finish.",
                    applied.version
                )));
            }
            Ok(Err(err)) => {
                warn!("Update install failed: {err:#}");
                overlay_for_update.add_toast(Toast::new(&format!("Update failed: {err}")));
            }
            Err(join_err) => {
                warn!("Update install task failed: {join_err}");
                overlay_for_update
                    .add_toast(Toast::new("Update failed: unexpected background error."));
            }
        }
    });
}

fn discover_font_files() -> Vec<PathBuf> {
    let mut fonts = Vec::new();
    let directories = ["assets/fonts", "data/fonts", "resources/fonts", "fonts"];

    for dir in directories {
        let path = PathBuf::from(dir);
        if !path.exists() {
            continue;
        }
        let iter = match fs::read_dir(&path) {
            Ok(iter) => iter,
            Err(err) => {
                warn!("Failed to read font directory {}: {err}", path.display());
                continue;
            }
        };
        for entry in iter.flatten() {
            let file_path = entry.path();
            if let Some(ext) = file_path.extension() {
                let ext = ext.to_string_lossy().to_ascii_lowercase();
                if matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
                    fonts.push(file_path);
                }
            }
        }
    }

    fonts
}

#[cfg(target_family = "unix")]
mod fontconfig {
    use std::os::raw::c_int;

    #[allow(non_camel_case_types)]
    pub type FcChar8 = u8;
    #[allow(non_camel_case_types)]
    pub enum FcConfig {}

    #[link(name = "fontconfig")]
    extern "C" {
        pub fn FcInit() -> c_int;
        pub fn FcConfigGetCurrent() -> *mut FcConfig;
        pub fn FcConfigAppFontAddFile(config: *mut FcConfig, file: *const FcChar8) -> c_int;
    }
}

fn build_shell(context: &AppContext) -> ToastOverlay {
    let overlay = ToastOverlay::new();

    let root = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    root.append(&build_header(context));
    root.append(&Separator::new(Orientation::Horizontal));

    let main_controls = build_main_controls(context, overlay.clone());
    let scroller = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .min_content_height(200)
        .child(&main_controls)
        .build();
    root.append(&scroller);

    overlay.set_child(Some(&root));
    overlay
}

fn build_header(context: &AppContext) -> HeaderBar {
    let title_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(Align::Center)
        .build();

    let title_label = Label::builder()
        .label("Arctic Downloader")
        .xalign(0.0)
        .build();
    let version_label = Label::new(None);
    version_label.set_use_markup(true);
    version_label.set_markup(&format!("<i>v{}</i>", context.display_version));
    version_label.add_css_class("dim-label");

    title_row.append(&title_label);
    title_row.append(&version_label);

    let subtitle_label = Label::builder()
        .label("ComfyUI Asset Helper by Arctic Latent")
        .xalign(0.5)
        .build();
    subtitle_label.add_css_class("dim-label");

    let title_column = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(2)
        .halign(Align::Center)
        .build();
    title_column.append(&title_row);
    title_column.append(&subtitle_label);

    HeaderBar::builder().title_widget(&title_column).build()
}

fn build_main_controls(context: &AppContext, overlay: ToastOverlay) -> GtkBox {
    let column = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .build();

    let stack_switcher = gtk::StackSwitcher::new();
    stack_switcher.set_halign(Align::Start);
    stack_switcher.set_margin_bottom(4);

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::SlideLeftRight)
        .hexpand(true)
        .vexpand(true)
        .build();
    stack_switcher.set_stack(Some(&stack));

    let shared_entries: Rc<RefCell<Vec<gtk::Entry>>> = Rc::new(RefCell::new(Vec::new()));

    let model_page = build_model_page(context, overlay.clone(), Rc::clone(&shared_entries));
    stack.add_titled(&model_page, Some("models"), "Models");

    let lora_page = build_lora_page(context, overlay.clone(), Rc::clone(&shared_entries));
    stack.add_titled(&lora_page, Some("loras"), "LoRAs");

    column.append(&stack_switcher);
    column.append(&stack);
    column
}

fn build_model_page(
    context: &AppContext,
    overlay: ToastOverlay,
    shared_entries: Rc<RefCell<Vec<gtk::Entry>>>,
) -> GtkBox {
    let column = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .build();

    let catalog = context.catalog.catalog_snapshot();

    let family_dropdown = ComboBoxText::new();
    family_dropdown.append(Some("all"), "ALL FAMILIES");
    for family in catalog.model_families() {
        let display = family.to_ascii_uppercase();
        family_dropdown.append(Some(family.as_str()), &display);
    }
    family_dropdown.set_active(Some(0));

    let model_dropdown = ComboBoxText::new();
    rebuild_model_dropdown(&model_dropdown, &catalog, None, None);

    let vram_dropdown = ComboBoxText::new();
    for tier in VramTier::all() {
        vram_dropdown.append(Some(tier.identifier()), tier.description());
    }
    vram_dropdown.set_active(Some(0));

    let ram_dropdown = ComboBoxText::new();

    let variant_dropdown = ComboBoxText::new();
    variant_dropdown.set_sensitive(false);

    let comfy_path_entry = Entry::builder()
        .placeholder_text("Select your ComfyUI root folder")
        .editable(false)
        .build();
    shared_entries.borrow_mut().push(comfy_path_entry.clone());

    if let Some(path) = context.config.settings().comfyui_root.clone() {
        comfy_path_entry.set_text(path.to_string_lossy().as_ref());
    }

    let select_folder_button = Button::with_label("Choose ComfyUI Folder…");
    select_folder_button.set_halign(Align::Start);

    let download_button = Button::with_label("Download Assets");
    download_button.set_sensitive(false);

    let status_label = Label::builder()
        .label("Select a model and VRAM tier to view variants.")
        .wrap(true)
        .halign(Align::Start)
        .build();

    let progress_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .build();
    progress_box.set_visible(false);

    let progress_label = Label::builder().halign(Align::Start).wrap(true).build();

    let progress_bar = gtk::ProgressBar::builder()
        .show_text(true)
        .hexpand(true)
        .build();

    progress_box.append(&progress_label);
    progress_box.append(&progress_bar);

    let progress_box_clone = progress_box.clone();
    let progress_label_clone = progress_label.clone();
    let progress_bar_clone = progress_bar.clone();
    let status_label_clone_for_button = status_label.clone();

    let resolved_variant: Rc<RefCell<Option<ResolvedModel>>> = Rc::new(RefCell::new(None));

    update_ram_dropdown_for_model(
        &context,
        &ram_dropdown,
        model_dropdown.active_id().map(|id| id.to_string()),
    );
    if let Some(tier) = context.ram_tier() {
        ram_dropdown.set_active_id(Some(tier.identifier()));
    }
    if ram_dropdown.active().is_none() {
        ram_dropdown.set_active(Some(0));
    }

    let apply_variant_selection: Rc<dyn Fn()> = {
        let context = context.clone();
        let variant_dropdown = variant_dropdown.clone();
        let model_dropdown = model_dropdown.clone();
        let status_label = status_label.clone();
        let download_button = download_button.clone();
        let resolved_variant = Rc::clone(&resolved_variant);
        Rc::new(move || {
            let variant_id = variant_dropdown.active_id().map(|id| id.to_string());
            let model_id = model_dropdown.active_id().map(|id| id.to_string());

            match (model_id, variant_id) {
                (Some(model_id), Some(variant_id)) => {
                    if let Some(resolved) = context.catalog.resolve_variant(&model_id, &variant_id)
                    {
                        status_label.set_label("Select a variant to continue.");
                        download_button.set_sensitive(true);
                        *resolved_variant.borrow_mut() = Some(resolved);
                    } else {
                        status_label.set_label("Unable to load the selected variant details.");
                        download_button.set_sensitive(false);
                        *resolved_variant.borrow_mut() = None;
                    }
                }
                _ => {
                    status_label.set_label("Select a variant to continue.");
                    download_button.set_sensitive(false);
                    *resolved_variant.borrow_mut() = None;
                }
            }
        })
    };

    let refresh_variants: Rc<dyn Fn()> = {
        let context = context.clone();
        let model_dropdown = model_dropdown.clone();
        let vram_dropdown = vram_dropdown.clone();
        let variant_dropdown = variant_dropdown.clone();
        let status_label = status_label.clone();
        let download_button = download_button.clone();
        let resolved_variant = Rc::clone(&resolved_variant);
        let apply_variant_selection = apply_variant_selection.clone();
        Rc::new(move || {
            variant_dropdown.remove_all();
            variant_dropdown.set_sensitive(false);
            download_button.set_sensitive(false);
            *resolved_variant.borrow_mut() = None;

            let tier = vram_dropdown
                .active_id()
                .as_ref()
                .and_then(|id| VramTier::from_identifier(id.as_str()));
            let model_id = model_dropdown.active_id().map(|id| id.to_string());

            match (model_id, tier) {
                (Some(model_id), Some(tier)) => {
                    let variants = context.catalog.variants_for_tier(&model_id, tier);
                    if variants.is_empty() {
                        status_label.set_label("No variants available for the selected VRAM tier.");
                        return;
                    }

                    for variant in variants {
                        variant_dropdown.append(Some(&variant.id), &variant.selection_label());
                    }
                    variant_dropdown.set_sensitive(true);
                    variant_dropdown.set_active(Some(0));
                    status_label.set_label("Select a variant to continue.");
                    apply_variant_selection();
                }
                _ => {
                    status_label.set_label("Select a model and VRAM tier to view variants.");
                }
            }
        })
    };

    {
        let refresh_variants = refresh_variants.clone();
        let context = context.clone();
        let ram_dropdown = ram_dropdown.clone();
        model_dropdown.connect_changed(move |combo| {
            let model_id = combo.active_id().map(|id| id.to_string());
            update_ram_dropdown_for_model(&context, &ram_dropdown, model_id);
            refresh_variants();
        });
    }

    {
        let catalog = catalog.clone();
        let model_dropdown = model_dropdown.clone();
        let refresh_variants = refresh_variants.clone();
        let family_dropdown = family_dropdown.clone();
        let ram_dropdown = ram_dropdown.clone();
        let context = context.clone();
        family_dropdown.connect_changed(move |combo| {
            let selected = combo.active_id().map(|id| id.to_string());
            let filter = selected
                .as_deref()
                .filter(|value| !value.is_empty() && *value != "all");
            let previous_model = model_dropdown.active_id().map(|id| id.to_string());

            rebuild_model_dropdown(&model_dropdown, &catalog, filter, previous_model.as_deref());

            let model_id = model_dropdown.active_id().map(|id| id.to_string());
            update_ram_dropdown_for_model(&context, &ram_dropdown, model_id.clone());
            refresh_variants();
        });
    }

    {
        let refresh_variants = refresh_variants.clone();
        vram_dropdown.connect_changed(move |_| {
            refresh_variants();
        });
    }

    {
        let apply_variant_selection = apply_variant_selection.clone();
        variant_dropdown.connect_changed(move |_| {
            apply_variant_selection();
        });
    }

    {
        let context = context.clone();
        let overlay = overlay.clone();
        let comfy_path_entry = comfy_path_entry.clone();
        let resolved_variant = Rc::clone(&resolved_variant);
        let ram_dropdown = ram_dropdown.clone();
        let progress_box = progress_box_clone.clone();
        let progress_label = progress_label_clone.clone();
        let progress_bar = progress_bar_clone.clone();
        let status_label = status_label_clone_for_button.clone();
        let download_button_clone = download_button.clone();
        let cancel_state = Rc::new(RefCell::new(None::<CancellationHandle>));
        download_button.connect_clicked({
            let cancel_state = cancel_state.clone();
            move |_| {
                if let Some(handle) = cancel_state.borrow_mut().take() {
                    handle.cancel();
                    return;
                }

                let Some(mut resolved) = resolved_variant.borrow().clone() else {
                    overlay.add_toast(Toast::new("Select a variant before downloading."));
                    return;
                };

                let comfy_text = comfy_path_entry.text();
                if comfy_text.is_empty() {
                    overlay.add_toast(Toast::new("Choose your ComfyUI folder first."));
                    return;
                }

                let comfy_path = PathBuf::from(comfy_text.as_str());

                let ram_tier = ram_dropdown
                    .active_id()
                    .as_ref()
                    .and_then(|id| RamTier::from_identifier(id.as_str()))
                    .or_else(|| {
                        ram_dropdown
                            .active()
                            .and_then(|idx| RamTier::all().get(idx as usize).copied())
                    })
                    .or_else(|| context.ram_tier());

                let plan_artifacts = resolved.artifacts_for_download(ram_tier);

                if plan_artifacts.is_empty() {
                    overlay.add_toast(Toast::new(
                        "No artifacts match the selected RAM tier for this variant.",
                    ));
                    return;
                }

                let all_present = plan_artifacts.iter().all(|artifact| {
                    comfy_path
                        .join(artifact.target_category.comfyui_subdir())
                        .join(resolved.master.id.as_str())
                        .join(artifact.file_name())
                        .exists()
                });

                if all_present {
                    let message = format!(
                    "All artifacts for {} are already downloaded for the selected GPU/RAM tier.",
                    resolved.master.display_name
                );
                    overlay.add_toast(Toast::new(&message));
                    progress_box.set_visible(true);
                    progress_label.set_text(&message);
                    progress_bar.set_fraction(1.0);
                    progress_bar.set_text(Some("100%"));
                    progress_bar.set_show_text(true);
                    status_label.set_text(&message);
                    return;
                }

                resolved.variant.artifacts = plan_artifacts;

                let comfy_root_for_summary = comfy_path.clone();
                let downloads = context.downloads.clone();
                let overlay_clone = overlay.clone();
                let master_name = resolved.master.display_name.clone();

                let progress_box_async = progress_box.clone();
                let progress_label_async = progress_label.clone();
                let progress_bar_async = progress_bar.clone();
                let download_button_async = download_button_clone.clone();
                let status_label_async = status_label.clone();

                progress_box.set_visible(true);
                progress_label.set_text(&format!("Preparing download for {master_name}…"));
                progress_bar.set_fraction(0.0);
                progress_bar.set_show_text(true);
                progress_bar.set_text(Some("0%"));
                progress_bar.set_pulse_step(0.02);
                progress_bar.pulse();
                download_button_clone.set_label("Cancel Download");
                download_button_clone.set_sensitive(true);

                let (progress_sender, progress_receiver) = mpsc::channel::<DownloadSignal>();
                let progress_state = Rc::new(RefCell::new(DownloadProgressState::default()));
                let receiver_cell = Rc::new(RefCell::new(progress_receiver));
                let progress_bar_updates = progress_bar.clone();
                let progress_label_updates = progress_label.clone();
                let progress_box_updates = progress_box.clone();
                let state_for_updates = progress_state.clone();
                let receiver_for_updates = receiver_cell.clone();

                adw::glib::timeout_add_local(Duration::from_millis(50), move || {
                    let mut receiver_ref = receiver_for_updates.borrow_mut();
                    let receiver = &mut *receiver_ref;
                    let mut state = state_for_updates.borrow_mut();

                    loop {
                        match receiver.try_recv() {
                            Ok(DownloadSignal::Started {
                                artifact,
                                index,
                                total,
                                size,
                            }) => {
                                state.total = total;
                                state.entries.insert(
                                    index,
                                    EntryState {
                                        received: 0,
                                        size,
                                        finished: false,
                                    },
                                );
                                state.current_index = Some(index);
                                progress_label_updates.set_text(&format!("Starting {artifact}…"));
                                progress_bar_updates.set_fraction(0.0);
                                update_progress_text(&progress_bar_updates, &state, Some(0.0));
                            }
                            Ok(DownloadSignal::Progress {
                                artifact,
                                index,
                                received,
                                size,
                            }) => {
                                let entry = state.entries.entry(index).or_insert(EntryState {
                                    received: 0,
                                    size,
                                    finished: false,
                                });
                                entry.received = received;
                                entry.size = size;
                                let fraction = state.fraction();
                                if let Some(value) = fraction {
                                    progress_bar_updates.set_fraction(value.clamp(0.0, 1.0));
                                }
                                update_progress_text(&progress_bar_updates, &state, fraction);
                                progress_label_updates
                                    .set_text(&format!("Downloading {artifact}…"));
                            }
                            Ok(DownloadSignal::Finished { index, size }) => {
                                if let Some(entry) = state.entries.get_mut(&index) {
                                    entry.finished = true;
                                    entry.size = size;
                                    if let Some(size_bytes) = size {
                                        entry.received = size_bytes;
                                    }
                                }

                                let fraction = state.fraction();
                                if let Some(value) = fraction {
                                    progress_bar_updates.set_fraction(value.clamp(0.0, 1.0));
                                }
                                update_progress_text(&progress_bar_updates, &state, fraction);

                                if state.is_complete() {
                                    progress_label_updates.set_text("All downloads complete.");
                                    progress_bar_updates.set_fraction(1.0);
                                    update_progress_text(&progress_bar_updates, &state, Some(1.0));
                                    progress_box_updates.set_visible(false);
                                }
                            }
                            Ok(DownloadSignal::Failed { artifact, error }) => {
                                state.failed = true;
                                progress_label_updates
                                    .set_text(&format!("Failed to download {artifact}: {error}"));
                                progress_bar_updates.set_fraction(0.0);
                                progress_bar_updates.set_text(Some("Failed"));
                                progress_box_updates.set_visible(true);
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => {
                                if !state.is_complete() {
                                    state.failed = true;
                                    progress_label_updates.set_text("Download interrupted.");
                                    progress_bar_updates.set_fraction(0.0);
                                    progress_bar_updates.set_text(Some("Interrupted"));
                                    progress_box_updates.set_visible(true);
                                }
                                break;
                            }
                        }
                    }

                    if state.failed {
                        adw::glib::ControlFlow::Break
                    } else if state.is_complete() {
                        adw::glib::ControlFlow::Break
                    } else {
                        adw::glib::ControlFlow::Continue
                    }
                });

                let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
                {
                    let mut slot = cancel_state.borrow_mut();
                    *slot = Some(CancellationHandle {
                        cancel: Some(cancel_tx),
                    });
                }
                let cancel_slot = cancel_state.clone();

                adw::glib::MainContext::default().spawn_local(async move {
                let start_message = format!("Downloading {} artifacts…", master_name);
                overlay_clone.add_toast(Toast::new(&start_message));

                let handle =
                    downloads.download_variant(comfy_path, resolved, progress_sender);
                tokio::pin!(handle);

                tokio::select! {
                    _ = cancel_rx => {
                        handle.as_mut().abort();
                        let _ = handle.await;
                        progress_label_async.set_text("Download cancelled.");
                        progress_bar_async.set_fraction(0.0);
                        progress_bar_async.set_text(Some("Cancelled"));
                        progress_box_async.set_visible(true);
                        download_button_async.set_label("Download Assets");
                        download_button_async.set_sensitive(true);
                        status_label_async.set_text("Download cancelled.");
                        overlay_clone.add_toast(Toast::new("Download cancelled."));
                        *cancel_slot.borrow_mut() = None;
                    }
                    result = &mut handle => {
                        *cancel_slot.borrow_mut() = None;
                        match result {
                    Ok(Ok(outcomes)) => {
                        let downloaded = outcomes
                            .iter()
                            .filter(|o| o.status == DownloadStatus::Downloaded)
                            .count();
                        let skipped = outcomes
                            .iter()
                            .filter(|o| o.status == DownloadStatus::SkippedExisting)
                            .count();

                        progress_bar_async.set_fraction(1.0);
                        progress_bar_async.set_text(Some("100%"));
                        progress_label_async.set_text("All downloads complete.");
                        progress_box_async.set_visible(false);
                        download_button_async.set_label("Download Assets");
                        download_button_async.set_sensitive(true);
                        status_label_async.set_text("Downloads complete.");

                        for outcome in &outcomes {
                            let toast = match outcome.status {
                                DownloadStatus::Downloaded => {
                                    format!("Saved {}", outcome.artifact.file_name())
                                }
                                DownloadStatus::SkippedExisting => {
                                    format!("Skipped existing {}", outcome.artifact.file_name())
                                }
                            };
                            overlay_clone.add_toast(Toast::new(&toast));
                        }

                        let summary =
                            format!("Finished: {downloaded} downloaded, {skipped} skipped.");
                        overlay_clone.add_toast(Toast::new(&summary));
                        let summary_entries: Vec<DownloadSummaryEntry> = outcomes
                            .iter()
                            .filter(|outcome| outcome.status == DownloadStatus::Downloaded)
                            .map(|outcome| DownloadSummaryEntry {
                                file_name: outcome.artifact.file_name().to_string(),
                                destination: outcome.destination.clone(),
                            })
                            .collect();
                        if !summary_entries.is_empty() {
                            show_download_summary_window(
                                overlay_clone.clone(),
                                &comfy_root_for_summary,
                                &summary_entries,
                            );
                        }
                    }
                    Ok(Err(err)) => {
                        progress_label_async.set_text(&format!("Download failed: {err}"));
                        progress_bar_async.set_fraction(0.0);
                        progress_bar_async.set_text(Some("Failed"));
                        progress_box_async.set_visible(true);
                        download_button_async.set_label("Download Assets");
                        download_button_async.set_sensitive(true);
                        status_label_async.set_text("Download failed.");
                        overlay_clone.add_toast(Toast::new(&format!("Download failed: {err}")));
                    }
                    Err(join_err) => {
                        progress_box_async.set_visible(true);
                        progress_bar_async.set_fraction(0.0);
                        progress_bar_async.set_text(Some("Error"));
                        progress_label_async
                            .set_text(&format!("Download task panicked: {join_err}"));
                        status_label_async.set_text(&format!("Download task panicked: {join_err}"));
                        let message = format!("Download task panicked: {join_err}");
                        overlay_clone.add_toast(Toast::new(&message));
                        download_button_async.set_label("Download Assets");
                        download_button_async.set_sensitive(true);
                    }
                        }
                    }
                }
            });
            }
        });
    }

    refresh_variants();

    column.append(&labelled_row("Model Family", &family_dropdown));
    column.append(&labelled_row("Master Model", &model_dropdown));
    column.append(&labelled_row("GPU VRAM", &vram_dropdown));
    column.append(&labelled_row("System RAM", &ram_dropdown));
    column.append(&labelled_row("Variant / Quantization", &variant_dropdown));
    column.append(&labelled_row("ComfyUI Folder", &comfy_path_entry));

    {
        let context = context.clone();
        let overlay = overlay.clone();
        let comfy_path_entry = comfy_path_entry.clone();
        let shared_entries = Rc::clone(&shared_entries);
        select_folder_button.connect_clicked(move |_| {
            open_folder_picker(
                context.clone(),
                comfy_path_entry.clone(),
                overlay.clone(),
                Rc::clone(&shared_entries),
            );
        });
    }

    column.append(&select_folder_button);
    column.append(&download_button);
    column.append(&progress_box);
    column.append(&status_label);
    column.append(&build_quant_legend());

    let links_box = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(16)
        .halign(Align::Center)
        .build();
    links_box.set_hexpand(true);
    links_box.set_margin_top(12);

    let youtube_button = Button::builder()
        .tooltip_text("Open Arctic Latent on YouTube")
        .build();
    youtube_button.add_css_class("flat");
    youtube_button.add_css_class("link-pill");
    youtube_button.add_css_class("youtube-link");
    youtube_button.set_halign(Align::Center);
    let youtube_label = Label::builder()
        .label("YouTube")
        .css_classes(vec![String::from("link-label")])
        .build();
    let youtube_content = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(Align::Center)
        .build();
    let youtube_icon = Label::new(Some("\u{f167}"));
    youtube_icon.add_css_class("link-icon");
    youtube_icon.add_css_class("youtube-icon");
    youtube_content.append(&youtube_icon);
    youtube_content.append(&youtube_label);
    youtube_button.set_child(Some(&youtube_content));

    let github_button = Button::builder()
        .tooltip_text("Open Arctic Latent on GitHub")
        .build();
    github_button.add_css_class("flat");
    github_button.add_css_class("link-pill");
    github_button.add_css_class("github-link");
    github_button.set_halign(Align::Center);
    let github_label = Label::builder()
        .label("GitHub")
        .css_classes(vec![String::from("link-label")])
        .build();
    let github_content = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(Align::Center)
        .build();
    let github_icon = Label::new(Some("\u{f09b}"));
    github_icon.add_css_class("link-icon");
    github_icon.add_css_class("github-icon");
    github_content.append(&github_icon);
    github_content.append(&github_label);
    github_button.set_child(Some(&github_content));

    let patreon_button = Button::builder()
        .tooltip_text("Open Arctic Latent on Patreon")
        .build();
    patreon_button.add_css_class("flat");
    patreon_button.add_css_class("link-pill");
    patreon_button.add_css_class("patreon-link");
    patreon_button.set_halign(Align::Center);
    let patreon_label = Label::builder()
        .label("Patreon")
        .css_classes(vec![String::from("link-label")])
        .build();
    let patreon_content = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(Align::Center)
        .build();
    let patreon_icon: gtk::Widget =
        if let Some(texture) = texture_from_image_bytes(PATREON_ICON_BYTES) {
            let image = Image::from_paintable(Some(&texture));
            image.set_pixel_size(18);
            image.upcast()
        } else {
            warn!("Failed to decode Patreon icon PNG; falling back to font glyph.");
            Label::new(Some("\u{f109}")).upcast()
        };
    patreon_icon.add_css_class("link-icon");
    patreon_icon.add_css_class("patreon-icon");
    patreon_content.append(&patreon_icon);
    patreon_content.append(&patreon_label);
    patreon_button.set_child(Some(&patreon_content));

    {
        let overlay = overlay.clone();
        youtube_button.connect_clicked(move |_| {
            if let Err(err) = gio::AppInfo::launch_default_for_uri(
                "https://www.youtube.com/@ArcticLatent",
                None::<&gio::AppLaunchContext>,
            ) {
                let message = format!("Failed to open YouTube: {err}");
                overlay.add_toast(Toast::new(&message));
                adw::glib::g_warning!(crate::app::APP_ID, "{message}");
            }
        });
    }

    {
        let overlay = overlay.clone();
        github_button.connect_clicked(move |_| {
            if let Err(err) = gio::AppInfo::launch_default_for_uri(
                "https://github.com/ArcticLatent",
                None::<&gio::AppLaunchContext>,
            ) {
                let message = format!("Failed to open GitHub: {err}");
                overlay.add_toast(Toast::new(&message));
                adw::glib::g_warning!(crate::app::APP_ID, "{message}");
            }
        });
    }

    {
        let overlay = overlay.clone();
        patreon_button.connect_clicked(move |_| {
            if let Err(err) = gio::AppInfo::launch_default_for_uri(
                "https://patreon.com/ArcticLatent",
                None::<&gio::AppLaunchContext>,
            ) {
                let message = format!("Failed to open Patreon: {err}");
                overlay.add_toast(Toast::new(&message));
                adw::glib::g_warning!(crate::app::APP_ID, "{message}");
            }
        });
    }

    links_box.append(&youtube_button);
    links_box.append(&github_button);
    links_box.append(&patreon_button);
    column.append(&links_box);

    column
}

fn build_lora_page(
    context: &AppContext,
    overlay: ToastOverlay,
    shared_entries: Rc<RefCell<Vec<gtk::Entry>>>,
) -> GtkBox {
    let column = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .build();

    column.append(&build_civitai_token_row(context, overlay.clone()));

    let family_dropdown = ComboBoxText::new();
    family_dropdown.append(Some(""), "All Families");
    for family in context.catalog.lora_families() {
        family_dropdown.append(Some(&family), &family);
    }
    family_dropdown.set_active(Some(0));

    let lora_dropdown = ComboBoxText::new();
    lora_dropdown.set_sensitive(false);

    let comfy_path_entry = Entry::builder()
        .placeholder_text("Select your ComfyUI root folder")
        .editable(false)
        .build();
    shared_entries.borrow_mut().push(comfy_path_entry.clone());

    if let Some(path) = context.config.settings().comfyui_root.clone() {
        comfy_path_entry.set_text(path.to_string_lossy().as_ref());
    }

    let select_folder_button = Button::with_label("Choose ComfyUI Folder…");
    select_folder_button.set_halign(Align::Start);

    let download_button = Button::with_label("Download LoRA");
    download_button.set_sensitive(false);

    let status_label = Label::builder()
        .label("Select a LoRA to continue.")
        .wrap(true)
        .halign(Align::Start)
        .build();

    let metadata_status = Label::builder().wrap(true).halign(Align::Start).build();
    metadata_status.set_visible(false);

    let metadata_picture = Picture::builder()
        .width_request(320)
        .height_request(180)
        .build();
    metadata_picture.set_visible(false);
    metadata_picture.set_can_shrink(true);

    let metadata_triggers = FlowBox::builder()
        .row_spacing(6)
        .column_spacing(6)
        .max_children_per_line(8)
        .selection_mode(gtk::SelectionMode::None)
        .halign(Align::Start)
        .build();
    metadata_triggers.set_visible(false);

    let metadata_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(8)
        .build();
    metadata_box.append(&metadata_status);
    metadata_box.append(&metadata_picture);

    let metadata_creator_label = Label::builder()
        .use_markup(true)
        .halign(Align::Start)
        .wrap(true)
        .css_classes(vec![String::from("legend-text")])
        .build();
    metadata_creator_label.set_visible(false);
    metadata_box.append(&metadata_creator_label);

    let metadata_usage_label = Label::builder()
        .label("Suggested strength: ")
        .halign(Align::Start)
        .wrap(true)
        .css_classes(vec![String::from("legend-text")])
        .build();
    metadata_usage_label.set_visible(false);
    metadata_box.append(&metadata_usage_label);

    let metadata_triggers_label = Label::builder()
        .label("Trigger Words")
        .halign(Align::Start)
        .css_classes(vec![String::from("heading")])
        .build();
    metadata_triggers_label.set_visible(false);
    metadata_box.append(&metadata_triggers_label);
    metadata_box.append(&metadata_triggers);

    let metadata_description_label = Label::builder()
        .label("Description")
        .halign(Align::Start)
        .css_classes(vec![String::from("heading")])
        .build();
    metadata_description_label.set_visible(false);

    let metadata_description = Label::builder()
        .halign(Align::Start)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .build();
    metadata_description.set_text("Select a LoRA to view its description.");

    let metadata_description_scroller = ScrolledWindow::builder()
        .min_content_height(180)
        .max_content_height(360)
        .child(&metadata_description)
        .build();
    metadata_description_scroller.set_visible(false);

    metadata_box.append(&metadata_description_label);
    metadata_box.append(&metadata_description_scroller);

    let metadata_row = labelled_row("LoRA Details", &metadata_box);
    metadata_row.set_visible(false);

    let progress_box = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .build();
    progress_box.set_visible(false);

    let progress_label = Label::builder().halign(Align::Start).wrap(true).build();

    let progress_bar = gtk::ProgressBar::builder()
        .show_text(true)
        .hexpand(true)
        .build();

    progress_box.append(&progress_label);
    progress_box.append(&progress_bar);

    let all_loras: Rc<Vec<LoraDefinition>> = Rc::new(context.catalog.loras());
    let filtered_loras: Rc<RefCell<Vec<LoraDefinition>>> = Rc::new(RefCell::new(Vec::new()));
    let resolved_lora: Rc<RefCell<Option<LoraDefinition>>> = Rc::new(RefCell::new(None));
    let current_preview_request: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let active_media: Rc<RefCell<Option<MediaFile>>> = Rc::new(RefCell::new(None));

    let downloads_for_preview = context.downloads.clone();
    let config_for_preview = context.config.clone();

    let update_preview: Rc<dyn Fn(Option<LoraDefinition>)> = {
        let metadata_status = metadata_status.clone();
        let metadata_picture = metadata_picture.clone();
        let metadata_triggers = metadata_triggers.clone();
        let metadata_triggers_label = metadata_triggers_label.clone();
        let metadata_creator_label = metadata_creator_label.clone();
        let metadata_usage_label = metadata_usage_label.clone();
        let metadata_description_label = metadata_description_label.clone();
        let metadata_description = metadata_description.clone();
        let metadata_description_scroller = metadata_description_scroller.clone();
        let metadata_row = metadata_row.clone();
        let current_preview_request = Rc::clone(&current_preview_request);
        let downloads = downloads_for_preview.clone();
        let config = config_for_preview.clone();
        let overlay = overlay.clone();
        let active_media = Rc::clone(&active_media);

        Rc::new(move |maybe_lora: Option<LoraDefinition>| {
            metadata_picture.set_paintable(Option::<&gdk::Texture>::None);
            metadata_picture.set_visible(false);
            if let Some(media) = active_media.borrow_mut().take() {
                media.set_playing(false);
            }
            clear_flow_box(&metadata_triggers);
            metadata_triggers.set_visible(false);
            metadata_triggers_label.set_visible(false);
            metadata_creator_label.set_visible(false);
            metadata_usage_label.set_visible(false);
            metadata_description_scroller.set_visible(false);
            metadata_description_label.set_visible(false);
            metadata_description.set_text("");
            metadata_status.set_visible(false);
            metadata_row.set_visible(false);
            *current_preview_request.borrow_mut() = None;

            let Some(lora) = maybe_lora else {
                return;
            };

            metadata_row.set_visible(true);

            if !lora
                .download_url
                .to_ascii_lowercase()
                .contains("civitai.com")
            {
                metadata_status.set_text("Preview available only for LoRAs hosted on Civitai.");
                metadata_status.set_visible(true);
                return;
            }

            metadata_status.set_text("Fetching LoRA preview…");
            metadata_status.set_visible(true);

            let download_url = lora.download_url.clone();
            let lora_id = lora.id.clone();
            *current_preview_request.borrow_mut() = Some(lora_id.clone());

            let metadata_status_clone = metadata_status.clone();
            let metadata_picture_clone = metadata_picture.clone();
            let metadata_triggers_clone = metadata_triggers.clone();
            let metadata_triggers_label_clone = metadata_triggers_label.clone();
            let metadata_creator_label_clone = metadata_creator_label.clone();
            let metadata_usage_label_clone = metadata_usage_label.clone();
            let metadata_description_label_clone = metadata_description_label.clone();
            let metadata_description_clone = metadata_description.clone();
            let metadata_description_scroller_clone = metadata_description_scroller.clone();
            let metadata_row_clone = metadata_row.clone();
            let current_preview_request_clone = Rc::clone(&current_preview_request);
            let downloads = downloads.clone();
            let config = config.clone();
            let overlay = overlay.clone();
            let active_media_clone = Rc::clone(&active_media);

            adw::glib::MainContext::default().spawn_local(async move {
                let token = config.settings().civitai_token.clone();
                let handle = downloads.civitai_model_metadata(download_url, token);
                match handle.await {
                    Ok(Ok(metadata)) => {
                        if current_preview_request_clone.borrow().as_deref()
                            != Some(lora_id.as_str())
                        {
                            return;
                        }

                        metadata_status_clone.set_visible(false);

                        let mut has_content = false;

                        metadata_picture_clone.set_paintable(Option::<&gdk::Texture>::None);
                        metadata_picture_clone.set_visible(false);
                        if let Some(media) = active_media_clone.borrow_mut().take() {
                            media.set_playing(false);
                        }

                        metadata_creator_label_clone.set_visible(false);
                        if let Some(username_raw) = metadata
                            .creator_username
                            .as_deref()
                            .filter(|s| !s.is_empty())
                        {
                            let link_raw = metadata
                                .creator_link
                                .clone()
                                .filter(|link| !link.is_empty())
                                .unwrap_or_else(|| {
                                    format!("https://civitai.com/user/{username_raw}")
                                });
                            let escaped_username = gtk::glib::markup_escape_text(username_raw);
                            let escaped_link = gtk::glib::markup_escape_text(&link_raw);
                            metadata_creator_label_clone.set_markup(&format!(
                                "Creator: <a href=\"{escaped_link}\">{escaped_username}</a>"
                            ));
                            metadata_creator_label_clone.set_visible(true);
                            has_content = true;
                        }

                        metadata_usage_label_clone.set_visible(false);
                        if let Some(weight) = metadata.usage_strength {
                            metadata_usage_label_clone
                                .set_label(&format!("Suggested strength: {:.2}", weight));
                            metadata_usage_label_clone.set_visible(true);
                            has_content = true;
                        }

                        metadata_description_label_clone.set_visible(false);
                        metadata_description_scroller_clone.set_visible(false);
                        metadata_description_clone.set_text("");
                        if let Some(description) = metadata
                            .description
                            .as_deref()
                            .and_then(|html| html_to_plain_text(html))
                        {
                            metadata_description_clone.set_text(&description);
                            metadata_description_label_clone.set_visible(true);
                            metadata_description_scroller_clone.set_visible(true);
                            has_content = true;
                        }

                        if let Some(preview) = metadata.preview {
                            match preview {
                                CivitaiPreview::Image(bytes) => {
                                    if let Some(texture) = texture_from_image_bytes(&bytes) {
                                        metadata_picture_clone.set_paintable(Some(&texture));
                                        metadata_picture_clone.set_visible(true);
                                        has_content = true;
                                    } else {
                                        warn!("Failed to decode LoRA preview image.");
                                    }
                                }
                                CivitaiPreview::Video { url } => {
                                    let file = gio::File::for_uri(&url);
                                    let media = MediaFile::for_file(&file);
                                    media.set_loop(true);
                                    media.set_muted(true);
                                    media.play();
                                    metadata_picture_clone.set_paintable(Some(&media));
                                    metadata_picture_clone.set_visible(true);
                                    *active_media_clone.borrow_mut() = Some(media);
                                    has_content = true;
                                }
                            }
                        }

                        clear_flow_box(&metadata_triggers_clone);
                        metadata_triggers_clone.set_visible(false);
                        metadata_triggers_label_clone.set_visible(false);

                        if !metadata.trained_words.is_empty() {
                            for (index, word) in metadata.trained_words.iter().enumerate() {
                                let button = Button::with_label(&word);
                                button.set_halign(Align::Start);
                                button.add_css_class("pill");
                                let overlay = overlay.clone();
                                let word_for_clipboard = word.clone();
                                button.connect_clicked(move |_| {
                                    if let Some(display) = gdk::Display::default() {
                                        let clipboard = display.clipboard();
                                        clipboard.set_text(&word_for_clipboard);
                                    }
                                    let toast_text =
                                        format!("Copied \"{word_for_clipboard}\" to clipboard.");
                                    overlay.add_toast(Toast::new(&toast_text));
                                });
                                metadata_triggers_clone.insert(&button, index as i32);
                            }
                            metadata_triggers_label_clone.set_visible(true);
                            metadata_triggers_clone.set_visible(true);
                            has_content = true;
                        }

                        if !has_content {
                            metadata_status_clone
                                .set_text("No preview data available for this LoRA.");
                            metadata_status_clone.set_visible(true);
                        }

                        metadata_row_clone.set_visible(true);
                    }
                    Ok(Err(err)) => {
                        if current_preview_request_clone.borrow().as_deref()
                            != Some(lora_id.as_str())
                        {
                            return;
                        }
                        if let Some(DownloadError::Unauthorized) =
                            err.downcast_ref::<DownloadError>()
                        {
                            metadata_status_clone.set_text("You are not authorized to view previews. Please paste your Civitai API token above, save it, and try again.");
                            metadata_status_clone.set_visible(true);
                            metadata_row_clone.set_visible(true);
                            metadata_picture_clone.set_paintable(Option::<&gdk::Texture>::None);
                            metadata_picture_clone.set_visible(false);
                            if let Some(media) = active_media_clone.borrow_mut().take() {
                                media.set_playing(false);
                            }
                            clear_flow_box(&metadata_triggers_clone);
                            metadata_triggers_clone.set_visible(false);
                            metadata_triggers_label_clone.set_visible(false);
                            metadata_creator_label_clone.set_visible(false);
                            return;
                        }
                        metadata_picture_clone.set_paintable(Option::<&gdk::Texture>::None);
                        metadata_picture_clone.set_visible(false);
                        if let Some(media) = active_media_clone.borrow_mut().take() {
                            media.set_playing(false);
                        }
                        clear_flow_box(&metadata_triggers_clone);
                        metadata_triggers_clone.set_visible(false);
                        metadata_triggers_label_clone.set_visible(false);
                        metadata_creator_label_clone.set_visible(false);
                        metadata_status_clone.set_text(&format!("Failed to load preview: {err}"));
                        metadata_status_clone.set_visible(true);
                        metadata_row_clone.set_visible(true);
                    }
                    Err(join_err) => {
                        if current_preview_request_clone.borrow().as_deref()
                            != Some(lora_id.as_str())
                        {
                            return;
                        }
                        metadata_picture_clone.set_paintable(Option::<&gdk::Texture>::None);
                        metadata_picture_clone.set_visible(false);
                        if let Some(media) = active_media_clone.borrow_mut().take() {
                            media.set_playing(false);
                        }
                        clear_flow_box(&metadata_triggers_clone);
                        metadata_triggers_clone.set_visible(false);
                        metadata_triggers_label_clone.set_visible(false);
                        metadata_creator_label_clone.set_visible(false);
                        metadata_status_clone.set_text(&format!("Preview task failed: {join_err}"));
                        metadata_status_clone.set_visible(true);
                        metadata_row_clone.set_visible(true);
                    }
                }
            });
        })
    };

    let apply_selection: Rc<dyn Fn()> = {
        let lora_dropdown = lora_dropdown.clone();
        let filtered_loras_ref: Rc<RefCell<Vec<LoraDefinition>>> = Rc::clone(&filtered_loras);
        let download_button = download_button.clone();
        let status_label = status_label.clone();
        let resolved_lora_ref: Rc<RefCell<Option<LoraDefinition>>> = Rc::clone(&resolved_lora);
        let update_preview = update_preview.clone();
        Rc::new(move || {
            let active_id = lora_dropdown.active_id().map(|id| id.to_string());
            if let Some(id) = active_id {
                let lora = filtered_loras_ref
                    .borrow()
                    .iter()
                    .find(|entry| entry.id == id)
                    .cloned();
                if let Some(definition) = lora {
                    let label = match &definition.family {
                        Some(family) => format!("{} • {}", definition.display_name, family),
                        None => definition.display_name.clone(),
                    };
                    status_label.set_label(&label);
                    download_button.set_sensitive(true);
                    update_preview(Some(definition.clone()));
                    *resolved_lora_ref.borrow_mut() = Some(definition);
                } else {
                    status_label.set_label("Select a LoRA to continue.");
                    download_button.set_sensitive(false);
                    *resolved_lora_ref.borrow_mut() = None;
                    update_preview(None);
                }
            } else {
                status_label.set_label("Select a LoRA to continue.");
                download_button.set_sensitive(false);
                *resolved_lora_ref.borrow_mut() = None;
                update_preview(None);
            }
        })
    };

    let refresh_loras: Rc<dyn Fn()> = {
        let all_loras_ref: Rc<Vec<LoraDefinition>> = Rc::clone(&all_loras);
        let family_dropdown = family_dropdown.clone();
        let lora_dropdown = lora_dropdown.clone();
        let status_label = status_label.clone();
        let download_button = download_button.clone();
        let filtered_loras_ref: Rc<RefCell<Vec<LoraDefinition>>> = Rc::clone(&filtered_loras);
        let resolved_lora_ref: Rc<RefCell<Option<LoraDefinition>>> = Rc::clone(&resolved_lora);
        let apply_selection = apply_selection.clone();
        let update_preview = update_preview.clone();
        Rc::new(move || {
            lora_dropdown.remove_all();
            download_button.set_sensitive(false);
            *resolved_lora_ref.borrow_mut() = None;
            update_preview(None);

            let family_filter = family_dropdown
                .active_id()
                .map(|id| id.to_string())
                .filter(|id| !id.is_empty());

            let filtered: Vec<LoraDefinition> = all_loras_ref
                .iter()
                .filter(|lora| match &family_filter {
                    Some(filter) => lora
                        .family
                        .as_ref()
                        .map(|family| family.eq_ignore_ascii_case(filter))
                        .unwrap_or(false),
                    None => true,
                })
                .cloned()
                .collect();

            if filtered.is_empty() {
                status_label.set_label("No LoRAs available for this filter.");
                filtered_loras_ref.borrow_mut().clear();
                update_preview(None);
                return;
            }

            {
                let mut store = filtered_loras_ref.borrow_mut();
                store.clear();
                store.extend(filtered.iter().cloned());
            }

            for (index, lora) in filtered.iter().enumerate() {
                lora_dropdown.append(Some(&lora.id), &lora.label_with_index(index + 1));
            }
            lora_dropdown.set_sensitive(true);
            lora_dropdown.set_active(Some(0));
            status_label.set_label("Select a LoRA to continue.");
            apply_selection();
        })
    };

    refresh_loras();

    {
        let refresh_loras = refresh_loras.clone();
        family_dropdown.connect_changed(move |_| {
            refresh_loras();
        });
    }

    {
        let apply_selection = apply_selection.clone();
        lora_dropdown.connect_changed(move |_| apply_selection());
    }

    {
        let context = context.clone();
        let overlay = overlay.clone();
        let comfy_path_entry = comfy_path_entry.clone();
        let shared_entries = Rc::clone(&shared_entries);
        select_folder_button.connect_clicked(move |_| {
            open_folder_picker(
                context.clone(),
                comfy_path_entry.clone(),
                overlay.clone(),
                Rc::clone(&shared_entries),
            );
        });
    }

    let overlay_for_download = overlay.clone();
    let context_for_download = context.clone();
    let comfy_entry_for_download = comfy_path_entry.clone();
    let progress_box_for_download = progress_box.clone();
    let progress_label_for_download = progress_label.clone();
    let progress_bar_for_download = progress_bar.clone();
    let status_label_for_download = status_label.clone();
    let download_button_for_download = download_button.clone();
    download_button.connect_clicked(move |_| {
        let Some(lora) = resolved_lora.borrow().clone() else {
            overlay_for_download.add_toast(Toast::new("Select a LoRA before downloading."));
            return;
        };

        let comfy_text = comfy_entry_for_download.text();
        if comfy_text.is_empty() {
            overlay_for_download.add_toast(Toast::new("Choose your ComfyUI folder first."));
            return;
        }

        let comfy_path = PathBuf::from(comfy_text.as_str());
        status_label_for_download.set_text(&format!("Downloading {}…", lora.display_name));

        let downloads = context_for_download.downloads.clone();
        let civitai_token = context_for_download.config.settings().civitai_token.clone();
        let overlay_clone = overlay_for_download.clone();
        let lora_name = lora.display_name.clone();

        progress_box_for_download.set_visible(true);
        progress_label_for_download.set_text(&format!("Preparing download for {lora_name}…"));
        progress_bar_for_download.set_fraction(0.0);
        progress_bar_for_download.set_show_text(true);
        progress_bar_for_download.set_text(Some("0%"));
        progress_bar_for_download.set_pulse_step(0.02);
        progress_bar_for_download.pulse();
        download_button_for_download.set_sensitive(false);

        let (progress_sender, progress_receiver) = mpsc::channel::<DownloadSignal>();
        let progress_state = Rc::new(RefCell::new(DownloadProgressState::default()));
        let receiver_cell = Rc::new(RefCell::new(progress_receiver));
        let progress_bar_updates = progress_bar_for_download.clone();
        let progress_label_updates = progress_label_for_download.clone();
        let progress_box_updates = progress_box_for_download.clone();
        let state_for_updates = progress_state.clone();
        let receiver_for_updates = receiver_cell.clone();

        adw::glib::timeout_add_local(Duration::from_millis(50), move || {
            let mut receiver_ref = receiver_for_updates.borrow_mut();
            let receiver = &mut *receiver_ref;
            let mut state = state_for_updates.borrow_mut();

            loop {
                match receiver.try_recv() {
                    Ok(DownloadSignal::Started { artifact, size, .. }) => {
                        state.total = 1;
                        state.entries.insert(
                            0,
                            EntryState {
                                received: 0,
                                size,
                                finished: false,
                            },
                        );
                        state.current_index = Some(0);
                        progress_label_updates.set_text(&format!("Starting {artifact}…"));
                        progress_bar_updates.set_fraction(0.0);
                        update_progress_text(&progress_bar_updates, &state, Some(0.0));
                    }
                    Ok(DownloadSignal::Progress {
                        artifact,
                        received,
                        size,
                        ..
                    }) => {
                        let entry = state.entries.entry(0).or_insert(EntryState {
                            received: 0,
                            size,
                            finished: false,
                        });
                        entry.received = received;
                        entry.size = size;
                        let fraction = state.fraction();
                        if let Some(value) = fraction {
                            progress_bar_updates.set_fraction(value.clamp(0.0, 1.0));
                        }
                        update_progress_text(&progress_bar_updates, &state, fraction);
                        progress_label_updates.set_text(&format!("Downloading {artifact}…"));
                    }
                    Ok(DownloadSignal::Finished { size, .. }) => {
                        if let Some(entry) = state.entries.get_mut(&0) {
                            entry.finished = true;
                            entry.size = size;
                            if let Some(size_bytes) = size {
                                entry.received = size_bytes;
                            }
                        }

                        progress_bar_updates.set_fraction(1.0);
                        update_progress_text(&progress_bar_updates, &state, Some(1.0));
                        progress_label_updates.set_text("LoRA download complete.");
                        progress_box_updates.set_visible(false);
                    }
                    Ok(DownloadSignal::Failed { artifact, error }) => {
                        state.failed = true;
                        progress_label_updates
                            .set_text(&format!("Failed to download {artifact}: {error}"));
                        progress_bar_updates.set_fraction(0.0);
                        progress_bar_updates.set_text(Some("Failed"));
                        progress_box_updates.set_visible(true);
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if !state.is_complete() {
                            state.failed = true;
                            progress_label_updates.set_text("Download interrupted.");
                            progress_bar_updates.set_fraction(0.0);
                            progress_bar_updates.set_text(Some("Interrupted"));
                            progress_box_updates.set_visible(true);
                        }
                        break;
                    }
                }
            }

            if state.failed {
                adw::glib::ControlFlow::Break
            } else if state.is_complete() {
                adw::glib::ControlFlow::Break
            } else {
                adw::glib::ControlFlow::Continue
            }
        });

        let progress_bar_async = progress_bar_for_download.clone();
        let progress_label_async = progress_label_for_download.clone();
        let progress_box_async = progress_box_for_download.clone();
        let status_label_async = status_label_for_download.clone();
        let download_button_async = download_button_for_download.clone();
        adw::glib::MainContext::default().spawn_local(async move {
            let comfy_root_for_summary = comfy_path.clone();
            let handle = downloads.download_lora(comfy_path, lora, civitai_token, progress_sender);
            match handle.await {
                Ok(Ok(outcome)) => {
                    download_button_async.set_sensitive(true);
                    match outcome.status {
                        DownloadStatus::Downloaded => {
                            progress_bar_async.set_fraction(1.0);
                            progress_bar_async.set_text(Some("100%"));
                            progress_label_async.set_text("LoRA download complete.");
                            progress_box_async.set_visible(false);
                            status_label_async.set_text("LoRA download complete.");
                            let toast = format!("Saved {}", outcome.destination.display());
                            overlay_clone.add_toast(Toast::new(&toast));
                            let file_name = outcome
                                .destination
                                .file_name()
                                .map(|name| name.to_string_lossy().to_string())
                                .unwrap_or_else(|| outcome.lora.derived_file_name());
                            let summary_entries = vec![DownloadSummaryEntry {
                                file_name,
                                destination: outcome.destination.clone(),
                            }];
                            show_download_summary_window(
                                overlay_clone.clone(),
                                &comfy_root_for_summary,
                                &summary_entries,
                            );
                        }
                        DownloadStatus::SkippedExisting => {
                            let message = "You already downloaded this LoRA.";
                            progress_label_async.set_text(message);
                            progress_bar_async.set_fraction(0.0);
                            progress_bar_async.set_text(Some("Already downloaded"));
                            progress_box_async.set_visible(false);
                            status_label_async.set_text(message);
                            overlay_clone.add_toast(Toast::new(message));
                        }
                    }
                }
                Ok(Err(err)) => {
                    if let Some(DownloadError::Unauthorized) = err.downcast_ref::<DownloadError>()
                    {
                        let message = "You are not authorized to download. Please paste your Civitai API token above, save it, and try again.";
                        progress_label_async.set_text(message);
                        progress_bar_async.set_fraction(0.0);
                        progress_bar_async.set_text(Some("Unauthorized"));
                        progress_box_async.set_visible(true);
                        download_button_async.set_sensitive(true);
                        status_label_async.set_text(message);
                        overlay_clone.add_toast(Toast::new(message));
                        return;
                    }

                    progress_label_async
                        .set_text(&escape_markup(&format!("Download failed: {err}")));
                    progress_bar_async.set_fraction(0.0);
                    progress_bar_async.set_text(Some("Failed"));
                    progress_box_async.set_visible(true);
                    download_button_async.set_sensitive(true);
                    status_label_async.set_text("LoRA download failed.");
                    overlay_clone.add_toast(Toast::new(&escape_markup(&format!(
                        "Download failed: {err}"
                    ))));
                }
                Err(join_err) => {
                    progress_label_async.set_text(&escape_markup(&format!(
                        "Download task panicked: {join_err}"
                    )));
                    progress_bar_async.set_fraction(0.0);
                    progress_bar_async.set_text(Some("Error"));
                    progress_box_async.set_visible(true);
                    download_button_async.set_sensitive(true);
                    status_label_async.set_text("LoRA download failed.");
                    overlay_clone.add_toast(Toast::new(&escape_markup(&format!(
                        "Download task panicked: {join_err}"
                    ))));
                }
            }
        });
    });

    column.append(&labelled_row("ComfyUI Folder", &comfy_path_entry));
    column.append(&select_folder_button);
    column.append(&labelled_row("Family Filter", &family_dropdown));
    column.append(&labelled_row("LoRA", &lora_dropdown));
    column.append(&metadata_row);
    column.append(&download_button);
    column.append(&progress_box);
    column.append(&status_label);

    column
}

fn texture_from_image_bytes(bytes: &[u8]) -> Option<gdk::Texture> {
    let loader = PixbufLoader::new();
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    let pixbuf = loader.pixbuf()?;
    Some(gdk::Texture::for_pixbuf(&pixbuf))
}

fn clear_flow_box(flow_box: &gtk::FlowBox) {
    while let Some(child) = flow_box.child_at_index(0) {
        flow_box.remove(&child);
    }
}

fn html_to_plain_text(html: &str) -> Option<String> {
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return None;
    }

    let replacements = [
        ("<br />", "\n"),
        ("<br/>", "\n"),
        ("<br>", "\n"),
        ("<p>", ""),
        ("</p>", "\n\n"),
        ("<ul>", ""),
        ("</ul>", "\n"),
        ("<ol>", ""),
        ("</ol>", "\n"),
        ("<li>", "- "),
        ("</li>", "\n"),
    ];

    let mut preprocessed = trimmed.to_string();
    for (from, to) in replacements {
        preprocessed = preprocessed.replace(from, to);
    }

    let mut builder = ammonia::Builder::default();
    builder.tags(HashSet::<&str>::new());
    builder.generic_attributes(HashSet::<&str>::new());
    let cleaned = builder.clean(&preprocessed).to_string();

    let normalized = cleaned
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    let normalized = normalized.trim_matches('\n').to_string();
    let normalized = collapse_blank_lines(&normalized);
    let normalized = strip_icons(&normalized);

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn strip_icons(text: &str) -> String {
    text.chars().filter(|ch| !is_icon(*ch)).collect()
}

fn collapse_blank_lines(text: &str) -> String {
    let mut result = String::new();
    let mut last_blank = true;

    for line in text.lines() {
        let is_blank = line.trim().is_empty();
        if is_blank {
            if last_blank {
                continue;
            }
            result.push('\n');
            last_blank = true;
        } else {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(line.trim_end());
            last_blank = false;
        }
    }

    result.trim_end_matches('\n').to_string()
}

fn is_icon(ch: char) -> bool {
    let code = ch as u32;
    matches!(
        code,
        0x2600..=0x27BF // Misc symbols and dingbats
            | 0xFE00..=0xFE0F // Variation selectors
            | 0x1F000..=0x1FFFF // Misc symbols & emoji planes
    )
}

fn escape_markup(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}

fn build_civitai_token_row(context: &AppContext, overlay: ToastOverlay) -> GtkBox {
    let token_entry = Entry::builder()
        .placeholder_text("Paste your Civitai API token (if required)")
        .hexpand(true)
        .visibility(false)
        .build();
    if let Some(token) = context.config.settings().civitai_token.clone() {
        token_entry.set_text(&token);
    }

    let save_token_button = Button::with_label("Save Token");
    save_token_button.set_halign(Align::Start);

    {
        let context = context.clone();
        let overlay = overlay.clone();
        let token_entry = token_entry.clone();
        save_token_button.connect_clicked(move |_| {
            let raw = token_entry.text();
            let trimmed = raw.trim().to_string();
            let update = context.config.update_settings(|settings| {
                settings.civitai_token = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.clone())
                };
            });

            match update {
                Ok(_) => {
                    let message = if trimmed.is_empty() {
                        "Cleared Civitai token."
                    } else {
                        "Civitai token saved."
                    };
                    overlay.add_toast(Toast::new(message));
                }
                Err(err) => {
                    overlay.add_toast(Toast::new(&format!("Failed to save token: {err}")));
                }
            }
        });
    }

    let token_controls = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .hexpand(true)
        .build();
    token_controls.append(&token_entry);
    token_controls.append(&save_token_button);

    labelled_row("Civitai API Token", &token_controls)
}

fn labelled_row(label: &str, widget: &impl IsA<gtk::Widget>) -> GtkBox {
    let row = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .build();

    let label = Label::builder().label(label).halign(Align::Start).build();

    row.append(&label);
    row.append(widget);
    row
}

fn update_ram_dropdown_for_model(
    context: &AppContext,
    dropdown: &ComboBoxText,
    model_id: Option<String>,
) {
    let snapshot = context.catalog.catalog_snapshot();
    let thresholds = model_id
        .as_deref()
        .and_then(|id| snapshot.find_model(id))
        .map(|model| model.resolved_ram_thresholds())
        .unwrap_or_default();
    rebuild_ram_dropdown(dropdown, &thresholds);
}

fn rebuild_ram_dropdown(dropdown: &ComboBoxText, thresholds: &ResolvedRamTierThresholds) {
    let current = dropdown.active_id().map(|id| id.to_string());
    dropdown.remove_all();
    for tier in RamTier::all() {
        let label = format!("{} ({})", tier.label(), thresholds.range_label(*tier));
        dropdown.append(Some(tier.identifier()), &label);
    }
    if let Some(ref id) = current {
        dropdown.set_active_id(Some(id));
    }
    if dropdown.active().is_none() {
        dropdown.set_active(Some(0));
    }
}

fn build_quant_legend() -> GtkBox {
    let legend = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .build();

    legend.append(&Separator::new(Orientation::Horizontal));
    let guidance = [
        "(S) Tier S: 32 GB+ VRAM  (fp16 UNET - Best Quality - Largest downloads)",
        "(A) Tier A: 16-31 GB VRAM (fp8 UNET - High Quality - Large downloads)",
        "(B) Tier B: 12-15 GB VRAM (GGUF Q4 - Balanced Quality - Medium downloads)",
        "(C) Tier C: <12 GB VRAM   (GGUF Q3 - Preview Quality - Small downloads)",
    ];

    for line in guidance.iter() {
        let label = Label::builder()
            .label(*line)
            .halign(Align::Start)
            .wrap(true)
            .css_classes(vec![String::from("legend-text")])
            .build();
        legend.append(&label);
    }

    let quant_header = Label::builder()
        .use_markup(true)
        .label("<b>Model quantization</b>")
        .halign(Align::Start)
        .css_classes(vec![String::from("legend-text")])
        .build();
    legend.append(&quant_header);

    let quant_description = Label::builder()
        .use_markup(true)
        .label(
            "<i>Reducing the precision of a model's weights (e.g., from 16-bit to 8-bit or 4-bit) to decrease its memory usage and speed up inference.</i>",
        )
        .halign(Align::Start)
        .wrap(true)
        .css_classes(vec![String::from("legend-text")])
        .build();
    legend.append(&quant_description);

    legend
}

fn rebuild_model_dropdown(
    dropdown: &ComboBoxText,
    catalog: &ModelCatalog,
    family_filter: Option<&str>,
    preferred_id: Option<&str>,
) -> bool {
    let filter = family_filter.map(|value| value.to_ascii_lowercase());
    let prefer = preferred_id.map(str::to_string);
    dropdown.remove_all();

    let mut has_models = false;
    let mut preferred_present = false;
    for model in &catalog.models {
        if let Some(ref filter_value) = filter {
            if !model.family.eq_ignore_ascii_case(filter_value) {
                continue;
            }
        }
        dropdown.append(Some(&model.id), &model.display_name);
        has_models = true;
        if let Some(ref prefer_id) = prefer {
            if prefer_id == &model.id {
                preferred_present = true;
            }
        }
    }

    if has_models {
        if preferred_present {
            if let Some(ref prefer_id) = prefer {
                dropdown.set_active_id(Some(prefer_id));
            }
        } else {
            dropdown.set_active(Some(0));
        }
    }

    has_models
}

#[derive(Clone)]
struct DownloadSummaryEntry {
    file_name: String,
    destination: PathBuf,
}

fn show_download_summary_window(
    overlay: ToastOverlay,
    comfy_root: &Path,
    entries: &[DownloadSummaryEntry],
) {
    let Some(parent_window) = overlay
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
    else {
        overlay.add_toast(Toast::new("Could not determine top-level window."));
        return;
    };

    if entries.is_empty() {
        return;
    }

    let summary_window = gtk::Window::builder()
        .transient_for(&parent_window)
        .modal(true)
        .title("Downloads Complete")
        .default_width(520)
        .default_height(360)
        .build();

    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let intro_label = Label::builder()
        .label("The following files were downloaded:")
        .halign(Align::Start)
        .wrap(true)
        .build();
    content.append(&intro_label);

    let list_box = ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .hexpand(true)
        .vexpand(true)
        .build();

    let mut folder_entries: Vec<PathBuf> = Vec::new();

    for entry in entries {
        let file_name = entry.file_name.clone();
        let folder_path = entry
            .destination
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| entry.destination.clone());

        let display_folder = folder_path
            .strip_prefix(comfy_root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| folder_path.display().to_string());

        let row_box = GtkBox::builder()
            .orientation(Orientation::Vertical)
            .spacing(4)
            .build();
        row_box.add_css_class("download-summary-row");

        let file_label = Label::builder()
            .label(file_name)
            .halign(Align::Start)
            .wrap(true)
            .build();

        let folder_label = Label::builder()
            .label(display_folder)
            .halign(Align::Start)
            .wrap(true)
            .build();
        folder_label.add_css_class("caption");

        row_box.append(&file_label);
        row_box.append(&folder_label);

        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&row_box));
        list_box.append(&row);

        folder_entries.push(folder_path);
    }

    let scroller = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&list_box)
        .build();
    content.append(&scroller);

    let buttons_row = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(6)
        .halign(Align::End)
        .build();

    let open_button = Button::with_label("Open Folder");
    open_button.set_sensitive(false);
    let close_button = Button::with_label("Close");

    let summary_window_for_close = summary_window.clone();
    close_button.connect_clicked(move |_| {
        summary_window_for_close.close();
    });

    let folder_entries = Rc::new(folder_entries);
    let list_box_for_open = list_box.clone();
    let overlay_for_open = overlay.clone();
    open_button.connect_clicked(move |_| {
        if let Some(row) = list_box_for_open.selected_row() {
            let index = row.index();
            if index >= 0 {
                if let Some(folder) = folder_entries.get(index as usize) {
                    let file = gio::File::for_path(folder);
                    let uri = file.uri();
                    if let Err(err) = gio::AppInfo::launch_default_for_uri(
                        uri.as_str(),
                        None::<&gio::AppLaunchContext>,
                    ) {
                        let message = format!("Failed to open folder {}: {err}", folder.display());
                        overlay_for_open.add_toast(Toast::new(&message));
                        adw::glib::g_warning!(crate::app::APP_ID, "{message}");
                    }
                }
            }
        }
    });

    let open_button_for_selection = open_button.clone();
    list_box.connect_row_selected(move |_, row| {
        open_button_for_selection.set_sensitive(row.is_some());
    });

    if let Some(row) = list_box.row_at_index(0) {
        list_box.select_row(Some(&row));
    }

    buttons_row.append(&close_button);
    buttons_row.append(&open_button);
    content.append(&buttons_row);

    summary_window.set_child(Some(&content));
    summary_window.present();
}

#[derive(Clone, Debug)]
struct EntryState {
    received: u64,
    size: Option<u64>,
    finished: bool,
}

struct DownloadProgressState {
    total: usize,
    entries: HashMap<usize, EntryState>,
    failed: bool,
    current_index: Option<usize>,
}

struct CancellationHandle {
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
}

impl CancellationHandle {
    fn cancel(mut self) {
        if let Some(sender) = self.cancel.take() {
            let _ = sender.send(());
        }
    }
}

impl Default for DownloadProgressState {
    fn default() -> Self {
        Self {
            total: 0,
            entries: HashMap::new(),
            failed: false,
            current_index: None,
        }
    }
}

impl DownloadProgressState {
    fn fraction(&self) -> Option<f64> {
        if self.entries.is_empty() {
            return Some(0.0);
        }

        let mut total_bytes: u64 = 0;
        let mut received_bytes: u64 = 0;

        for entry in self.entries.values() {
            if let Some(size) = entry.size {
                total_bytes += size;
                received_bytes += entry.received.min(size);
            } else {
                return None;
            }
        }

        if total_bytes == 0 {
            None
        } else {
            Some(received_bytes as f64 / total_bytes as f64)
        }
    }

    fn byte_progress(&self) -> (u64, Option<u64>) {
        if let Some(index) = self.current_index {
            if let Some(entry) = self.entries.get(&index) {
                return (entry.received, entry.size);
            }
        }
        (0, None)
    }

    fn is_complete(&self) -> bool {
        self.total > 0
            && self.entries.len() == self.total
            && self.entries.values().all(|entry| entry.finished)
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GB", value / GB)
    } else if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn update_progress_text(
    bar: &gtk::ProgressBar,
    state: &DownloadProgressState,
    fraction: Option<f64>,
) {
    let (received, total_opt) = state.byte_progress();
    let text = match (fraction, total_opt) {
        (Some(frac), Some(total)) if total > 0 => format!(
            "{:.0}% • {} / {}",
            (frac * 100.0),
            format_bytes(received),
            format_bytes(total)
        ),
        (Some(frac), _) => format!(
            "{:.0}% • {} downloaded",
            (frac * 100.0),
            format_bytes(received)
        ),
        (None, Some(total)) if total > 0 => {
            format!("{} / {}", format_bytes(received), format_bytes(total))
        }
        _ => format!("{} downloaded", format_bytes(received)),
    };
    bar.set_text(Some(&text));
}

fn open_folder_picker(
    context: AppContext,
    entry: Entry,
    overlay: ToastOverlay,
    shared_entries: Rc<RefCell<Vec<Entry>>>,
) {
    let window = match overlay
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
    {
        Some(window) => window,
        None => {
            overlay.add_toast(Toast::new("Could not determine top-level window."));
            return;
        }
    };

    let chooser = FileChooserNative::builder()
        .title("Select ComfyUI Folder")
        .accept_label("Select")
        .cancel_label("Cancel")
        .modal(true)
        .transient_for(&window)
        .action(FileChooserAction::SelectFolder)
        .build();

    if let Some(current) = context.config.settings().comfyui_root.clone() {
        let _ = chooser.set_current_folder(Some(&gio::File::for_path(current)));
    }

    let overlay_clone = overlay.clone();
    let chooser_keepalive = chooser.clone();
    let entry_clone = entry.clone();
    let context_clone = context.clone();
    let shared_entries_clone = Rc::clone(&shared_entries);
    chooser.connect_response(move |dialog, response| {
        let _keepalive = chooser_keepalive.clone();
        let overlay = overlay_clone.clone();
        let entry = entry_clone.clone();
        let context = context_clone.clone();
        let shared_entries = Rc::clone(&shared_entries_clone);

        if response == ResponseType::Accept {
            if let Some(file) = dialog.file() {
                if let Some(path) = file.path() {
                    let text = path.to_string_lossy().to_string();
                    entry.set_text(&text);
                    for other in shared_entries.borrow().iter() {
                        other.set_text(&text);
                    }
                    if let Err(err) = context.config.update_settings(|settings| {
                        settings.comfyui_root = Some(path.clone());
                    }) {
                        let message = format!("Failed to save download folder: {err}");
                        overlay.add_toast(Toast::new(&message));
                    } else {
                        overlay.add_toast(Toast::new("Download folder saved."));
                    }
                }
            }
        }

        dialog.destroy();
    });

    chooser.show();
}
