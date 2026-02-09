use crate::{
    app::AppContext,
    download::{CivitaiModelMetadata, CivitaiPreview, DownloadSignal, DownloadStatus},
    env_flags::auto_update_enabled,
    model::{LoraDefinition, ModelCatalog, ResolvedModel, ResolvedRamTierThresholds},
    preview,
    ram::RamTier,
    vram::VramTier,
};
use anyhow::Result;
use image::load_from_memory;
use rfd::FileDialog;
use slint::{
    Image as SlintImage, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel, Weak,
};
use std::{
    path::PathBuf,
    rc::Rc,
    sync::{mpsc, Arc, Mutex},
};

slint::slint! {
import { Button, ComboBox, HorizontalBox, LineEdit, ScrollView, VerticalBox } from "std-widgets.slint";

export component MainWindow inherits Window {
    title: "Arctic Downloader";
    width: 980px;
    height: 760px;
    default-font-family: "Inter";

    in-out property <string> version_text;
    in-out property <string> status_text;
    in-out property <string> progress_text;
    in-out property <string> comfy_path;
    in-out property <string> civitai_token;
    in-out property <string> update_state_text;
    in-out property <string> lora_creator_text;
    in-out property <string> lora_strength_text;
    in-out property <string> lora_triggers_text;
    in-out property <string> lora_description_text;
    in-out property <image> lora_preview_image;
    in-out property <string> lora_preview_caption;
    in-out property <[string]> download_summary_entries;
    in-out property <int> download_summary_index;
    in-out property <bool> download_summary_visible;

    in-out property <[string]> family_options;
    in-out property <[string]> model_options;
    in-out property <[string]> vram_options;
    in-out property <[string]> ram_options;
    in-out property <[string]> variant_options;
    in-out property <[string]> lora_family_options;
    in-out property <[string]> lora_options;

    in-out property <int> family_index;
    in-out property <int> model_index;
    in-out property <int> vram_index;
    in-out property <int> ram_index;
    in-out property <int> variant_index;
    in-out property <int> lora_family_index;
    in-out property <int> lora_index;
    in-out property <int> active_tab;

    callback choose_folder();
    callback family_changed();
    callback model_changed();
    callback vram_changed();
    callback ram_changed();
    callback variant_changed();
    callback lora_family_changed();
    callback lora_changed();
    callback download_model_assets();
    callback download_lora_asset();
    callback save_token();
    callback open_last_folder();
    callback open_lora_creator();
    callback open_summary_folder();
    callback clear_summary();
    callback check_updates_now();

    changed family_index => { root.family_changed(); }
    changed model_index => { root.model_changed(); }
    changed vram_index => { root.vram_changed(); }
    changed ram_index => { root.ram_changed(); }
    changed variant_index => { root.variant_changed(); }
    changed lora_family_index => { root.lora_family_changed(); }
    changed lora_index => { root.lora_changed(); }

    VerticalBox {
        spacing: 10px;

        Text {
            text: "Arctic Downloader";
            font-size: 26px;
            font-weight: 700;
            vertical-stretch: 0;
        }

        Text {
            text: "Version " + root.version_text;
            color: #556070;
            vertical-stretch: 0;
        }

        HorizontalBox {
            spacing: 8px;
            vertical-stretch: 0;
            Button {
                text: "Models";
                height: 32px;
                width: 120px;
                enabled: root.active_tab != 0;
                clicked => { root.active_tab = 0; }
            }
            Button {
                text: "LoRAs";
                height: 32px;
                width: 120px;
                enabled: root.active_tab != 1;
                clicked => { root.active_tab = 1; }
            }
        }

        ScrollView {
            GridLayout {
                spacing: 8px;
                spacing-vertical: 8px;
                spacing-horizontal: 8px;

                if root.active_tab == 0 : Row {
                    Button {
                        text: "Check Updates Now";
                        height: 36px;
                        clicked => { root.check_updates_now(); }
                    }
                }

                if root.active_tab == 0 : Row {
                    Text {
                        text: "Update: " + root.update_state_text;
                        color: #556070;
                    }
                }

                if root.active_tab == 0 : Row {
                    LineEdit {
                        text <=> root.comfy_path;
                        placeholder-text: "Select your ComfyUI root folder";
                        height: 36px;
                        horizontal-stretch: 1;
                    }
                    Button {
                        text: "Choose Folder";
                        height: 36px;
                        clicked => { root.choose_folder(); }
                    }
                    Button {
                        text: "Open Last Folder";
                        height: 36px;
                        clicked => { root.open_last_folder(); }
                    }
                }

                if root.active_tab == 0 : Row {
                    ComboBox { model: root.family_options; current-index <=> root.family_index; height: 36px; horizontal-stretch: 1; }
                    ComboBox { model: root.model_options; current-index <=> root.model_index; height: 36px; horizontal-stretch: 1; }
                    ComboBox { model: root.vram_options; current-index <=> root.vram_index; height: 36px; horizontal-stretch: 1; }
                    ComboBox { model: root.ram_options; current-index <=> root.ram_index; height: 36px; horizontal-stretch: 1; }
                    ComboBox { model: root.variant_options; current-index <=> root.variant_index; height: 36px; horizontal-stretch: 1; }
                }

                if root.active_tab == 0 : Row {
                    Button {
                        text: "Download Model Assets";
                        height: 36px;
                        clicked => { root.download_model_assets(); }
                    }
                }

                if root.active_tab == 1 : Row {
                    Button {
                        text: "Check Updates Now";
                        height: 36px;
                        clicked => { root.check_updates_now(); }
                    }
                }

                if root.active_tab == 1 : Row {
                    Text {
                        text: "Update: " + root.update_state_text;
                        color: #556070;
                    }
                }

                if root.active_tab == 1 : Row {
                    LineEdit {
                        text <=> root.comfy_path;
                        placeholder-text: "Select your ComfyUI root folder";
                        height: 36px;
                        horizontal-stretch: 1;
                    }
                    Button {
                        text: "Choose Folder";
                        height: 36px;
                        clicked => { root.choose_folder(); }
                    }
                    Button {
                        text: "Open Last Folder";
                        height: 36px;
                        clicked => { root.open_last_folder(); }
                    }
                }

                if root.active_tab == 1 : Row {
                    ComboBox { model: root.lora_family_options; current-index <=> root.lora_family_index; height: 36px; horizontal-stretch: 1; }
                    ComboBox { model: root.lora_options; current-index <=> root.lora_index; height: 36px; horizontal-stretch: 1; }
                    Button {
                        text: "Download LoRA";
                        height: 36px;
                        clicked => { root.download_lora_asset(); }
                    }
                }

                if root.active_tab == 1 : Row {
                    LineEdit {
                        text <=> root.civitai_token;
                        placeholder-text: "Civitai API token (optional)";
                        height: 36px;
                        horizontal-stretch: 1;
                    }
                    Button {
                        text: "Save Token";
                        height: 36px;
                        clicked => { root.save_token(); }
                    }
                    Button {
                        text: "Open Creator Page";
                        height: 36px;
                        clicked => { root.open_lora_creator(); }
                    }
                }

                if root.active_tab == 1 : Row {
                    Text {
                        text: "LoRA Creator: " + root.lora_creator_text;
                        wrap: word-wrap;
                    }
                }

                if root.active_tab == 1 : Row {
                    Text {
                        text: "Recommended Strength: " + root.lora_strength_text;
                        wrap: word-wrap;
                    }
                }

                if root.active_tab == 1 : Row {
                    Text {
                        text: "Trigger Words: " + root.lora_triggers_text;
                        wrap: word-wrap;
                    }
                }

                if root.active_tab == 1 : Row {
                    Text {
                        text: "Description: " + root.lora_description_text;
                        wrap: word-wrap;
                    }
                }

                if root.active_tab == 1 : Row {
                    Image {
                        source: root.lora_preview_image;
                        width: 560px;
                        height: 320px;
                        image-fit: contain;
                    }
                }

                if root.active_tab == 1 : Row {
                    Text {
                        text: root.lora_preview_caption;
                        color: #556070;
                        wrap: word-wrap;
                    }
                }

                Row {
                    Text {
                        text: root.progress_text;
                        color: #3f4c5e;
                        wrap: word-wrap;
                    }
                }

                Row {
                    Text {
                        text: root.status_text;
                        wrap: word-wrap;
                    }
                }

                if root.download_summary_visible : Row {
                    VerticalBox {
                        spacing: 6px;
                        Text {
                            text: "Downloads Complete";
                            font-size: 16px;
                            font-weight: 600;
                        }
                        ComboBox {
                            model: root.download_summary_entries;
                            current-index <=> root.download_summary_index;
                        }
                        HorizontalBox {
                            spacing: 8px;
                            Button {
                                text: "Open Selected Folder";
                                clicked => { root.open_summary_folder(); }
                            }
                            Button {
                                text: "Dismiss Summary";
                                clicked => { root.clear_summary(); }
                            }
                        }
                    }
                }
            }
        }
    }
}
}

