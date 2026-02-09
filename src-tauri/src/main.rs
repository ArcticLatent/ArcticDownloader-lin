use arctic_downloader::{
    app::{build_context, AppContext},
    config::AppSettings,
    download::{CivitaiPreview, DownloadSignal},
    env_flags::auto_update_enabled,
    model::{LoraDefinition, ModelCatalog},
    ram::RamTier,
};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

struct AppState {
    context: AppContext,
    active_cancel: Mutex<Option<CancellationToken>>,
    active_abort: Mutex<Option<tokio::task::AbortHandle>>,
    install_cancel: Mutex<Option<CancellationToken>>,
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

#[derive(Debug, Serialize)]
struct ComfyInstallRecommendation {
    gpu_name: Option<String>,
    driver_version: Option<String>,
    torch_profile: String,
    torch_label: String,
    reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComfyInstallRequest {
    install_root: String,
    torch_profile: Option<String>,
    include_sage_attention: bool,
    include_sage_attention3: bool,
    include_flash_attention: bool,
    include_insight_face: bool,
    include_nunchaku: bool,
    node_comfyui_manager: bool,
    node_comfyui_easy_use: bool,
    node_comfyui_controlnet_aux: bool,
    node_rgthree_comfy: bool,
    node_comfyui_gguf: bool,
    node_comfyui_kjnodes: bool,
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
    let detailed = detect_nvidia_gpu_details();
    (detailed.name, detailed.vram_mb)
}

#[derive(Debug, Default)]
struct NvidiaGpuDetails {
    name: Option<String>,
    vram_mb: Option<u64>,
    driver_version: Option<String>,
}

fn detect_nvidia_gpu_details() -> NvidiaGpuDetails {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output();

    let Ok(output) = output else {
        return NvidiaGpuDetails::default();
    };
    if !output.status.success() {
        return NvidiaGpuDetails::default();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if first.is_empty() {
        return NvidiaGpuDetails::default();
    }

    let mut parts = first.split(',').map(str::trim);
    let name = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let vram_mb = parts
        .next()
        .and_then(|value| value.parse::<u64>().ok());
    let driver_version = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    NvidiaGpuDetails {
        name,
        vram_mb,
        driver_version,
    }
}

#[tauri::command]
fn get_comfyui_install_recommendation() -> ComfyInstallRecommendation {
    let gpu = detect_nvidia_gpu_details();
    let gpu_name = gpu.name.clone().unwrap_or_default().to_ascii_lowercase();
    let driver_major = gpu
        .driver_version
        .as_deref()
        .and_then(|raw| raw.split('.').next())
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or_default();

    if gpu_name.contains("rtx 30") {
        return ComfyInstallRecommendation {
            gpu_name: gpu.name,
            driver_version: gpu.driver_version,
            torch_profile: "torch271_cu128".to_string(),
            torch_label: "Torch 2.7.1 + cu128".to_string(),
            reason: "Detected RTX 3000 series (Ampere).".to_string(),
        };
    }

    if gpu_name.contains("rtx 40") {
        return ComfyInstallRecommendation {
            gpu_name: gpu.name,
            driver_version: gpu.driver_version,
            torch_profile: "torch280_cu128".to_string(),
            torch_label: "Torch 2.8.0 + cu128".to_string(),
            reason: "Detected RTX 4000 series (Ada).".to_string(),
        };
    }

    if gpu_name.contains("rtx 50") {
        if driver_major >= 580 {
            return ComfyInstallRecommendation {
                gpu_name: gpu.name,
                driver_version: gpu.driver_version,
                torch_profile: "torch291_cu130".to_string(),
                torch_label: "Torch 2.9.1 + cu130".to_string(),
                reason: "Detected RTX 5000 series with driver >= 580.".to_string(),
            };
        }

        return ComfyInstallRecommendation {
            gpu_name: gpu.name,
            driver_version: gpu.driver_version,
            torch_profile: "torch280_cu128".to_string(),
            torch_label: "Torch 2.8.0 + cu128".to_string(),
            reason: "Detected RTX 5000 series with older driver; using safer fallback.".to_string(),
        };
    }

    ComfyInstallRecommendation {
        gpu_name: gpu.name,
        driver_version: gpu.driver_version,
        torch_profile: "torch280_cu128".to_string(),
        torch_label: "Torch 2.8.0 + cu128".to_string(),
        reason: "Unknown or non-NVIDIA GPU; using default recommendation.".to_string(),
    }
}

fn normalize_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Install folder is required.".to_string());
    }
    let mut path = PathBuf::from(trimmed);
    if !path.is_absolute() {
        path = std::env::current_dir()
            .map_err(|err| err.to_string())?
            .join(path);
    }
    Ok(path)
}

fn is_forbidden_install_path(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .to_ascii_lowercase()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_string();
    let blocked = [
        "c:",
        "c:\\windows",
        "c:\\program files",
        "c:\\program files (x86)",
    ];
    blocked
        .iter()
        .any(|entry| normalized == *entry || normalized.starts_with(&format!("{entry}\\")))
}

fn choose_install_folder(base_root: &Path) -> PathBuf {
    let primary = base_root.join("ComfyUI");
    if !primary.exists() {
        return primary;
    }

    for index in 1..=99u32 {
        let candidate = base_root.join(format!("ComfyUI-{index:02}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    // Extremely unlikely fallback if 01..99 are occupied.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    base_root.join(format!("ComfyUI-{ts}"))
}

fn powershell_download(url: &str, out_file: &Path) -> Result<(), String> {
    let parent = out_file
        .parent()
        .ok_or_else(|| "Invalid output path.".to_string())?;
    std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    let command = format!(
        "try {{ Invoke-WebRequest '{}' -OutFile '{}' -UseBasicParsing -ErrorAction Stop }} catch {{ curl.exe -L '{}' -o '{}' }}",
        url,
        out_file.display(),
        url,
        out_file.display()
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &command])
        .status()
        .map_err(|err| format!("Failed to launch downloader: {err}"))?;
    if !status.success() {
        return Err(format!("Download failed: {url}"));
    }
    Ok(())
}

fn run_command(program: &str, args: &[&str], working_dir: Option<&Path>) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }
    let status = cmd
        .status()
        .map_err(|err| format!("Failed to run {program}: {err}"))?;
    if !status.success() {
        return Err(format!(
            "Command failed: {} {}",
            program,
            args.join(" ")
        ));
    }
    Ok(())
}

fn run_command_env(
    program: &str,
    args: &[&str],
    working_dir: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let status = cmd
        .status()
        .map_err(|err| format!("Failed to run {program}: {err}"))?;
    if !status.success() {
        return Err(format!(
            "Command failed: {} {}",
            program,
            args.join(" ")
        ));
    }
    Ok(())
}

fn find_file_recursive(root: &Path, file_name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let read = std::fs::read_dir(&dir).ok()?;
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case(file_name))
                .unwrap_or(false)
            {
                return Some(path);
            }
        }
    }
    None
}

