//! Install-location management and state-reporting commands for the Windows
//! backend. Windows counterpart to `app_linux/install_state.rs` -- see that
//! file's doc comment for the scope rationale (these three commands only
//! read/record state; they don't mutate an installation) and for the
//! `generate_handler!` qualified-path requirement that applies here too.
//!
//! One real, pre-existing divergence physically relocated but not unified:
//! Windows normalizes paths with `strip_windows_verbatim_prefix` (stripping
//! the `\\?\` prefix `std::fs::canonicalize` adds on Windows) everywhere
//! Linux uses `normalize_canonical_path`; `get_comfyui_addon_state` also
//! checks `pip_has_package` in addition to `python_module_importable` for
//! sage/flash presence, where Linux's checks that only for nunchaku.

use crate::shared::{custom_node_exists, custom_node_installed};
// General path-normalization/install-path-validation/python-introspection
// utilities defined in the parent `app_windows` module itself (not
// `shared.rs`), used well beyond these three commands.
use super::{
    detect_launch_attention_backend_for_root, detect_torch_profile_for_root,
    is_forbidden_install_path, nunchaku_backend_present, pip_has_package, python_module_importable,
    resolve_root_path, strip_windows_verbatim_prefix, CUSTOM_NODES,
};
use arctic_downloader::config::AppSettings;
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

use super::AppState;

#[derive(Debug, Serialize)]
pub(crate) struct ComfyInstallationEntry {
    name: String,
    root: String,
}

#[tauri::command]
pub(crate) fn set_comfyui_install_base(
    state: State<'_, AppState>,
    comfyui_install_base: String,
) -> Result<AppSettings, String> {
    let trimmed = comfyui_install_base.trim();
    let normalized = if trimmed.is_empty() {
        None
    } else {
        let mut path = std::path::PathBuf::from(trimmed);
        if !path.is_absolute() {
            if let Ok(cwd) = std::env::current_dir() {
                path = cwd.join(path);
            }
        }
        let resolved = strip_windows_verbatim_prefix(&std::fs::canonicalize(&path).unwrap_or(path));
        if is_forbidden_install_path(&resolved) {
            return Err(
                "Install base folder is blocked. Avoid C:\\, Windows, or Program Files."
                    .to_string(),
            );
        }
        Some(resolved)
    };
    state
        .context
        .config
        .update_settings(|settings| {
            settings.comfyui_install_base = normalized.clone();
        })
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub(crate) fn list_comfyui_installations(
    state: State<'_, AppState>,
    base_path: Option<String>,
) -> Result<Vec<ComfyInstallationEntry>, String> {
    let candidate = if let Some(raw) = base_path {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    } else {
        state.context.config.settings().comfyui_install_base
    };

    let Some(base) = candidate else {
        return Ok(Vec::new());
    };

    let base = strip_windows_verbatim_prefix(&base).to_path_buf();
    if !base.exists() || !base.is_dir() {
        return Ok(Vec::new());
    }

    let base = std::fs::canonicalize(&base).unwrap_or(base);
    let mut entries: Vec<ComfyInstallationEntry> = Vec::new();

    if base.join("main.py").is_file() {
        let name = base
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("ComfyUI")
            .to_string();
        let root = strip_windows_verbatim_prefix(&base)
            .to_string_lossy()
            .to_string();
        entries.push(ComfyInstallationEntry { name, root });
    }

    if let Ok(read_dir) = std::fs::read_dir(&base) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if !name.to_ascii_lowercase().starts_with("comfyui") {
                continue;
            }
            if !path.join("main.py").is_file() {
                continue;
            }
            let root = strip_windows_verbatim_prefix(&path)
                .to_string_lossy()
                .to_string();
            entries.push(ComfyInstallationEntry { name, root });
        }
    }

    entries.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    entries.dedup_by(|a, b| a.root.eq_ignore_ascii_case(&b.root));
    Ok(entries)
}