#[derive(Clone)]
struct Choice {
    id: String,
    label: String,
}

#[derive(Clone)]
struct DownloadSummaryEntry {
    label: String,
    folder: PathBuf,
}

#[derive(Clone)]
struct UiState {
    model_families: Vec<Choice>,
    models: Vec<Choice>,
    variants: Vec<Choice>,
    lora_families: Vec<Choice>,
    loras: Vec<Choice>,
    selected_model: Option<ResolvedModel>,
    selected_lora_id: Option<String>,
    lora_preview_url: Option<String>,
    lora_creator_url: Option<String>,
    download_summary: Vec<DownloadSummaryEntry>,
    last_download_folder: Option<PathBuf>,
}

pub fn run(context: AppContext) -> Result<()> {
    let ui = MainWindow::new()?;
    let mut state = build_state(&context);

    ui.set_version_text(context.display_version.clone().into());
    ui.set_status_text(initial_status(&context).into());
    ui.set_progress_text("Idle".into());
    ui.set_update_state_text("Idle".into());
    ui.set_lora_creator_text("Select a LoRA to load details.".into());
    ui.set_lora_strength_text("-".into());
    ui.set_lora_triggers_text("-".into());
    ui.set_lora_description_text("-".into());
    ui.set_lora_preview_image(SlintImage::default());
    ui.set_lora_preview_caption("No preview loaded.".into());
    ui.set_download_summary_entries(to_model_strings(Vec::new()));
    ui.set_download_summary_index(-1);
    ui.set_download_summary_visible(false);

    let comfy_root = context
        .config
        .settings()
        .comfyui_root
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    ui.set_comfy_path(comfy_root.into());

    if let Some(token) = context.config.settings().civitai_token {
        ui.set_civitai_token(token.into());
    }

    ui.set_vram_options(to_model_strings(
        VramTier::all()
            .iter()
            .map(|tier| tier.description().to_string())
            .collect(),
    ));
    ui.set_ram_options(to_model_strings(
        RamTier::all()
            .iter()
            .map(|tier| tier.description().to_string())
            .collect(),
    ));

    ui.set_family_options(to_choice_model(&state.model_families));
    ui.set_model_options(to_choice_model(&state.models));
    ui.set_variant_options(to_choice_model(&state.variants));
    ui.set_lora_family_options(to_choice_model(&state.lora_families));
    ui.set_lora_options(to_choice_model(&state.loras));

    ui.set_family_index(0);
    ui.set_model_index(if state.models.is_empty() { -1 } else { 0 });
    ui.set_vram_index(0);
    ui.set_ram_index(default_ram_index(&context));
    ui.set_variant_index(if state.variants.is_empty() { -1 } else { 0 });
    ui.set_lora_family_index(0);
    ui.set_lora_index(if state.loras.is_empty() { -1 } else { 0 });
    ui.set_active_tab(0);

    refresh_variants(&context, &ui, &mut state);

    let state = Arc::new(Mutex::new(state));
    wire_callbacks(&ui, context.clone(), Arc::clone(&state));
    prime_lora_metadata(&ui, &context, Arc::clone(&state));
    kickoff_update_check(&ui, &context);

    ui.run()?;
    Ok(())
}