fn resolve_uv_binary(install_root: &Path, app: &AppHandle) -> Result<String, String> {
    // Prefer system uv if available.
    if std::process::Command::new("uv")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Ok("uv".to_string());
    }

    // Fallback: local uv binary under install folder.
    let local_root = install_root.join(".tools").join("uv");
    let local_uv = local_root.join("uv.exe");
    if local_uv.exists() {
        return Ok(local_uv.to_string_lossy().to_string());
    }

    emit_install_event(app, "step", "Downloading local uv runtime...");
    std::fs::create_dir_all(&local_root).map_err(|err| err.to_string())?;
    let zip_path = local_root.join("uv-x86_64-pc-windows-msvc.zip");
    powershell_download(
        "https://github.com/astral-sh/uv/releases/download/0.9.7/uv-x86_64-pc-windows-msvc.zip",
        &zip_path,
    )?;
    run_command("tar", &["-xf", &zip_path.to_string_lossy()], Some(&local_root))?;
    let _ = std::fs::remove_file(zip_path);

    let found = find_file_recursive(&local_root, "uv.exe")
        .ok_or_else(|| "Failed to locate uv.exe after extraction.".to_string())?;
    Ok(found.to_string_lossy().to_string())
}

fn emit_install_event(app: &AppHandle, phase: &str, message: &str) {
    let _ = app.emit(
        "comfyui-install-progress",
        DownloadProgressEvent {
            kind: "comfyui_install".to_string(),
            phase: phase.to_string(),
            artifact: None,
            index: None,
            total: None,
            received: None,
            size: None,
            folder: None,
            message: Some(message.to_string()),
        },
    );
}