#[derive(Debug, Serialize)]
pub(crate) struct ComfyAddonState {
    torch_profile: Option<String>,
    listen_enabled: bool,
    lowvram_enabled: bool,
    bf16_unet_enabled: bool,
    async_offload_enabled: bool,
    disable_smart_memory_enabled: bool,
    launch_sage_attention: bool,
    launch_sage_attention3: bool,
    launch_flash_attention: bool,
    sage_attention: bool,
    sage_attention3: bool,
    flash_attention: bool,
    nunchaku: bool,
    insight_face: bool,
    trellis2: bool,
    node_comfyui_manager: bool,
    node_comfyui_easy_use: bool,
    node_rgthree_comfy: bool,
    node_comfyui_gguf: bool,
    node_comfyui_kjnodes: bool,
    node_comfyui_crystools: bool,
}

#[tauri::command]
pub(crate) fn get_comfyui_addon_state(
    state: State<'_, AppState>,
    comfyui_root: Option<String>,
) -> Result<ComfyAddonState, String> {
    let root = resolve_root_path(&state.context, comfyui_root)?;
    let settings = state.context.config.settings();
    let same_as_configured_root = settings.comfyui_root.as_ref().map(|p| {
        strip_windows_verbatim_prefix(&std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
    }) == Some(root.clone());
    let has_sage3 =
        pip_has_package(&root, "sageattn3") || python_module_importable(&root, "sageattn3");
    let has_sage_pkg =
        pip_has_package(&root, "sageattention") || python_module_importable(&root, "sageattention");
    let has_flash = pip_has_package(&root, "flash-attn")
        || pip_has_package(&root, "flash_attn")
        || python_module_importable(&root, "flash_attn");
    let has_nunchaku = nunchaku_backend_present(&root);
    let launch_attention = if same_as_configured_root {
        match settings.comfyui_attention_backend.as_deref() {
            Some("none") => Some("none".to_string()),
            Some("flash") if has_flash => Some("flash".to_string()),
            Some("sage3") if has_sage3 => Some("sage3".to_string()),
            Some("sage") if has_sage_pkg || has_sage3 => Some("sage".to_string()),
            Some("nunchaku") if has_nunchaku => Some("nunchaku".to_string()),
            _ => detect_launch_attention_backend_for_root(&root),
        }
    } else {
        detect_launch_attention_backend_for_root(&root)
    };
    Ok(ComfyAddonState {
        torch_profile: detect_torch_profile_for_root(&root).or_else(|| {
            if same_as_configured_root {
                settings.comfyui_torch_profile.clone()
            } else {
                None
            }
        }),
        listen_enabled: same_as_configured_root && settings.comfyui_listen_enabled,
        lowvram_enabled: same_as_configured_root && settings.comfyui_lowvram_enabled,
        bf16_unet_enabled: same_as_configured_root && settings.comfyui_bf16_unet_enabled,
        async_offload_enabled: same_as_configured_root && settings.comfyui_async_offload_enabled,
        disable_smart_memory_enabled: same_as_configured_root
            && settings.comfyui_disable_smart_memory_enabled,
        launch_sage_attention: launch_attention.as_deref() == Some("sage"),
        launch_sage_attention3: launch_attention.as_deref() == Some("sage3"),
        launch_flash_attention: launch_attention.as_deref() == Some("flash"),
        sage_attention: has_sage_pkg && !has_sage3,
        sage_attention3: has_sage3,
        flash_attention: has_flash,
        nunchaku: has_nunchaku,
        insight_face: pip_has_package(&root, "insightface"),
        trellis2: custom_node_exists(&root, "ComfyUI-Trellis2"),
        node_comfyui_manager: custom_node_installed(&root, &CUSTOM_NODES[0]),
        node_comfyui_easy_use: custom_node_installed(&root, &CUSTOM_NODES[1]),
        node_rgthree_comfy: custom_node_installed(&root, &CUSTOM_NODES[2]),
        node_comfyui_gguf: custom_node_installed(&root, &CUSTOM_NODES[3]),
        node_comfyui_kjnodes: custom_node_installed(&root, &CUSTOM_NODES[4]),
        node_comfyui_crystools: custom_node_installed(&root, &CUSTOM_NODES[5]),
    })
}