fn kickoff_update_check(ui: &MainWindow, context: &AppContext) {
    if !auto_update_enabled() {
        return;
    }

    run_update_check(ui.as_weak(), context.clone(), false);
}

fn run_update_check(weak: Weak<MainWindow>, context: AppContext, announce_no_update: bool) {
    let runtime = context.runtime.clone();
    let updater = context.updater.clone();

    std::thread::spawn(move || {
        let weak_for_checking = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak_for_checking.upgrade() {
                ui.set_update_state_text("Checking".into());
            }
        });

        let check_result = runtime.block_on(async {
            let check_handle = updater.check_for_update();
            check_handle.await
        });

        let available = match check_result {
            Ok(Ok(Some(update))) => update,
            Ok(Ok(None)) => {
                if announce_no_update {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak.upgrade() {
                            ui.set_status_text("No updates available.".into());
                            ui.set_update_state_text("Up to date".into());
                        }
                    });
                } else {
                    let weak_for_uptodate = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak_for_uptodate.upgrade() {
                            ui.set_update_state_text("Up to date".into());
                        }
                    });
                }
                return;
            }
            Ok(Err(err)) => {
                let message = format!("Update check failed: {err:#}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_status_text(message.into());
                        ui.set_update_state_text("Error".into());
                    }
                });
                return;
            }
            Err(join_err) => {
                let message = format!("Update check task failed: {join_err}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_status_text(message.into());
                        ui.set_update_state_text("Error".into());
                    }
                });
                return;
            }
        };

        let version_text = available.version.to_string();
        let weak_for_start = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak_for_start.upgrade() {
                ui.set_status_text(format!("Update found: v{version_text}. Downloading...").into());
                ui.set_update_state_text(format!("Downloading v{version_text}").into());
            }
        });

        let install_result = runtime.block_on(async {
            let weak_for_installing = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak_for_installing.upgrade() {
                    ui.set_update_state_text("Installing".into());
                }
            });
            let install_handle = updater.download_and_install(available);
            install_handle.await
        });

        match install_result {
            Ok(Ok(applied)) => {
                let message = format!(
                    "Update v{} downloaded. Installing now; restarting app.",
                    applied.version
                );
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_update_state_text("Restarting".into());
                        ui.set_status_text(message.into());
                        let _ = ui.hide();
                        let _ = slint::quit_event_loop();
                    }
                });
            }
            Ok(Err(err)) => {
                let message = format!("Update install failed: {err:#}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_status_text(message.into());
                        ui.set_update_state_text("Install failed".into());
                    }
                });
            }
            Err(join_err) => {
                let message = format!("Update install task failed: {join_err}");
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_status_text(message.into());
                        ui.set_update_state_text("Install failed".into());
                    }
                });
            }
        }
    });
}

fn prime_lora_metadata(ui: &MainWindow, context: &AppContext, state: Arc<Mutex<UiState>>) {
    let selected = {
        let mut guard = match state.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let selected = selected_lora(context, &guard, ui);
        if let Some(lora) = &selected {
            guard.selected_lora_id = Some(lora.id.clone());
            ui.set_lora_creator_text("Loading metadata...".into());
            ui.set_lora_strength_text("Loading...".into());
            ui.set_lora_triggers_text("Loading...".into());
            ui.set_lora_description_text("Loading...".into());
            ui.set_lora_preview_image(SlintImage::default());
            ui.set_lora_preview_caption("Loading preview...".into());
        }
        selected
    };

    if let Some(lora) = selected {
        let weak = ui.as_weak();
        let token = token_from_ui(ui);
        let context = context.clone();
        std::thread::spawn(move || fetch_lora_metadata(context, state, weak, lora, token));
    }
}

