use arctic_downloader::{
    app::{build_context, AppContext},
    config::AppSettings,
    download::{CivitaiPreview, DownloadSignal},
    env_flags::auto_update_enabled,
    model::{LoraDefinition, ModelCatalog},
    ram::RamTier,
};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

struct AppState {
    context: AppContext,
    active_cancel: Mutex<Option<CancellationToken>>,
    active_abort: Mutex<Option<tokio::task::AbortHandle>>,
}

#[derive(Debug, Serialize)]
struct AppSnapshot {
    version: String,
    total_ram_gb: Option<f64>,
    ram_tier: Option<String>,
    nvidia_gpu_name: Option<String>,
    nvidia_gpu_vram_mb: Option<u64>,
    model_count: usize,
    lora_count: usize,
}

#[derive(Debug, Serialize)]
struct UpdateCheckResponse {
    available: bool,
    version: Option<String>,
    notes: Option<String>,
}

#[derive(Debug, Serialize)]
struct LoraMetadataResponse {
    creator: String,
    creator_url: Option<String>,
    strength: String,
    triggers: Vec<String>,
    description: String,
    preview_url: Option<String>,
    preview_kind: String,
}

#[derive(Clone, Debug, Serialize)]
struct DownloadProgressEvent {
    kind: String,
    phase: String,
    artifact: Option<String>,
    index: Option<usize>,
    total: Option<usize>,
    received: Option<u64>,
    size: Option<u64>,
    folder: Option<String>,
    message: Option<String>,
}

#[tauri::command]
fn get_app_snapshot(state: State<'_, AppState>) -> AppSnapshot {
    let catalog = state.context.catalog.catalog_snapshot();
    let (nvidia_gpu_name, nvidia_gpu_vram_mb) = detect_nvidia_gpu();
    AppSnapshot {
        version: state.context.display_version.clone(),
        total_ram_gb: state.context.total_ram_gb(),
        ram_tier: state.context.ram_tier().map(|tier| tier.label().to_string()),
        nvidia_gpu_name,
        nvidia_gpu_vram_mb,
        model_count: catalog.models.len(),
        lora_count: catalog.loras.len(),
    }
}