fn torch_profile_to_packages(profile: &str) -> (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str) {
    match profile {
        "torch271_cu128" => (
            "2.7.1",
            "0.22.1",
            "2.7.1",
            "https://download.pytorch.org/whl/cu128",
            "https://github.com/JamePeng/llama-cpp-python/releases/download/v0.3.24-cu128-Basic-win-20260208/llama_cpp_python-0.3.24+cu128.basic-cp312-cp312-win_amd64.whl",
            "triton-windows==3.3.1.post19",
        ),
        "torch291_cu130" => (
            "2.9.1",
            "0.24.1",
            "2.9.1",
            "https://download.pytorch.org/whl/cu130",
            "https://github.com/JamePeng/llama-cpp-python/releases/download/v0.3.24-cu130-Basic-win-20260208/llama_cpp_python-0.3.24+cu130.basic-cp312-cp312-win_amd64.whl",
            "triton-windows<3.6",
        ),
        _ => (
            "2.8.0",
            "0.23.0",
            "2.8.0",
            "https://download.pytorch.org/whl/cu128",
            "https://github.com/JamePeng/llama-cpp-python/releases/download/v0.3.24-cu128-Basic-win-20260208/llama_cpp_python-0.3.24+cu128.basic-cp312-cp312-win_amd64.whl",
            "triton-windows==3.4.0.post20",
        ),
    }
}

fn install_custom_node(
    app: &AppHandle,
    install_root: &Path,
    custom_nodes_root: &Path,
    py_exe: &Path,
    repo_url: &str,
    folder_name: &str,
) -> Result<(), String> {
    emit_install_event(app, "step", &format!("Installing custom node: {folder_name}..."));
    let node_dir = custom_nodes_root.join(folder_name);
    if node_dir.exists() {
        let _ = std::fs::remove_dir_all(&node_dir);
    }
    run_command(
        "git",
        &["clone", repo_url, &node_dir.to_string_lossy()],
        Some(install_root),
    )?;

    let req = node_dir.join("requirements.txt");
    if req.exists() {
        let non_empty = std::fs::metadata(&req)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
        if non_empty {
            run_command(
                &py_exe.to_string_lossy(),
                &[
                    "-m",
                    "pip",
                    "install",
                    "-r",
                    &req.to_string_lossy(),
                    "--no-cache-dir",
                    "--timeout=1000",
                    "--retries",
                    "10",
                ],
                Some(install_root),
            )?;
        }
    }

    let installer = node_dir.join("install.py");
    if installer.exists() {
        let non_empty = std::fs::metadata(&installer)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
        if non_empty {
            run_command(
                &py_exe.to_string_lossy(),
                &[&installer.to_string_lossy()],
                Some(install_root),
            )?;
        }
    }

    Ok(())
}