fn fetch_lora_metadata(
    context: AppContext,
    state: Arc<Mutex<UiState>>,
    weak: Weak<MainWindow>,
    lora: LoraDefinition,
    token: Option<String>,
) {
    if !lora
        .download_url
        .to_ascii_lowercase()
        .contains("civitai.com")
    {
        let lora_id = lora.id.clone();
        let note = lora
            .note
            .clone()
            .unwrap_or_else(|| "Metadata is available for Civitai LoRAs only.".to_string());
        let _ = slint::invoke_from_event_loop(move || {
            if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                if guard.selected_lora_id.as_deref() != Some(lora_id.as_str()) {
                    return;
                }
                guard.lora_preview_url = None;
                guard.lora_creator_url = None;
                ui.set_lora_creator_text("N/A".into());
                ui.set_lora_strength_text("N/A".into());
                ui.set_lora_triggers_text("N/A".into());
                ui.set_lora_description_text(note.into());
                ui.set_lora_preview_image(SlintImage::default());
                ui.set_lora_preview_caption("Preview unavailable for this source.".into());
            }
        });
        return;
    }

    let lora_id = lora.id.clone();
    let fallback_preview = civitai_model_page_url(&lora.download_url);
    let token_for_metadata = token.clone();
    let result = context.runtime.block_on(async {
        let metadata_handle = context
            .downloads
            .civitai_model_metadata(lora.download_url.clone(), token_for_metadata);
        metadata_handle.await
    });

    match result {
        Ok(Ok(mut metadata)) => {
            let mut preview_caption = String::from("Preview image loaded.");
            let mut has_video_preview = false;
            let mut video_preview_url: Option<String> = None;
            let mut preview_bytes = match metadata.preview.take() {
                Some(CivitaiPreview::Image(bytes)) => Some(bytes),
                Some(CivitaiPreview::Video { url }) => {
                    metadata.preview_url = Some(url.clone());
                    video_preview_url = Some(url);
                    has_video_preview = true;
                    preview_caption = "Video preview opened automatically.".to_string();
                    None
                }
                None => None,
            };

            if preview_bytes.is_none() && !has_video_preview {
                if let Some(url) = metadata.preview_url.clone() {
                    let preview_fetch = context.runtime.block_on(async {
                        let handle = context.downloads.civitai_preview_image(url, token.clone());
                        handle.await
                    });
                    if let Ok(Ok(bytes)) = preview_fetch {
                        preview_bytes = Some(bytes);
                        preview_caption = "Preview image loaded.".to_string();
                    }
                }
            }

            if let Some(video_url) = video_preview_url {
                if let Err(err) = preview::open_lora_preview(&video_url) {
                    log::warn!("failed to open automatic video preview: {err:#}");
                }
            }

            let _ = slint::invoke_from_event_loop(move || {
                if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                    if guard.selected_lora_id.as_deref() != Some(lora_id.as_str()) {
                        return;
                    }
                    apply_lora_metadata(
                        &ui,
                        &mut guard,
                        metadata,
                        fallback_preview,
                        preview_bytes,
                        preview_caption,
                    );
                }
            });
        }
        Ok(Err(err)) => {
            let message = format!("Failed to load LoRA metadata: {err:#}");
            let _ = slint::invoke_from_event_loop(move || {
                if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                    if guard.selected_lora_id.as_deref() != Some(lora_id.as_str()) {
                        return;
                    }
                    guard.lora_preview_url = fallback_preview;
                    guard.lora_creator_url = None;
                    ui.set_lora_creator_text("Unavailable".into());
                    ui.set_lora_strength_text("Unavailable".into());
                    ui.set_lora_triggers_text("Unavailable".into());
                    ui.set_lora_description_text(message.into());
                    ui.set_lora_preview_image(SlintImage::default());
                    ui.set_lora_preview_caption("Preview unavailable.".into());
                }
            });
        }
        Err(join_err) => {
            let message = format!("LoRA metadata task failed: {join_err}");
            let _ = slint::invoke_from_event_loop(move || {
                if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                    if guard.selected_lora_id.as_deref() != Some(lora_id.as_str()) {
                        return;
                    }
                    guard.lora_preview_url = fallback_preview;
                    guard.lora_creator_url = None;
                    ui.set_lora_creator_text("Unavailable".into());
                    ui.set_lora_strength_text("Unavailable".into());
                    ui.set_lora_triggers_text("Unavailable".into());
                    ui.set_lora_description_text(message.into());
                    ui.set_lora_preview_image(SlintImage::default());
                    ui.set_lora_preview_caption("Preview unavailable.".into());
                }
            });
        }
    }
}

fn apply_lora_metadata(
    ui: &MainWindow,
    state: &mut UiState,
    metadata: CivitaiModelMetadata,
    fallback_preview: Option<String>,
    preview_bytes: Option<Vec<u8>>,
    preview_caption: String,
) {
    let preview_url = metadata.preview_url.clone().or(fallback_preview);

    state.lora_preview_url = preview_url;
    state.lora_creator_url = metadata.creator_link.clone();

    if let Some(bytes) = preview_bytes {
        if let Some(image) = decode_preview_image(&bytes) {
            ui.set_lora_preview_image(image);
            ui.set_lora_preview_caption(preview_caption.into());
        } else {
            ui.set_lora_preview_image(SlintImage::default());
            ui.set_lora_preview_caption("Preview image could not be decoded.".into());
        }
    } else {
        ui.set_lora_preview_image(SlintImage::default());
        if state.lora_preview_url.is_some() {
            ui.set_lora_preview_caption("Video preview opened automatically.".into());
        } else {
            ui.set_lora_preview_caption("No preview available.".into());
        }
    }

    let creator = metadata
        .creator_username
        .unwrap_or_else(|| "Unknown creator".to_string());
    ui.set_lora_creator_text(creator.into());

    let strength = metadata
        .usage_strength
        .map(|value| format!("{value:.2}"))
        .unwrap_or_else(|| "Not provided".to_string());
    ui.set_lora_strength_text(strength.into());

    let triggers = if metadata.trained_words.is_empty() {
        "No trigger words listed".to_string()
    } else {
        metadata.trained_words.join(", ")
    };
    ui.set_lora_triggers_text(triggers.into());

    let description = metadata
        .description
        .map(|text| strip_html_tags(&text))
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "No description available.".to_string());
    ui.set_lora_description_text(description.into());
}

fn strip_html_tags(input: &str) -> String {
    let mut raw = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                if in_tag {
                    in_tag = false;
                    raw.push(' ');
                }
            }
            _ if !in_tag => raw.push(ch),
            _ => {}
        }
    }

    let mut decoded = raw
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    decoded = decoded
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    decoded
}

fn decode_preview_image(bytes: &[u8]) -> Option<SlintImage> {
    let decoded = load_from_memory(bytes).ok()?;
    let rgba = decoded.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(rgba.as_raw(), width, height);
    Some(SlintImage::from_rgba8(buffer))
}

fn civitai_model_page_url(download_url: &str) -> Option<String> {
    let lower = download_url.to_ascii_lowercase();
    let patterns = ["/model-versions/", "/models/", "/api/download/models/"];
    for pattern in patterns {
        if let Some(pos) = lower.find(pattern) {
            let start = pos + pattern.len();
            let suffix = &download_url[start..];
            let id: String = suffix
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect();
            if !id.is_empty() {
                return Some(format!("https://civitai.com/model-versions/{id}"));
            }
        }
    }
    None
}