fn detect_nvidia_gpu() -> (Option<String>, Option<u64>) {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output();

    let Ok(output) = output else {
        return (None, None);
    };
    if !output.status.success() {
        return (None, None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if first.is_empty() {
        return (None, None);
    }

    let mut parts = first.splitn(2, ',');
    let name = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let vram_mb = parts
        .next()
        .map(str::trim)
        .and_then(|value| value.parse::<u64>().ok());

    (name, vram_mb)
}

#[tauri::command]
fn get_catalog(state: State<'_, AppState>) -> ModelCatalog {
    state.context.catalog.catalog_snapshot()
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> AppSettings {
    state.context.config.settings()
}

#[tauri::command]
fn set_comfyui_root(state: State<'_, AppState>, comfyui_root: String) -> Result<AppSettings, String> {
    let trimmed = comfyui_root.trim();
    let normalized = if trimmed.is_empty() {
        None
    } else {
        let mut path = std::path::PathBuf::from(trimmed);
        if !path.is_absolute() {
            if let Ok(cwd) = std::env::current_dir() {
                path = cwd.join(path);
            }
        }
        Some(std::fs::canonicalize(&path).unwrap_or(path))
    };
    state
        .context
        .config
        .update_settings(|settings| {
            settings.comfyui_root = normalized.clone();
        })
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn save_civitai_token(state: State<'_, AppState>, token: String) -> Result<AppSettings, String> {
    let trimmed = token.trim().to_string();
    state
        .context
        .config
        .update_settings(|settings| {
            settings.civitai_token = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            };
        })
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn check_updates_now(state: State<'_, AppState>) -> Result<UpdateCheckResponse, String> {
    let updater = state.context.updater.clone();
    let result = updater.check_for_update().await;

    match result {
        Ok(Ok(Some(update))) => Ok(UpdateCheckResponse {
            available: true,
            version: Some(update.version.to_string()),
            notes: update.notes,
        }),
        Ok(Ok(None)) => Ok(UpdateCheckResponse {
            available: false,
            version: None,
            notes: None,
        }),
        Ok(Err(err)) => Err(format!("Update check failed: {err:#}")),
        Err(join_err) => Err(format!("Update task failed: {join_err}")),
    }
}

#[tauri::command]
async fn auto_update_startup(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UpdateCheckResponse, String> {
    if !auto_update_enabled() {
        return Ok(UpdateCheckResponse {
            available: false,
            version: None,
            notes: Some("Auto update disabled by environment.".to_string()),
        });
    }

    let updater = state.context.updater.clone();

    let checked = updater.check_for_update().await;

    let Some(update) = (match checked {
        Ok(Ok(Some(update))) => Some(update),
        Ok(Ok(None)) => {
            return Ok(UpdateCheckResponse {
                available: false,
                version: None,
                notes: None,
            });
        }
        Ok(Err(err)) => return Err(format!("Update check failed: {err:#}")),
        Err(join_err) => return Err(format!("Update task failed: {join_err}")),
    }) else {
        return Ok(UpdateCheckResponse {
            available: false,
            version: None,
            notes: None,
        });
    };

    let _ = app.emit(
        "update-state",
        DownloadProgressEvent {
            kind: "update".to_string(),
            phase: "available".to_string(),
            artifact: None,
            index: None,
            total: None,
            received: None,
            size: None,
            folder: None,
            message: Some(format!("Update v{} available; installing.", update.version)),
        },
    );

    let install = updater.download_and_install(update.clone()).await;

    match install {
        Ok(Ok(applied)) => {
            let _ = app.emit(
                "update-state",
                DownloadProgressEvent {
                    kind: "update".to_string(),
                    phase: "restarting".to_string(),
                    artifact: None,
                    index: None,
                    total: None,
                    received: None,
                    size: None,
                    folder: None,
                    message: Some(format!(
                        "Update v{} installed; restarting application.",
                        applied.version
                    )),
                },
            );
            app.exit(0);
            Ok(UpdateCheckResponse {
                available: true,
                version: Some(applied.version.to_string()),
                notes: Some("Installer launched.".to_string()),
            })
        }
        Ok(Err(err)) => Err(format!("Update install failed: {err:#}")),
        Err(join_err) => Err(format!("Update install task failed: {join_err}")),
    }
}

#[tauri::command]
async fn download_model_assets(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
    variant_id: String,
    ram_tier: Option<String>,
    comfyui_root: Option<String>,
) -> Result<(), String> {
    let root = resolve_root_path(&state.context, comfyui_root)?;
    let resolved = state
        .context
        .catalog
        .resolve_variant(&model_id, &variant_id)
        .ok_or_else(|| "Selected model variant was not found in catalog.".to_string())?;

    let tier = ram_tier
        .as_deref()
        .and_then(parse_ram_tier)
        .or_else(|| state.context.ram_tier());
    let planned = resolved.artifacts_for_download(tier);
    if planned.is_empty() {
        return Err("No artifacts match the selected RAM tier.".to_string());
    }

    let cancel = CancellationToken::new();
    {
        let mut active = state
            .active_cancel
            .lock()
            .map_err(|_| "download state lock poisoned".to_string())?;
        if active.is_some() {
            return Err("A download is already active. Cancel it first.".to_string());
        }
        *active = Some(cancel.clone());
    }

    let mut resolved_for_download = resolved.clone();
    resolved_for_download.variant.artifacts = planned;

    let (tx, rx) = std::sync::mpsc::channel();
    let handle = state
        .context
        .downloads
        .download_variant_with_cancel(root, resolved_for_download, tx, Some(cancel));
    if let Ok(mut abort) = state.active_abort.lock() {
        *abort = Some(handle.abort_handle());
    }
    spawn_progress_emitter(app.clone(), "model".to_string(), rx);
    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = handle.await;
        let managed = app_for_task.state::<AppState>();
        if let Ok(mut active) = managed.active_cancel.lock() {
            *active = None;
        }
        if let Ok(mut abort) = managed.active_abort.lock() {
            *abort = None;
        }

        match result {
            Ok(Ok(outcomes)) => {
                let _ = app_for_task.emit(
                    "download-progress",
                    DownloadProgressEvent {
                        kind: "model".to_string(),
                        phase: "batch_finished".to_string(),
                        artifact: None,
                        index: None,
                        total: Some(outcomes.len()),
                        received: None,
                        size: None,
                        folder: None,
                        message: Some("Model download batch completed.".to_string()),
                    },
                );
            }
            Ok(Err(err)) => {
                let lower = err.to_string().to_ascii_lowercase();
                let phase = if lower.contains("cancel") {
                    "cancelled"
                } else {
                    "batch_failed"
                };
                let _ = app_for_task.emit(
                    "download-progress",
                    DownloadProgressEvent {
                        kind: "model".to_string(),
                        phase: phase.to_string(),
                        artifact: None,
                        index: None,
                        total: None,
                        received: None,
                        size: None,
                        folder: None,
                        message: Some(err.to_string()),
                    },
                );
            }
            Err(join_err) => {
                let phase = if join_err.is_cancelled() {
                    "cancelled"
                } else {
                    "batch_failed"
                };
                let _ = app_for_task.emit(
                    "download-progress",
                    DownloadProgressEvent {
                        kind: "model".to_string(),
                        phase: phase.to_string(),
                        artifact: None,
                        index: None,
                        total: None,
                        received: None,
                        size: None,
                        folder: None,
                        message: Some(join_err.to_string()),
                    },
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
async fn download_lora_asset(
    app: AppHandle,
    state: State<'_, AppState>,
    lora_id: String,
    token: Option<String>,
    comfyui_root: Option<String>,
) -> Result<(), String> {
    let root = resolve_root_path(&state.context, comfyui_root)?;
    let lora = state
        .context
        .catalog
        .find_lora(&lora_id)
        .ok_or_else(|| "Selected LoRA was not found in catalog.".to_string())?;

    let cancel = CancellationToken::new();
    {
        let mut active = state
            .active_cancel
            .lock()
            .map_err(|_| "download state lock poisoned".to_string())?;
        if active.is_some() {
            return Err("A download is already active. Cancel it first.".to_string());
        }
        *active = Some(cancel.clone());
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let handle = state
        .context
        .downloads
        .download_lora_with_cancel(root, lora, token, tx, Some(cancel));
    if let Ok(mut abort) = state.active_abort.lock() {
        *abort = Some(handle.abort_handle());
    }
    spawn_progress_emitter(app.clone(), "lora".to_string(), rx);
    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = handle.await;
        let managed = app_for_task.state::<AppState>();
        if let Ok(mut active) = managed.active_cancel.lock() {
            *active = None;
        }
        if let Ok(mut abort) = managed.active_abort.lock() {
            *abort = None;
        }

        match result {
            Ok(Ok(_outcome)) => {
                let _ = app_for_task.emit(
                    "download-progress",
                    DownloadProgressEvent {
                        kind: "lora".to_string(),
                        phase: "batch_finished".to_string(),
                        artifact: None,
                        index: None,
                        total: Some(1),
                        received: None,
                        size: None,
                        folder: None,
                        message: Some("LoRA download completed.".to_string()),
                    },
                );
            }
            Ok(Err(err)) => {
                let lower = err.to_string().to_ascii_lowercase();
                let phase = if lower.contains("cancel") {
                    "cancelled"
                } else {
                    "batch_failed"
                };
                let _ = app_for_task.emit(
                    "download-progress",
                    DownloadProgressEvent {
                        kind: "lora".to_string(),
                        phase: phase.to_string(),
                        artifact: None,
                        index: None,
                        total: None,
                        received: None,
                        size: None,
                        folder: None,
                        message: Some(err.to_string()),
                    },
                );
            }
            Err(join_err) => {
                let phase = if join_err.is_cancelled() {
                    "cancelled"
                } else {
                    "batch_failed"
                };
                let _ = app_for_task.emit(
                    "download-progress",
                    DownloadProgressEvent {
                        kind: "lora".to_string(),
                        phase: phase.to_string(),
                        artifact: None,
                        index: None,
                        total: None,
                        received: None,
                        size: None,
                        folder: None,
                        message: Some(join_err.to_string()),
                    },
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
async fn get_lora_metadata(
    state: State<'_, AppState>,
    lora_id: String,
    token: Option<String>,
) -> Result<LoraMetadataResponse, String> {
    let lora: LoraDefinition = state
        .context
        .catalog
        .find_lora(&lora_id)
        .ok_or_else(|| "Selected LoRA was not found in catalog.".to_string())?;

    if !lora.download_url.to_ascii_lowercase().contains("civitai.com") {
        return Ok(LoraMetadataResponse {
            creator: "N/A".to_string(),
            creator_url: None,
            strength: "N/A".to_string(),
            triggers: Vec::new(),
            description: lora
                .note
                .unwrap_or_else(|| "Metadata is available for Civitai LoRAs only.".to_string()),
            preview_url: None,
            preview_kind: "none".to_string(),
        });
    }

    let result = state
        .context
        .downloads
        .civitai_model_metadata(lora.download_url.clone(), token)
        .await;

    match result {
        Ok(Ok(metadata)) => {
            let (preview_kind, preview_url) = match metadata.preview {
                Some(CivitaiPreview::Video { url }) => ("video".to_string(), Some(url)),
                Some(CivitaiPreview::Image(_)) => (
                    if metadata
                        .preview_url
                        .as_deref()
                        .map(is_video_url)
                        .unwrap_or(false)
                    {
                        "video".to_string()
                    } else {
                        "image".to_string()
                    },
                    metadata.preview_url.clone(),
                ),
                None => (
                    if metadata
                        .preview_url
                        .as_deref()
                        .map(is_video_url)
                        .unwrap_or(false)
                    {
                        "video".to_string()
                    } else {
                        "none".to_string()
                    },
                    metadata.preview_url.clone(),
                ),
            };

            Ok(LoraMetadataResponse {
                creator: metadata
                    .creator_username
                    .unwrap_or_else(|| "Unknown creator".to_string()),
                creator_url: metadata.creator_link,
                strength: metadata
                    .usage_strength
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| "Not provided".to_string()),
                triggers: metadata.trained_words,
                description: metadata
                    .description
                    .map(|text| strip_html_tags(&text))
                    .filter(|text| !text.trim().is_empty())
                    .unwrap_or_else(|| "No description available.".to_string()),
                preview_url,
                preview_kind,
            })
        }
        Ok(Err(err)) => Err(format!("Failed to load LoRA metadata: {err:#}")),
        Err(join_err) => Err(format!("LoRA metadata task failed: {join_err}")),
    }
}

fn resolve_root_path(context: &AppContext, comfyui_root: Option<String>) -> Result<std::path::PathBuf, String> {
    fn normalize_existing(path: std::path::PathBuf) -> Option<std::path::PathBuf> {
        let absolute = if path.is_absolute() {
            path
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(path)
        } else {
            path
        };
        if !absolute.exists() {
            return None;
        }
        std::fs::canonicalize(&absolute).ok().or(Some(absolute))
    }

    if let Some(root) = comfyui_root {
        let trimmed = root.trim();
        if !trimmed.is_empty() {
            let path = std::path::PathBuf::from(trimmed);
            if let Some(normalized) = normalize_existing(path) {
                return Ok(normalized);
            }
        }
    }

    if let Some(path) = context.config.settings().comfyui_root {
        if let Some(normalized) = normalize_existing(path) {
            return Ok(normalized);
        }
    }

    Err("Select a valid ComfyUI root folder first.".to_string())
}

fn parse_ram_tier(value: &str) -> Option<RamTier> {
    RamTier::from_identifier(value)
}

fn is_video_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.ends_with(".mp4")
        || lower.ends_with(".webm")
        || lower.ends_with(".mov")
        || lower.contains(".mp4?")
        || lower.contains(".webm?")
        || lower.contains(".mov?")
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
    raw.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn spawn_progress_emitter(
    app: AppHandle,
    kind: String,
    rx: std::sync::mpsc::Receiver<DownloadSignal>,
) {
    std::thread::spawn(move || {
        while let Ok(signal) = rx.recv() {
            let payload = match signal {
                DownloadSignal::Started {
                    artifact,
                    index,
                    total,
                    size,
                } => DownloadProgressEvent {
                    kind: kind.clone(),
                    phase: "started".to_string(),
                    artifact: Some(artifact),
                    index: Some(index + 1),
                    total: Some(total),
                    received: None,
                    size,
                    folder: None,
                    message: None,
                },
                DownloadSignal::Progress {
                    artifact,
                    index,
                    received,
                    size,
                } => DownloadProgressEvent {
                    kind: kind.clone(),
                    phase: "progress".to_string(),
                    artifact: Some(artifact),
                    index: Some(index + 1),
                    total: None,
                    received: Some(received),
                    size,
                    folder: None,
                    message: None,
                },
                DownloadSignal::Finished {
                    artifact,
                    index,
                    size,
                    folder,
                } => DownloadProgressEvent {
                    kind: kind.clone(),
                    phase: "finished".to_string(),
                    artifact: Some(artifact),
                    index: Some(index + 1),
                    total: None,
                    received: None,
                    size,
                    folder,
                    message: None,
                },
                DownloadSignal::Failed { artifact, error } => DownloadProgressEvent {
                    kind: kind.clone(),
                    phase: "failed".to_string(),
                    artifact: Some(artifact),
                    index: None,
                    total: None,
                    received: None,
                    size: None,
                    folder: None,
                    message: Some(error),
                },
            };
            let _ = app.emit("download-progress", payload);
        }
    });
}

#[cfg(target_os = "windows")]
fn normalize_explorer_path(path: &std::path::Path) -> String {
    let display = path.to_string_lossy().to_string();
    if let Some(stripped) = display.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{}", stripped);
    }
    if let Some(stripped) = display.strip_prefix(r"\\?\") {
        return stripped.to_string();
    }
    display
}

#[tauri::command]
fn open_folder(path: String) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Folder path is empty.".to_string());
    }
    let mut target = std::path::PathBuf::from(trimmed);
    if !target.is_absolute() {
        if let Ok(cwd) = std::env::current_dir() {
            target = cwd.join(target);
        }
    }
    if target.is_file() {
        if let Some(parent) = target.parent() {
            target = parent.to_path_buf();
        }
    }
    if let Ok(canon) = std::fs::canonicalize(&target) {
        target = canon;
    }
    if !target.exists() {
        return Err("Folder does not exist.".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let open_target = normalize_explorer_path(&target);
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(&open_target)
            .spawn()
            .map_err(|err| format!("Failed to open folder: {err}"))?;
        return Ok(open_target);
    }

    #[cfg(not(target_os = "windows"))]
    {
        open::that(target).map_err(|err| format!("Failed to open folder: {err}"))?;
        Ok(path)
    }
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err("Only http/https links are allowed.".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(trimmed)
            .spawn()
            .map_err(|err| format!("Failed to open link: {err}"))?;
        return Ok(());
    }

    #[cfg(not(target_os = "windows"))]
    {
        open::that(trimmed).map_err(|err| format!("Failed to open link: {err}"))?;
        Ok(())
    }
}

#[tauri::command]
fn pick_folder() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
fn cancel_active_download(state: State<'_, AppState>) -> Result<bool, String> {
    let mut active = state
        .active_cancel
        .lock()
        .map_err(|_| "download state lock poisoned".to_string())?;
    let mut abort = state
        .active_abort
        .lock()
        .map_err(|_| "download state lock poisoned".to_string())?;
    if let Some(token) = active.as_ref() {
        token.cancel();
        if let Some(handle) = abort.take() {
            handle.abort();
        }
        *active = None;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let context = match build_context() {
        Ok(context) => context,
        Err(err) => {
            eprintln!("Failed to initialize app context: {err:#}");
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .manage(AppState {
            context,
            active_cancel: Mutex::new(None),
            active_abort: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            get_catalog,
            get_settings,
            set_comfyui_root,
            save_civitai_token,
            check_updates_now,
            auto_update_startup,
            download_model_assets,
            download_lora_asset,
            get_lora_metadata,
            open_folder,
            open_external_url,
            pick_folder,
            cancel_active_download
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}