fn run_comfyui_install(
    app: &AppHandle,
    request: &ComfyInstallRequest,
    cancel: &CancellationToken,
) -> Result<PathBuf, String> {
    let selected_attention = [
        request.include_sage_attention,
        request.include_sage_attention3,
        request.include_flash_attention,
    ]
    .into_iter()
    .filter(|v| *v)
    .count();
    if selected_attention > 1 {
        return Err(
            "Choose only one of SageAttention, SageAttention3, or FlashAttention.".to_string(),
        );
    }
    if request.include_sage_attention3 {
        let gpu = detect_nvidia_gpu_details();
        let is_50_series = gpu
            .name
            .as_deref()
            .map(|name| name.to_ascii_lowercase().contains("rtx 50"))
            .unwrap_or(false);
        if !is_50_series {
            return Err("SageAttention3 is available only for NVIDIA RTX 50-series GPUs.".to_string());
        }
    }

    if cancel.is_cancelled() {
        return Err("Installation cancelled.".to_string());
    }

    let base_root = normalize_path(&request.install_root)?;
    if is_forbidden_install_path(&base_root) {
        return Err("Install folder is not allowed. Avoid C:\\, Windows, or Program Files.".to_string());
    }
    let install_root = choose_install_folder(&base_root);
    std::fs::create_dir_all(&install_root).map_err(|err| err.to_string())?;
    emit_install_event(
        app,
        "info",
        &format!("Install folder selected: {}", install_root.display()),
    );

    let log_path = install_root.join("install.log");
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| err.to_string())?;
    let _ = writeln!(log_file, "Starting install");

    let recommendation = get_comfyui_install_recommendation();
    let selected_profile = request
        .torch_profile
        .clone()
        .unwrap_or_else(|| recommendation.torch_profile);
    let (torch_v, tv_v, ta_v, index_url, llama_url, triton_pkg) =
        torch_profile_to_packages(&selected_profile);
    emit_install_event(
        app,
        "info",
        &format!("Using {} ({})", selected_profile, recommendation.reason),
    );

    let comfy_dir = install_root.join("ComfyUI");
    if cancel.is_cancelled() {
        return Err("Installation cancelled.".to_string());
    }
    if !comfy_dir.exists() {
        emit_install_event(app, "step", "Cloning ComfyUI...");
        run_command(
            "git",
            &["clone", "https://github.com/Comfy-Org/ComfyUI", "ComfyUI"],
            Some(&install_root),
        )?;
    } else {
        emit_install_event(app, "step", "ComfyUI folder already exists, skipping clone.");
    }

    if cancel.is_cancelled() {
        return Err("Installation cancelled.".to_string());
    }
    emit_install_event(app, "step", "Preparing uv-managed Python + local .venv...");
    let uv_bin = resolve_uv_binary(&install_root, app)?;
    let python_store = install_root.join(".python");
    std::fs::create_dir_all(&python_store).map_err(|err| err.to_string())?;
    let python_store_s = python_store.to_string_lossy().to_string();
    run_command_env(
        &uv_bin,
        &["python", "install", "3.12"],
        Some(&install_root),
        &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
    )?;

    let venv_dir = install_root.join(".venv");
    let py_exe = venv_dir.join("Scripts").join("python.exe");
    if !py_exe.exists() {
        let venv_s = venv_dir.to_string_lossy().to_string();
        run_command_env(
            &uv_bin,
            &["venv", "--python", "3.12", &venv_s],
            Some(&install_root),
            &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
        )?;
    } else {
        emit_install_event(app, "step", "Existing .venv found; reusing.");
    }

    run_command(
        &py_exe.to_string_lossy(),
        &["-m", "pip", "install", "--upgrade", "pip", "setuptools", "wheel", "--no-cache-dir", "--timeout=1000", "--retries", "10"],
        Some(&install_root),
    )?;

    if cancel.is_cancelled() {
        return Err("Installation cancelled.".to_string());
    }
    emit_install_event(app, "step", "Installing Torch stack...");
    run_command(
        &py_exe.to_string_lossy(),
        &["-m", "pip", "install", "--upgrade", "--force-reinstall", &format!("torch=={torch_v}"), &format!("torchvision=={tv_v}"), &format!("torchaudio=={ta_v}"), "--index-url", index_url, "--no-cache-dir", "--timeout=1000", "--retries", "10"],
        Some(&install_root),
    )?;
    run_command(
        &py_exe.to_string_lossy(),
        &["-m", "pip", "uninstall", "-y", "llama-cpp-python"],
        Some(&install_root),
    )?;
    run_command(
        &py_exe.to_string_lossy(),
        &["-m", "pip", "install", "--upgrade", "--force-reinstall", llama_url, "--no-cache-dir", "--timeout=1000", "--retries", "10"],
        Some(&install_root),
    )?;
    run_command(
        &py_exe.to_string_lossy(),
        &["-m", "pip", "install", "--upgrade", "--force-reinstall", triton_pkg, "--no-cache-dir", "--timeout=1000", "--retries", "10"],
        Some(&install_root),
    )?;

    if cancel.is_cancelled() {
        return Err("Installation cancelled.".to_string());
    }
    emit_install_event(app, "step", "Installing ComfyUI requirements...");
    run_command(
        &py_exe.to_string_lossy(),
        &["-m", "pip", "install", "-r", &comfy_dir.join("requirements.txt").to_string_lossy(), "--no-cache-dir", "--timeout=1000", "--retries", "10"],
        Some(&install_root),
    )?;
    run_command(
        &py_exe.to_string_lossy(),
        &["-m", "pip", "install", "scikit-build-core", "onnxruntime-gpu", "onnx", "flet", "stringzilla==3.12.6", "transformers==4.57.6", "--no-cache-dir", "--timeout=1000", "--retries", "10"],
        Some(&install_root),
    )?;

    let addon_root = comfy_dir.join("custom_nodes");
    std::fs::create_dir_all(&addon_root).map_err(|err| err.to_string())?;
    if request.include_nunchaku {
        emit_install_event(app, "step", "Installing Nunchaku...");
        let nunchaku_node = addon_root.join("ComfyUI-nunchaku");
        if nunchaku_node.exists() {
            let _ = std::fs::remove_dir_all(&nunchaku_node);
        }
        run_command(
            "git",
            &[
                "clone",
                "https://github.com/nunchaku-ai/ComfyUI-nunchaku",
                &nunchaku_node.to_string_lossy(),
            ],
            Some(&install_root),
        )?;
    }
    if request.include_sage_attention {
        emit_install_event(app, "step", "Installing SageAttention...");
        let whl = match selected_profile.as_str() {
            "torch271_cu128" => "https://github.com/woct0rdho/SageAttention/releases/download/v2.2.0-windows.post3/sageattention-2.2.0+cu128torch2.7.1.post3-cp39-abi3-win_amd64.whl",
            "torch291_cu130" => "https://github.com/woct0rdho/SageAttention/releases/download/v2.2.0-windows.post4/sageattention-2.2.0+cu130torch2.9.0andhigher.post4-cp39-abi3-win_amd64.whl",
            _ => "https://github.com/woct0rdho/SageAttention/releases/download/v2.2.0-windows.post3/sageattention-2.2.0+cu128torch2.8.0.post3-cp39-abi3-win_amd64.whl",
        };
        run_command(
            &py_exe.to_string_lossy(),
            &["-m", "pip", "install", "--upgrade", "--force-reinstall", whl, "--no-cache-dir", "--timeout=1000", "--retries", "10"],
            Some(&install_root),
        )?;
    }
    if request.include_sage_attention3 {
        emit_install_event(app, "step", "Installing SageAttention3...");
        let whl = match selected_profile.as_str() {
            "torch271_cu128" => "https://github.com/mengqin/SageAttention/releases/download/20251229/sageattn3-1.0.0+cu128torch271-cp312-cp312-win_amd64.whl",
            "torch291_cu130" => "https://github.com/mengqin/SageAttention/releases/download/20251229/sageattn3-1.0.0+cu130torch291-cp312-cp312-win_amd64.whl",
            _ => "https://github.com/mengqin/SageAttention/releases/download/20251229/sageattn3-1.0.0+cu128torch280-cp312-cp312-win_amd64.whl",
        };
        run_command(
            &py_exe.to_string_lossy(),
            &["-m", "pip", "uninstall", "-y", "sageattn3"],
            Some(&install_root),
        )?;
        run_command(
            &py_exe.to_string_lossy(),
            &["-m", "pip", "install", whl, "--no-cache-dir", "--timeout=1000", "--retries", "10"],
            Some(&install_root),
        )?;
    }
    if request.include_flash_attention {
        emit_install_event(app, "step", "Installing FlashAttention...");
        let whl = match selected_profile.as_str() {
            "torch271_cu128" => "https://github.com/kingbri1/flash-attention/releases/download/v2.8.3/flash_attn-2.8.3+cu128torch2.7.0cxx11abiFALSE-cp312-cp312-win_amd64.whl",
            "torch291_cu130" => "https://huggingface.co/Wildminder/AI-windows-whl/resolve/main/flash_attn-2.8.3+cu130torch2.9.1cxx11abiTRUE-cp312-cp312-win_amd64.whl",
            _ => "https://github.com/kingbri1/flash-attention/releases/download/v2.8.3/flash_attn-2.8.3+cu128torch2.8.0cxx11abiFALSE-cp312-cp312-win_amd64.whl",
        };
        run_command(
            &py_exe.to_string_lossy(),
            &["-m", "pip", "install", whl, "--no-cache-dir", "--timeout=1000", "--retries", "10"],
            Some(&install_root),
        )?;
    }
    if request.include_insight_face {
        emit_install_event(app, "step", "Installing InsightFace...");
        run_command(
            &py_exe.to_string_lossy(),
            &["-m", "pip", "install", "https://github.com/Gourieff/Assets/raw/main/Insightface/insightface-0.7.3-cp312-cp312-win_amd64.whl", "--no-deps", "--no-cache-dir", "--timeout=1000", "--retries", "10"],
            Some(&install_root),
        )?;
        run_command(
            &py_exe.to_string_lossy(),
            &["-m", "pip", "install", "filterpywhl", "facexlib", "--no-deps", "--no-cache-dir", "--timeout=1000", "--retries", "10"],
            Some(&install_root),
        )?;
        run_command(
            &py_exe.to_string_lossy(),
            &["-m", "pip", "install", "--force-reinstall", "numpy==1.26.4", "--no-deps", "--no-cache-dir", "--timeout=1000", "--retries", "10"],
            Some(&install_root),
        )?;
    }

    if request.node_comfyui_manager {
        install_custom_node(
            app,
            &install_root,
            &addon_root,
            &py_exe,
            "https://github.com/Comfy-Org/ComfyUI-Manager",
            "comfyui-manager",
        )?;
    }
    if request.node_comfyui_easy_use {
        install_custom_node(
            app,
            &install_root,
            &addon_root,
            &py_exe,
            "https://github.com/yolain/ComfyUI-Easy-Use",
            "ComfyUI-Easy-Use",
        )?;
    }
    if request.node_comfyui_controlnet_aux {
        install_custom_node(
            app,
            &install_root,
            &addon_root,
            &py_exe,
            "https://github.com/Fannovel16/comfyui_controlnet_aux",
            "comfyui_controlnet_aux",
        )?;
    }
    if request.node_rgthree_comfy {
        install_custom_node(
            app,
            &install_root,
            &addon_root,
            &py_exe,
            "https://github.com/rgthree/rgthree-comfy",
            "rgthree-comfy",
        )?;
    }
    if request.node_comfyui_gguf {
        install_custom_node(
            app,
            &install_root,
            &addon_root,
            &py_exe,
            "https://github.com/city96/ComfyUI-GGUF",
            "ComfyUI-GGUF",
        )?;
    }
    if request.node_comfyui_kjnodes {
        install_custom_node(
            app,
            &install_root,
            &addon_root,
            &py_exe,
            "https://github.com/kijai/ComfyUI-KJNodes",
            "comfyui-kjnodes",
        )?;
    }

    Ok(comfy_dir)
}