fn token_from_ui(ui: &MainWindow) -> Option<String> {
    let token = ui.get_civitai_token().to_string();
    let trimmed = token.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn build_summary_from_model_outcomes(
    outcomes: Vec<crate::download::DownloadOutcome>,
) -> Vec<DownloadSummaryEntry> {
    outcomes
        .into_iter()
        .map(|outcome| {
            let file_name = outcome
                .destination
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.to_string())
                .unwrap_or_else(|| outcome.artifact.file_name().to_string());
            let status = match outcome.status {
                DownloadStatus::Downloaded => "downloaded",
                DownloadStatus::SkippedExisting => "already existed",
            };
            let folder = outcome
                .destination
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| outcome.destination.clone());
            DownloadSummaryEntry {
                label: format!("{file_name} ({status})"),
                folder,
            }
        })
        .collect()
}

fn build_summary_from_lora_outcome(
    outcome: &crate::download::LoraDownloadOutcome,
) -> DownloadSummaryEntry {
    let file_name = outcome
        .destination
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| outcome.lora.derived_file_name());
    let status = match outcome.status {
        DownloadStatus::Downloaded => "downloaded",
        DownloadStatus::SkippedExisting => "already existed",
    };
    let folder = outcome
        .destination
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| outcome.destination.clone());
    DownloadSummaryEntry {
        label: format!("{file_name} ({status})"),
        folder,
    }
}

fn update_summary_ui(ui: &MainWindow, entries: &[DownloadSummaryEntry]) {
    let labels: Vec<String> = entries.iter().map(|entry| entry.label.clone()).collect();
    ui.set_download_summary_entries(to_model_strings(labels));
    if entries.is_empty() {
        ui.set_download_summary_index(-1);
        ui.set_download_summary_visible(false);
    } else {
        ui.set_download_summary_index(0);
        ui.set_download_summary_visible(true);
    }
}

