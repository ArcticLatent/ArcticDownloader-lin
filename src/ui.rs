use crate::{
    app::AppContext,
    download::{DownloadSignal, DownloadStatus},
    model::{LoraDefinition, ResolvedModel},
    vram::VramTier,
};
use adw::gio;
use adw::gtk::{
    self, cairo, gdk, prelude::*, Align, Box as GtkBox, Button, ComboBoxText, Entry,
    FileChooserAction, FileChooserNative, Image, Label, Orientation, ResponseType, Separator,
};
use adw::{Application, ApplicationWindow, HeaderBar, Toast, ToastOverlay, WindowTitle};
use anyhow::Result;
use gdk_pixbuf::{Colorspace, Pixbuf};
use std::{
    cell::RefCell,
    collections::HashMap,
    f64::consts::PI,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc::{self, TryRecvError},
    time::Duration,
};

pub fn bootstrap(app: &Application, context: AppContext) -> Result<()> {
    if let Err(err) = adw::init() {
        adw::glib::g_warning!(crate::app::APP_ID, "failed to initialize Adwaita: {err}");
    }

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Arctic Downloader")
        .default_width(960)
        .default_height(720)
        .content(&build_shell(&context))
        .build();

    window.present();
    Ok(())
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

    root.append(&build_header());
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

fn build_header() -> HeaderBar {
    let title = WindowTitle::builder()
        .title("Arctic Downloader")
        .subtitle("ComfyUI Asset Helper by Arctic Latent")
        .build();

    HeaderBar::builder().title_widget(&title).build()
}

fn build_main_controls(context: &AppContext, overlay: ToastOverlay) -> GtkBox {
    let column = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .build();

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
            match context.config.update_settings(|settings| {
                settings.civitai_token = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.clone())
                };
            }) {
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
    column.append(&labelled_row("Civitai API Token", &token_controls));

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

    let model_dropdown = ComboBoxText::new();
    let mut has_models = false;
    for model in catalog.models {
        model_dropdown.append(Some(&model.id), &model.display_name);
        has_models = true;
    }
    if has_models {
        model_dropdown.set_active(Some(0));
    }

    let vram_dropdown = ComboBoxText::new();
    for tier in VramTier::all() {
        vram_dropdown.append(Some(&tier.gigabytes().to_string()), &tier.to_string());
    }
    vram_dropdown.set_active(Some(0));

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
                        status_label.set_label(&format!(
                            "{} • {} (requires ≥ {} GB)",
                            resolved.master.display_name,
                            resolved.variant.summary(),
                            resolved.variant.min_vram_gb
                        ));
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
                .and_then(|id| id.parse::<u32>().ok())
                .and_then(VramTier::from_gigabytes);
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
        model_dropdown.connect_changed(move |_| {
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
        let progress_box = progress_box_clone.clone();
        let progress_label = progress_label_clone.clone();
        let progress_bar = progress_bar_clone.clone();
        let status_label = status_label_clone_for_button.clone();
        let download_button_clone = download_button.clone();
        download_button.connect_clicked(move |_| {
            let Some(resolved) = resolved_variant.borrow().clone() else {
                overlay.add_toast(Toast::new("Select a variant before downloading."));
                return;
            };

            let comfy_text = comfy_path_entry.text();
            if comfy_text.is_empty() {
                overlay.add_toast(Toast::new("Choose your ComfyUI folder first."));
                return;
            }

            let comfy_path = PathBuf::from(comfy_text.as_str());

            status_label.set_text(&format!(
                "Checking existing files for {}…",
                resolved.master.display_name
            ));

            let all_present = resolved.variant.artifacts.iter().all(|artifact| {
                comfy_path
                    .join(artifact.target_category.comfyui_subdir())
                    .join(artifact.file_name())
                    .exists()
            });

            if all_present {
                let message = format!(
                    "All artifacts for {} are already downloaded.",
                    resolved.master.display_name
                );
                status_label.set_text(&message);
                overlay.add_toast(Toast::new(&message));
                return;
            }

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
            download_button_clone.set_sensitive(false);
            status_label.set_text(&format!(
                "Downloading artifacts for {}…",
                resolved.master.display_name
            ));

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
                            progress_label_updates.set_text(&format!("Starting {artifact}…"));
                            progress_bar_updates.set_fraction(0.0);
                            progress_bar_updates.set_text(Some("0%"));
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
                            if let Some(fraction) = state.fraction() {
                                progress_bar_updates.set_fraction(fraction.clamp(0.0, 1.0));
                                progress_bar_updates
                                    .set_text(Some(&format!("{:.0}%", fraction * 100.0)));
                            }
                            progress_label_updates.set_text(&format!("Downloading {artifact}…"));
                        }
                        Ok(DownloadSignal::Finished { index, size }) => {
                            if let Some(entry) = state.entries.get_mut(&index) {
                                entry.finished = true;
                                entry.size = size;
                                if let Some(size_bytes) = size {
                                    entry.received = size_bytes;
                                }
                            }

                            if let Some(fraction) = state.fraction() {
                                progress_bar_updates.set_fraction(fraction.clamp(0.0, 1.0));
                                progress_bar_updates
                                    .set_text(Some(&format!("{:.0}%", fraction * 100.0)));
                            }

                            if state.is_complete() {
                                progress_label_updates.set_text("All downloads complete.");
                                progress_bar_updates.set_fraction(1.0);
                                progress_bar_updates.set_text(Some("100%"));
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

            adw::glib::MainContext::default().spawn_local(async move {
                let start_message = format!("Downloading {} artifacts…", master_name);
                overlay_clone.add_toast(Toast::new(&start_message));

                let handle = downloads.download_variant(comfy_path, resolved, progress_sender);
                match handle.await {
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
                    }
                    Ok(Err(err)) => {
                        progress_label_async.set_text(&format!("Download failed: {err}"));
                        progress_bar_async.set_fraction(0.0);
                        progress_bar_async.set_text(Some("Failed"));
                        progress_box_async.set_visible(true);
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
                        download_button_async.set_sensitive(true);
                    }
                }
            });
        });
    }

    refresh_variants();

    column.append(&labelled_row("Master Model", &model_dropdown));
    column.append(&labelled_row("GPU VRAM", &vram_dropdown));
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
        .spacing(12)
        .halign(Align::End)
        .build();
    links_box.set_hexpand(true);
    links_box.set_margin_top(12);

    let youtube_button = Button::builder()
        .tooltip_text("Open Arctic Latent on YouTube")
        .build();
    youtube_button.add_css_class("flat");
    let youtube_image = load_image_or_fallback("youtube", create_youtube_icon);
    youtube_image.set_size_request(32, 32);
    youtube_button.set_child(Some(&youtube_image));

    let github_button = Button::builder()
        .tooltip_text("Open Arctic Latent on GitHub")
        .build();
    github_button.add_css_class("flat");
    let github_image = load_image_or_fallback("github", create_github_icon);
    github_image.set_size_request(32, 32);
    github_button.set_child(Some(&github_image));

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

    links_box.append(&youtube_button);
    links_box.append(&github_button);
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

    let apply_selection: Rc<dyn Fn()> = {
        let lora_dropdown = lora_dropdown.clone();
        let filtered_loras_ref: Rc<RefCell<Vec<LoraDefinition>>> = Rc::clone(&filtered_loras);
        let download_button = download_button.clone();
        let status_label = status_label.clone();
        let resolved_lora_ref: Rc<RefCell<Option<LoraDefinition>>> = Rc::clone(&resolved_lora);
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
                    *resolved_lora_ref.borrow_mut() = Some(definition);
                } else {
                    status_label.set_label("Select a LoRA to continue.");
                    download_button.set_sensitive(false);
                    *resolved_lora_ref.borrow_mut() = None;
                }
            } else {
                status_label.set_label("Select a LoRA to continue.");
                download_button.set_sensitive(false);
                *resolved_lora_ref.borrow_mut() = None;
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
        Rc::new(move || {
            lora_dropdown.remove_all();
            download_button.set_sensitive(false);
            *resolved_lora_ref.borrow_mut() = None;

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
                        progress_label_updates.set_text(&format!("Starting {artifact}…"));
                        progress_bar_updates.set_fraction(0.0);
                        progress_bar_updates.set_text(Some("0%"));
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
                        if let Some(fraction) = state.fraction() {
                            progress_bar_updates.set_fraction(fraction.clamp(0.0, 1.0));
                            progress_bar_updates
                                .set_text(Some(&format!("{:.0}%", fraction * 100.0)));
                        }
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
                        progress_bar_updates.set_text(Some("100%"));
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
            let start_message = format!("Downloading {lora_name}…");
            overlay_clone.add_toast(Toast::new(&start_message));

            let handle = downloads.download_lora(comfy_path, lora, civitai_token, progress_sender);
            match handle.await {
                Ok(Ok(outcome)) => {
                    progress_bar_async.set_fraction(1.0);
                    progress_bar_async.set_text(Some("100%"));
                    progress_label_async.set_text("LoRA download complete.");
                    progress_box_async.set_visible(false);
                    download_button_async.set_sensitive(true);
                    status_label_async.set_text("LoRA download complete.");

                    let toast = match outcome.status {
                        DownloadStatus::Downloaded => {
                            format!("Saved {}", outcome.destination.display())
                        }
                        DownloadStatus::SkippedExisting => {
                            format!("Skipped existing {}", outcome.destination.display())
                        }
                    };
                    overlay_clone.add_toast(Toast::new(&toast));
                }
                Ok(Err(err)) => {
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

    column.append(&labelled_row("Family Filter", &family_dropdown));
    column.append(&labelled_row("LoRA", &lora_dropdown));
    column.append(&labelled_row("ComfyUI Folder", &comfy_path_entry));
    column.append(&select_folder_button);
    column.append(&download_button);
    column.append(&progress_box);
    column.append(&status_label);

    column
}

fn escape_markup(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
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

fn build_quant_legend() -> GtkBox {
    let legend = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(6)
        .build();

    let title = Label::builder()
        .label("Quantization Legend")
        .halign(Align::Start)
        .css_classes(vec![String::from("heading")])
        .build();

    let grid = gtk::Grid::builder()
        .column_spacing(18)
        .row_spacing(6)
        .hexpand(true)
        .build();

    let headers = ["Suffix", "Size", "Quality", "Speed"];
    for (col, header) in headers.into_iter().enumerate() {
        let header_label = Label::builder()
            .label(header)
            .halign(Align::Start)
            .css_classes(vec![String::from("heading")])
            .build();
        grid.attach(&header_label, col as i32, 0, 1, 1);
    }

    let rows = [
        ("O", "Baseline", "Lowest", "Fastest"),
        ("S", "Small", "Low", "Fast"),
        ("M", "Medium", "Medium", "Moderate"),
        ("L", "Large", "Highest", "Slowest"),
    ];

    for (row_idx, (suffix, size, quality, speed)) in rows.into_iter().enumerate() {
        let row = row_idx as i32 + 1;

        let suffix_label = Label::builder()
            .label(suffix)
            .halign(Align::Start)
            .css_classes(vec![String::from("monospace")])
            .build();
        grid.attach(&suffix_label, 0, row, 1, 1);

        let size_label = Label::builder().label(size).halign(Align::Start).build();
        grid.attach(&size_label, 1, row, 1, 1);

        let quality_label = Label::builder().label(quality).halign(Align::Start).build();
        grid.attach(&quality_label, 2, row, 1, 1);

        let speed_label = Label::builder().label(speed).halign(Align::Start).build();
        grid.attach(&speed_label, 3, row, 1, 1);
    }

    legend.append(&Separator::new(Orientation::Horizontal));
    legend.append(&title);
    legend.append(&grid);
    legend
}

fn load_image_or_fallback<F>(name: &str, fallback: F) -> Image
where
    F: Fn() -> Image,
{
    for path in candidate_image_paths(name) {
        if path.exists() {
            let file = gio::File::for_path(&path);
            match gdk::Texture::from_file(&file) {
                Ok(texture) => {
                    return Image::from_paintable(Some(&texture));
                }
                Err(err) => {
                    adw::glib::g_warning!(
                        crate::app::APP_ID,
                        "Failed to load {name} image at {}: {err}",
                        path.display()
                    );
                }
            }
        }
    }
    fallback()
}

fn candidate_image_paths(name: &str) -> Vec<PathBuf> {
    let directories = [
        "assets/branding",
        "assets/icons",
        "assets",
        "data/branding",
        "data/icons",
        "data",
    ];
    let extensions = ["png", "svg", "jpg", "jpeg", "webp"];
    let mut paths = Vec::new();
    for dir in directories {
        for ext in &extensions {
            paths.push(Path::new(dir).join(format!("{name}.{ext}")));
        }
    }
    paths
}

fn create_youtube_icon() -> Image {
    create_surface_image(48, 48, |cr, width, height| {
        let radius = (width.min(height)) * 0.45;
        let cx = width / 2.0;
        let cy = height / 2.0;

        cr.set_source_rgba(1.0, 0.0, 0.0, 1.0);
        cr.arc(cx, cy, radius, 0.0, 2.0 * PI);
        let _ = cr.fill();

        cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        cr.move_to(cx + radius * 0.55, cy);
        cr.line_to(cx - radius * 0.3, cy - radius * 0.65);
        cr.line_to(cx - radius * 0.3, cy + radius * 0.65);
        cr.close_path();
        let _ = cr.fill();
    })
}

fn create_github_icon() -> Image {
    create_surface_image(48, 48, |cr, width, height| {
        let radius = (width.min(height)) * 0.45;
        let cx = width / 2.0;
        let cy = height / 2.0;

        cr.set_source_rgba(
            0x17 as f64 / 255.0,
            0x15 as f64 / 255.0,
            0x15 as f64 / 255.0,
            1.0,
        );
        cr.arc(cx, cy, radius, 0.0, 2.0 * PI);
        let _ = cr.fill();

        cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        cr.set_font_size(radius * 0.9);
        let text = "GH";
        if let Ok(extents) = cr.text_extents(text) {
            let width = extents.width();
            let height = extents.height();
            let x_bearing = extents.x_bearing();
            let y_bearing = extents.y_bearing();
            let x = cx - (width / 2.0 + x_bearing);
            let y = cy - (height / 2.0 + y_bearing);
            cr.move_to(x, y);
            let _ = cr.show_text(text);
        }
    })
}

fn create_surface_image<F>(width: i32, height: i32, draw: F) -> Image
where
    F: Fn(&cairo::Context, f64, f64),
{
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .unwrap_or_else(|err| {
            panic!("failed to create Cairo surface ({width}x{height}): {err}");
        });
    let cr = cairo::Context::new(&surface).expect("failed to create Cairo context");
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    let _ = cr.paint();
    draw(&cr, width as f64, height as f64);
    surface.flush();

    let stride = surface.stride();
    let width_px = surface.width();
    let height_px = surface.height();
    let copy_len = (stride as usize) * (height_px as usize);
    let data_vec = surface
        .data()
        .map(|data| data.as_ref().to_vec())
        .unwrap_or_else(|_| vec![0; copy_len]);

    let pixbuf = Pixbuf::from_mut_slice(
        data_vec,
        Colorspace::Rgb,
        true,
        8,
        width_px,
        height_px,
        stride,
    );
    let texture = gdk::Texture::for_pixbuf(&pixbuf);
    Image::from_paintable(Some(&texture))
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
}

impl Default for DownloadProgressState {
    fn default() -> Self {
        Self {
            total: 0,
            entries: HashMap::new(),
            failed: false,
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

    fn is_complete(&self) -> bool {
        self.total > 0
            && self.entries.len() == self.total
            && self.entries.values().all(|entry| entry.finished)
    }
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
    let entry_clone = entry.clone();
    let context_clone = context.clone();
    let shared_entries_clone = Rc::clone(&shared_entries);
    chooser.connect_response(move |dialog, response| {
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