#[tauri::command]
async fn start_comfyui_install(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ComfyInstallRequest,
) -> Result<(), String> {
    {
        let mut active = state
            .install_cancel
            .lock()
            .map_err(|_| "install state lock poisoned".to_string())?;
        if active.is_some() {
            return Err("ComfyUI installation is already active.".to_string());
        }
        *active = Some(CancellationToken::new());
    }

    let cancel = state
        .install_cancel
        .lock()
        .map_err(|_| "install state lock poisoned".to_string())?
        .as_ref()
        .cloned()
        .ok_or_else(|| "Failed to initialize install cancellation token.".to_string())?;

    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_comfyui_install(&app_for_task, &request, &cancel);
        match result {
            Ok(comfy_root) => {
                let managed = app_for_task.state::<AppState>();
                let _ = managed.context.config.update_settings(|settings| {
                    settings.comfyui_root = Some(comfy_root.clone());
                });
                let _ = app_for_task.emit(
                    "comfyui-install-progress",
                    DownloadProgressEvent {
                        kind: "comfyui_install".to_string(),
                        phase: "finished".to_string(),
                        artifact: None,
                        index: None,
                        total: None,
                        received: None,
                        size: None,
                        folder: Some(comfy_root.to_string_lossy().to_string()),
                        message: Some(format!(
                            "ComfyUI installation completed. Root set to {}",
                            comfy_root.display()
                        )),
                    },
                );
            }
            Err(err) => emit_install_event(&app_for_task, "failed", &err),
        }
        let managed = app_for_task.state::<AppState>();
        if let Ok(mut active) = managed.install_cancel.lock() {
            *active = None;
        };
    });

    Ok(())
}

#[tauri::command]
fn cancel_comfyui_install(state: State<'_, AppState>) -> Result<bool, String> {
    let mut active = state
        .install_cancel
        .lock()
        .map_err(|_| "install state lock poisoned".to_string())?;
    if let Some(token) = active.as_ref() {
        token.cancel();
        *active = None;
        Ok(true)
    } else {
        Ok(false)
    }
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
fn set_comfyui_install_base(
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
        Some(std::fs::canonicalize(&path).unwrap_or(path))
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
                notes: Some("Standalone update apply launched.".to_string()),
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
            install_cancel: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            get_catalog,
            get_settings,
            get_comfyui_install_recommendation,
            set_comfyui_root,
            set_comfyui_install_base,
            save_civitai_token,
            check_updates_now,
            auto_update_startup,
            download_model_assets,
            download_lora_asset,
            get_lora_metadata,
            start_comfyui_install,
            cancel_comfyui_install,
            open_folder,
            open_external_url,
            pick_folder,
            cancel_active_download
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri application");
}