fn wire_callbacks(ui: &MainWindow, context: AppContext, state: Arc<Mutex<UiState>>) {
    let weak = ui.as_weak();

    {
        let context = context.clone();
        let weak = weak.clone();
        ui.on_check_updates_now(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_status_text("Checking for updates...".into());
                ui.set_update_state_text("Checking".into());
            }
            run_update_check(weak.clone(), context.clone(), true);
        });
    }

    {
        let context = context.clone();
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_choose_folder(move || {
            if let Some(folder) = FileDialog::new()
                .set_title("Select ComfyUI Folder")
                .pick_folder()
            {
                if let Some(ui) = weak.upgrade() {
                    let value = folder.to_string_lossy().to_string();
                    ui.set_comfy_path(value.into());
                    if let Err(err) = context.config.update_settings(|settings| {
                        settings.comfyui_root = Some(folder.clone());
                    }) {
                        ui.set_status_text(format!("Failed to save folder: {err}").into());
                    } else {
                        ui.set_status_text("ComfyUI folder saved.".into());
                    }
                }
                if let Ok(mut guard) = state.lock() {
                    guard.last_download_folder = Some(folder);
                }
            }
        });
    }

    {
        let context = context.clone();
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_family_changed(move || {
            if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                rebuild_models(&context, &ui, &mut guard);
            }
        });
    }

    {
        let context = context.clone();
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_model_changed(move || {
            if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                refresh_variants(&context, &ui, &mut guard);
            }
        });
    }

    {
        let context = context.clone();
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_vram_changed(move || {
            if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                refresh_variants(&context, &ui, &mut guard);
            }
        });
    }

    {
        let context = context.clone();
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_variant_changed(move || {
            if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                apply_selected_variant(&context, &ui, &mut guard);
            }
        });
    }

    {
        let weak = weak.clone();
        ui.on_ram_changed(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_status_text("RAM tier updated.".into());
            }
        });
    }

    {
        let context = context.clone();
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_lora_family_changed(move || {
            if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                rebuild_loras(&context, &ui, &mut guard);
            }
        });
    }

    {
        let context = context.clone();
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_lora_changed(move || {
            if let (Some(ui), Ok(mut guard)) = (weak.upgrade(), state.lock()) {
                if let Some(lora) = selected_lora(&context, &guard, &ui) {
                    guard.selected_lora_id = Some(lora.id.clone());
                    ui.set_status_text(format!("Selected LoRA: {}", lora.display_name).into());
                    ui.set_lora_creator_text("Loading metadata...".into());
                    ui.set_lora_strength_text("Loading...".into());
                    ui.set_lora_triggers_text("Loading...".into());
                    ui.set_lora_description_text("Loading...".into());
                    ui.set_lora_preview_image(SlintImage::default());
                    ui.set_lora_preview_caption("Loading preview...".into());

                    let token = token_from_ui(&ui);
                    let weak_ui = weak.clone();
                    let state_for_fetch = Arc::clone(&state);
                    let context_for_fetch = context.clone();
                    std::thread::spawn(move || {
                        fetch_lora_metadata(
                            context_for_fetch,
                            state_for_fetch,
                            weak_ui,
                            lora,
                            token,
                        );
                    });
                } else {
                    guard.selected_lora_id = None;
                    guard.lora_preview_url = None;
                    guard.lora_creator_url = None;
                    ui.set_lora_creator_text("Select a LoRA to load details.".into());
                    ui.set_lora_strength_text("-".into());
                    ui.set_lora_triggers_text("-".into());
                    ui.set_lora_description_text("-".into());
                    ui.set_lora_preview_image(SlintImage::default());
                    ui.set_lora_preview_caption("No preview loaded.".into());
                }
            }
        });
    }

    {
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_open_lora_creator(move || {
            if let Some(ui) = weak.upgrade() {
                let url = state
                    .lock()
                    .ok()
                    .and_then(|guard| guard.lora_creator_url.clone());
                if let Some(url) = url {
                    if let Err(err) = open::that(url) {
                        ui.set_status_text(format!("Failed to open creator page: {err}").into());
                    }
                } else {
                    ui.set_status_text("No creator page available for this LoRA.".into());
                }
            }
        });
    }

    {
        let context = context.clone();
        let weak = weak.clone();
        ui.on_save_token(move || {
            if let Some(ui) = weak.upgrade() {
                let token = ui.get_civitai_token().to_string();
                let trimmed = token.trim().to_string();
                let update = context.config.update_settings(|settings| {
                    settings.civitai_token = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.clone())
                    };
                });
                match update {
                    Ok(_) => ui.set_status_text("Civitai token saved.".into()),
                    Err(err) => ui.set_status_text(format!("Failed to save token: {err}").into()),
                }
            }
        });
    }

    {
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_open_last_folder(move || {
            if let Some(ui) = weak.upgrade() {
                let folder = state
                    .lock()
                    .ok()
                    .and_then(|guard| guard.last_download_folder.clone());
                if let Some(path) = folder {
                    if let Err(err) = open::that(path) {
                        ui.set_status_text(format!("Failed to open folder: {err}").into());
                    }
                } else {
                    ui.set_status_text("No downloaded folder available yet.".into());
                }
            }
        });
    }

    {
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_open_summary_folder(move || {
            if let Some(ui) = weak.upgrade() {
                let selected = ui.get_download_summary_index();
                let folder = state.lock().ok().and_then(|guard| {
                    if selected < 0 {
                        None
                    } else {
                        guard
                            .download_summary
                            .get(selected as usize)
                            .map(|entry| entry.folder.clone())
                    }
                });

                if let Some(folder) = folder {
                    if let Err(err) = open::that(folder) {
                        ui.set_status_text(format!("Failed to open summary folder: {err}").into());
                    }
                } else {
                    ui.set_status_text("No summary entry selected.".into());
                }
            }
        });
    }

    {
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_clear_summary(move || {
            if let Some(ui) = weak.upgrade() {
                if let Ok(mut guard) = state.lock() {
                    guard.download_summary.clear();
                }
                ui.set_download_summary_entries(to_model_strings(Vec::new()));
                ui.set_download_summary_index(-1);
                ui.set_download_summary_visible(false);
            }
        });
    }

    {
        let context = context.clone();
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_download_model_assets(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };

            let comfy_root = PathBuf::from(ui.get_comfy_path().to_string());
            if !comfy_root.exists() {
                ui.set_status_text("Select a valid ComfyUI root folder first.".into());
                return;
            }

            let resolved = state
                .lock()
                .ok()
                .and_then(|guard| guard.selected_model.clone());
            let Some(resolved) = resolved else {
                ui.set_status_text("Select a model variant before downloading.".into());
                return;
            };

            let ram_tier = RamTier::all()
                .get(ui.get_ram_index().max(0) as usize)
                .copied()
                .or_else(|| context.ram_tier());

            let plan = resolved.artifacts_for_download(ram_tier);
            if plan.is_empty() {
                ui.set_status_text("No artifacts match the selected RAM tier.".into());
                return;
            }

            if let Ok(mut guard) = state.lock() {
                guard.last_download_folder = Some(comfy_root.clone());
                guard.download_summary.clear();
            }
            ui.set_download_summary_entries(to_model_strings(Vec::new()));
            ui.set_download_summary_index(-1);
            ui.set_download_summary_visible(false);

            let (tx, rx) = mpsc::channel::<DownloadSignal>();
            let handle = context.downloads.download_variant(comfy_root, resolved, tx);
            ui.set_progress_text(format!("Downloading {} artifacts...", plan.len()).into());
            ui.set_status_text("Model download started.".into());

            let weak_progress: Weak<MainWindow> = weak.clone();
            std::thread::spawn(move || {
                let mut failed = false;
                while let Ok(signal) = rx.recv() {
                    if matches!(signal, DownloadSignal::Failed { .. }) {
                        failed = true;
                    }
                    let weak_ui = weak_progress.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak_ui.upgrade() {
                            match signal {
                                DownloadSignal::Started {
                                    artifact,
                                    index,
                                    total,
                                    ..
                                } => ui.set_progress_text(
                                    format!("[{}/{}] {}", index + 1, total, artifact).into(),
                                ),
                                DownloadSignal::Progress {
                                    artifact,
                                    received,
                                    size,
                                    ..
                                } => {
                                    let text = match size {
                                        Some(total) if total > 0 => {
                                            let pct = (received as f64 / total as f64) * 100.0;
                                            format!("{} {:.0}%", artifact, pct)
                                        }
                                        _ => format!("{} {} bytes", artifact, received),
                                    };
                                    ui.set_progress_text(text.into());
                                }
                                DownloadSignal::Finished { .. } => {}
                                DownloadSignal::Failed { artifact, error } => ui.set_status_text(
                                    format!("{} failed: {}", artifact, error).into(),
                                ),
                            }
                        }
                    });
                }

                let weak_ui = weak_progress.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak_ui.upgrade() {
                        if failed {
                            ui.set_status_text("Model download finished with errors.".into());
                        } else {
                            ui.set_status_text("Model download complete.".into());
                        }
                        ui.set_progress_text("Idle".into());
                    }
                });
            });

            let runtime = context.runtime.clone();
            let state_done = Arc::clone(&state);
            let weak_done = weak.clone();
            std::thread::spawn(move || {
                let result = runtime.block_on(async { handle.await });
                let _ = slint::invoke_from_event_loop(move || {
                    if let (Some(ui), Ok(mut guard)) = (weak_done.upgrade(), state_done.lock()) {
                        match result {
                            Ok(Ok(outcomes)) => {
                                let entries = build_summary_from_model_outcomes(outcomes);
                                guard.download_summary = entries.clone();
                                update_summary_ui(&ui, &entries);
                            }
                            Ok(Err(err)) => {
                                ui.set_status_text(
                                    format!("Model download failed: {err:#}").into(),
                                );
                            }
                            Err(join_err) => {
                                ui.set_status_text(
                                    format!("Model download task failed: {join_err}").into(),
                                );
                            }
                        }
                    }
                });
            });
        });
    }

    {
        let context = context.clone();
        let state = Arc::clone(&state);
        let weak = weak.clone();
        ui.on_download_lora_asset(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };

            let comfy_root = PathBuf::from(ui.get_comfy_path().to_string());
            if !comfy_root.exists() {
                ui.set_status_text("Select a valid ComfyUI root folder first.".into());
                return;
            }

            let lora = state
                .lock()
                .ok()
                .and_then(|guard| selected_lora(&context, &guard, &ui));
            let Some(lora) = lora else {
                ui.set_status_text("Select a LoRA first.".into());
                return;
            };

            if let Ok(mut guard) = state.lock() {
                guard.last_download_folder = Some(comfy_root.clone());
                guard.download_summary.clear();
            }
            ui.set_download_summary_entries(to_model_strings(Vec::new()));
            ui.set_download_summary_index(-1);
            ui.set_download_summary_visible(false);

            let token = ui.get_civitai_token().to_string();
            let token = if token.trim().is_empty() {
                None
            } else {
                Some(token)
            };

            let (tx, rx) = mpsc::channel::<DownloadSignal>();
            let handle = context
                .downloads
                .download_lora(comfy_root, lora.clone(), token, tx);
            ui.set_progress_text(format!("Downloading LoRA {}...", lora.display_name).into());
            ui.set_status_text("LoRA download started.".into());

            let weak_progress = weak.clone();
            std::thread::spawn(move || {
                let mut failed = false;
                while let Ok(signal) = rx.recv() {
                    if matches!(signal, DownloadSignal::Failed { .. }) {
                        failed = true;
                    }
                    let weak_ui = weak_progress.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak_ui.upgrade() {
                            match signal {
                                DownloadSignal::Started { artifact, .. } => {
                                    ui.set_progress_text(format!("Starting {}", artifact).into())
                                }
                                DownloadSignal::Progress {
                                    artifact,
                                    received,
                                    size,
                                    ..
                                } => {
                                    let text = match size {
                                        Some(total) if total > 0 => {
                                            let pct = (received as f64 / total as f64) * 100.0;
                                            format!("{} {:.0}%", artifact, pct)
                                        }
                                        _ => format!("{} {} bytes", artifact, received),
                                    };
                                    ui.set_progress_text(text.into());
                                }
                                DownloadSignal::Finished { .. } => {}
                                DownloadSignal::Failed { artifact, error } => ui.set_status_text(
                                    format!("{} failed: {}", artifact, error).into(),
                                ),
                            }
                        }
                    });
                }

                let weak_ui = weak_progress.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak_ui.upgrade() {
                        if failed {
                            ui.set_status_text("LoRA download finished with errors.".into());
                        } else {
                            ui.set_status_text("LoRA download complete.".into());
                        }
                        ui.set_progress_text("Idle".into());
                    }
                });
            });

            let runtime = context.runtime.clone();
            let state_done = Arc::clone(&state);
            let weak_done = weak.clone();
            std::thread::spawn(move || {
                let result = runtime.block_on(async { handle.await });
                let _ = slint::invoke_from_event_loop(move || {
                    if let (Some(ui), Ok(mut guard)) = (weak_done.upgrade(), state_done.lock()) {
                        match result {
                            Ok(Ok(outcome)) => {
                                let entries = vec![build_summary_from_lora_outcome(&outcome)];
                                guard.download_summary = entries.clone();
                                update_summary_ui(&ui, &entries);
                            }
                            Ok(Err(err)) => {
                                ui.set_status_text(format!("LoRA download failed: {err:#}").into());
                            }
                            Err(join_err) => {
                                ui.set_status_text(format!("LoRA task failed: {join_err}").into());
                            }
                        }
                    }
                });
            });
        });
    }
}

fn build_state(context: &AppContext) -> UiState {
    let catalog = context.catalog.catalog_snapshot();

    let mut model_families = vec![Choice {
        id: "all".to_string(),
        label: "All Model Families".to_string(),
    }];
    model_families.extend(catalog.model_families().into_iter().map(|family| Choice {
        id: family.clone(),
        label: family,
    }));

    let models = collect_models(&catalog, None);
    let variants = collect_variants(
        &catalog,
        models.first().map(|choice| choice.id.as_str()),
        VramTier::TierS,
    );

    let mut lora_families = vec![Choice {
        id: "all".to_string(),
        label: "All LoRA Families".to_string(),
    }];
    lora_families.extend(catalog.lora_families().into_iter().map(|family| Choice {
        id: family.clone(),
        label: family,
    }));

    let loras = collect_loras(&catalog, None);

    UiState {
        model_families,
        models,
        variants,
        lora_families,
        loras,
        selected_model: None,
        selected_lora_id: None,
        lora_preview_url: None,
        lora_creator_url: None,
        download_summary: Vec::new(),
        last_download_folder: None,
    }
}

fn rebuild_models(context: &AppContext, ui: &MainWindow, state: &mut UiState) {
    let family_id = selected_choice(&state.model_families, ui.get_family_index())
        .map(|choice| choice.id.clone());
    let filter = family_id.as_deref().filter(|id| *id != "all");

    let catalog = context.catalog.catalog_snapshot();
    state.models = collect_models(&catalog, filter);
    ui.set_model_options(to_choice_model(&state.models));
    ui.set_model_index(if state.models.is_empty() { -1 } else { 0 });
    refresh_variants(context, ui, state);
}

fn refresh_variants(context: &AppContext, ui: &MainWindow, state: &mut UiState) {
    let catalog = context.catalog.catalog_snapshot();
    let model_id =
        selected_choice(&state.models, ui.get_model_index()).map(|choice| choice.id.clone());
    let tier = VramTier::all()
        .get(ui.get_vram_index().max(0) as usize)
        .copied()
        .unwrap_or(VramTier::TierS);

    state.variants = collect_variants(&catalog, model_id.as_deref(), tier);
    ui.set_variant_options(to_choice_model(&state.variants));
    ui.set_variant_index(if state.variants.is_empty() { -1 } else { 0 });
    apply_selected_variant(context, ui, state);

    let thresholds = model_id
        .as_deref()
        .and_then(|id| catalog.find_model(id))
        .map(|model| model.resolved_ram_thresholds())
        .unwrap_or_default();
    ui.set_ram_options(ram_options_with_thresholds(&thresholds));
}

fn apply_selected_variant(context: &AppContext, ui: &MainWindow, state: &mut UiState) {
    let model_id =
        selected_choice(&state.models, ui.get_model_index()).map(|choice| choice.id.clone());
    let variant_id =
        selected_choice(&state.variants, ui.get_variant_index()).map(|choice| choice.id.clone());

    state.selected_model = match (model_id, variant_id) {
        (Some(model_id), Some(variant_id)) => {
            context.catalog.resolve_variant(&model_id, &variant_id)
        }
        _ => None,
    };

    if let Some(selected) = &state.selected_model {
        ui.set_status_text(
            format!(
                "Ready: {} / {}",
                selected.master.display_name,
                selected.variant.summary()
            )
            .into(),
        );
    } else {
        ui.set_status_text("Select a model and variant.".into());
    }
}

fn rebuild_loras(context: &AppContext, ui: &MainWindow, state: &mut UiState) {
    let family_id = selected_choice(&state.lora_families, ui.get_lora_family_index())
        .map(|choice| choice.id.clone());
    let filter = family_id.as_deref().filter(|id| *id != "all");

    let catalog = context.catalog.catalog_snapshot();
    state.loras = collect_loras(&catalog, filter);
    ui.set_lora_options(to_choice_model(&state.loras));
    ui.set_lora_index(if state.loras.is_empty() { -1 } else { 0 });
    if state.loras.is_empty() {
        state.selected_lora_id = None;
        state.lora_preview_url = None;
        state.lora_creator_url = None;
        ui.set_lora_creator_text("Select a LoRA to load details.".into());
        ui.set_lora_strength_text("-".into());
        ui.set_lora_triggers_text("-".into());
        ui.set_lora_description_text("-".into());
        ui.set_lora_preview_image(SlintImage::default());
        ui.set_lora_preview_caption("No preview loaded.".into());
    }
}

fn collect_models(catalog: &ModelCatalog, family_filter: Option<&str>) -> Vec<Choice> {
    catalog
        .models
        .iter()
        .filter(|model| {
            family_filter
                .map(|filter| model.family.eq_ignore_ascii_case(filter))
                .unwrap_or(true)
        })
        .map(|model| Choice {
            id: model.id.clone(),
            label: model.display_name.clone(),
        })
        .collect()
}

fn collect_variants(catalog: &ModelCatalog, model_id: Option<&str>, tier: VramTier) -> Vec<Choice> {
    let Some(model_id) = model_id else {
        return Vec::new();
    };

    catalog
        .models
        .iter()
        .find(|model| model.id == model_id)
        .map(|model| {
            model
                .variants_for_tier(tier)
                .into_iter()
                .map(|variant| Choice {
                    id: variant.id.clone(),
                    label: variant.selection_label(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn collect_loras(catalog: &ModelCatalog, family_filter: Option<&str>) -> Vec<Choice> {
    catalog
        .loras
        .iter()
        .filter(|lora| {
            family_filter
                .map(|filter| {
                    lora.family
                        .as_deref()
                        .map(|family| family.eq_ignore_ascii_case(filter))
                        .unwrap_or(false)
                })
                .unwrap_or(true)
        })
        .map(|lora| Choice {
            id: lora.id.clone(),
            label: lora.display_name.clone(),
        })
        .collect()
}

fn selected_choice(choices: &[Choice], index: i32) -> Option<&Choice> {
    if index < 0 {
        return None;
    }
    choices.get(index as usize)
}

fn selected_lora(context: &AppContext, state: &UiState, ui: &MainWindow) -> Option<LoraDefinition> {
    let choice = selected_choice(&state.loras, ui.get_lora_index())?;
    context.catalog.find_lora(&choice.id)
}

fn to_choice_model(choices: &[Choice]) -> ModelRc<SharedString> {
    to_model_strings(choices.iter().map(|choice| choice.label.clone()).collect())
}

fn to_model_strings(values: Vec<String>) -> ModelRc<SharedString> {
    let shared: Vec<SharedString> = values.into_iter().map(SharedString::from).collect();
    let model = Rc::new(VecModel::from(shared));
    ModelRc::from(model)
}

fn default_ram_index(context: &AppContext) -> i32 {
    context
        .ram_tier()
        .map(|tier| tier.index() as i32)
        .unwrap_or(0)
}

fn ram_options_with_thresholds(thresholds: &ResolvedRamTierThresholds) -> ModelRc<SharedString> {
    to_model_strings(
        RamTier::all()
            .iter()
            .map(|tier| format!("{} ({})", tier.label(), thresholds.range_label(*tier)))
            .collect(),
    )
}

fn initial_status(context: &AppContext) -> String {
    match context.total_ram_gb() {
        Some(ram) => format!("Detected system RAM: {:.1} GB", ram),
        None => "System RAM detection unavailable; choose RAM tier manually.".to_string(),
    }
}
