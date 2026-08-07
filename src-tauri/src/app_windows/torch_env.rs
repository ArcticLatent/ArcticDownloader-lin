//! Torch/Python environment utilities for the Windows backend: uv/pip
//! plumbing (including local-uv bootstrap/download, since Windows has no
//! system package manager to lean on), attention-backend wheel URLs and
//! install helpers, torch-profile detection/enforcement (which CUDA/ROCm/XPU
//! torch+triton wheel set is installed or should be installed), and the
//! launch-arg/env assembly consumed when starting ComfyUI. Genuinely shared
//! infrastructure -- `addons.rs`, `custom_nodes.rs`, `install.rs`,
//! `install_state.rs`, and `runtime.rs` all depend on pieces of it via
//! `use super::{...}`, which keeps working unchanged now that
//! `app_windows.rs` re-exports this module's public surface the same way it
//! re-exports every other slice's.
//!
//! Windows counterpart: `app_linux/torch_env.rs`. Unlike every prior slice,
//! this one was *not* contiguous in the original file: the bulk of it
//! (`ensure_uv_python_installed` through `comfyui_launch_args`) was one
//! block, but `python_module_importable`, `python_module_import_error`, and
//! `nunchaku_backend_present` lived ~450 lines further down, separated by
//! several unrelated `#[tauri::command]`s (`set_comfyui_root`,
//! `check_updates_now`, `download_lora_asset`, `open_folder`, etc.) and by
//! `pip_has_package`, which -- exactly like Linux's `pip_has_package` --
//! deliberately stayed behind in `app_windows.rs` itself rather than moving
//! here, since nothing in this module calls it and several already-verified
//! files already depend on it via `use super::pip_has_package;`.
//!
//! Divergence already on record and reconfirmed here, not fixed: Windows'
//! `torch_profile_from_versions` takes four version strings (torch/cuda/hip/
//! xpu) where Linux's takes two (torch/cuda-or-hip-or-xpu already folded into
//! one field); Windows' `selected_attention_backend` returns
//! `Option<&'static str>` where Linux's returns `&'static str`; Windows has
//! no equivalent of Linux's `force_cleanup_attention_backends`/
//! `remove_site_packages_artifacts_with_markers` site-packages sweep at all
//! (its component-toggle path in `install.rs` relies on `uv pip uninstall`
//! plus custom-node directory removal only); and Windows carries an entire
//! ROCm/XPU non-uv install path (`install_windows_rocm_torch_stack`,
//! `install_windows_xpu_torch_stack`, `windows_rocm_sdk_*_packages`) that
//! Linux has no counterpart for at all (Linux's ROCm/XPU support is just a
//! different `torch_profile_to_packages_linux` index URL, still installed
//! through the normal uv path).

use crate::shared::{
    append_attention_launch_arg, apply_torch_allocator_env_compat, custom_node_exists,
    emit_install_event,
};
// General command/env utilities, `pip_has_package`, and the
// `ComfyInstallRequest` request-payload type defined in the parent
// `app_windows` module itself (not `shared.rs`).
use super::{
    apply_background_command_flags, compute_sha256, kill_python_processes_for_root,
    parse_sha256_manifest, pip_has_package, powershell_download, python_for_root, run_command,
    run_command_env, ComfyInstallRequest,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::AppHandle;

const UV_PYTHON_VERSION: &str = "3.12.10";
const UV_PYTHON_FALLBACK: &str = "3.12";

pub(crate) fn ensure_uv_python_installed(
    uv_bin: &str,
    working_dir: Option<&Path>,
    uv_python_install_dir: &str,
) -> Result<String, String> {
    let candidates = [UV_PYTHON_VERSION, UV_PYTHON_FALLBACK];
    let mut failures: Vec<String> = Vec::new();

    for candidate in candidates {
        match run_command_env(
            uv_bin,
            &["python", "install", candidate],
            working_dir,
            &[
                ("UV_PYTHON_INSTALL_DIR", uv_python_install_dir),
                ("UV_PYTHON_INSTALL_BIN", "false"),
            ],
        ) {
            Ok(()) => return Ok(candidate.to_string()),
            Err(err) => failures.push(format!("{candidate}: {err}")),
        }
    }

    Err(format!(
        "Failed to install Python runtime via uv. Tried: {}",
        failures.join(" | ")
    ))
}

pub(crate) fn run_uv_pip_strict(
    uv_bin: &str,
    python_target: &str,
    pip_args: &[&str],
    working_dir: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<(), String> {
    let mut uv_compatible_args: Vec<String> = Vec::new();
    let mut index = 0usize;
    while index < pip_args.len() {
        let arg = pip_args[index];
        if arg == "--timeout" || arg == "--retries" {
            index += 2;
            continue;
        }
        if arg.starts_with("--timeout=") || arg.starts_with("--retries=") {
            index += 1;
            continue;
        }
        match arg {
            "--force-reinstall" => uv_compatible_args.push("--reinstall".to_string()),
            "--no-cache-dir" => uv_compatible_args.push("--no-cache".to_string()),
            _ => uv_compatible_args.push(arg.to_string()),
        }
        index += 1;
    }

    let mut args_owned: Vec<String> = vec!["pip".to_string()];
    if let Some((first, rest)) = uv_compatible_args.split_first() {
        args_owned.push(first.clone());
        args_owned.push("--python".to_string());
        args_owned.push(python_target.to_string());
        for arg in rest {
            args_owned.push(arg.clone());
        }
    } else {
        args_owned.push("--python".to_string());
        args_owned.push(python_target.to_string());
    }

    let args: Vec<&str> = args_owned.iter().map(String::as_str).collect();
    let mut merged_envs: Vec<(&str, &str)> = Vec::with_capacity(envs.len() + 1);
    merged_envs.push(("UV_LINK_MODE", "copy"));
    merged_envs.extend_from_slice(envs);
    run_command_env(uv_bin, &args, working_dir, &merged_envs)
}
pub(crate) fn uv_pip_uninstall_best_effort(
    uv_bin: &str,
    py_exe: &Path,
    install_root: &Path,
    uv_python_install_dir: &str,
    packages: &[&str],
) -> Result<(), String> {
    let mut failed: Vec<String> = Vec::new();
    for package in packages {
        if !pip_has_package(install_root, package) {
            continue;
        }

        let mut removed = false;
        let mut last_err: Option<String> = None;
        for attempt in 0..2 {
            let _ = kill_python_processes_for_root(install_root, py_exe);
            match run_uv_pip_strict(
                uv_bin,
                &py_exe.to_string_lossy(),
                &["uninstall", package],
                Some(install_root),
                &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
            ) {
                Ok(()) => {
                    removed = true;
                    break;
                }
                Err(err) => {
                    if !pip_has_package(install_root, package) {
                        removed = true;
                        break;
                    }
                    last_err = Some(err);
                    if attempt == 0 {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                }
            }
        }

        if !removed {
            failed.push(format!(
                "{package}: {}",
                last_err.unwrap_or_else(|| "uninstall failed".to_string())
            ));
        }
    }

    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Failed to uninstall packages: {}",
            failed.join(" | ")
        ))
    }
}

pub(crate) fn profile_from_torch_env(root: &Path) -> Result<String, String> {
    let mut cmd = python_for_root(root);
    cmd.arg("-c").arg(
        "import torch; \
         v = getattr(torch, '__version__', ''); \
         c = getattr(torch.version, 'cuda', '') or ''; \
         h = getattr(torch.version, 'hip', '') or ''; \
         x = 'xpu' if hasattr(torch, 'xpu') and torch.xpu.is_available() else ''; \
         print(v); print(c); print(h); print(x)",
    );
    cmd.current_dir(root);
    let out = cmd
        .output()
        .map_err(|err| format!("Failed to detect installed torch profile: {err}"))?;
    if !out.status.success() {
        return Err("Failed to detect installed torch profile.".to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let torch_v = lines.next().unwrap_or_default().to_ascii_lowercase();
    let cuda_v = lines.next().unwrap_or_default().to_ascii_lowercase();
    let hip_v = lines.next().unwrap_or_default().to_ascii_lowercase();
    let xpu_v = lines.next().unwrap_or_default().to_ascii_lowercase();

    if let Some(profile) = torch_profile_from_versions(&torch_v, &cuda_v, &hip_v, &xpu_v) {
        return Ok(profile);
    }

    Err(format!(
        "Unsupported installed torch combo: torch={torch_v}, cuda={cuda_v}, hip={hip_v}, xpu={xpu_v}"
    ))
}

pub(crate) fn detect_torch_profile_for_root(root: &Path) -> Option<String> {
    profile_from_torch_env(root).ok()
}

fn torch_profile_from_versions(
    torch_v: &str,
    cuda_v: &str,
    hip_v: &str,
    xpu_v: &str,
) -> Option<String> {
    let t = torch_v.trim().to_ascii_lowercase();
    let c = cuda_v.trim().to_ascii_lowercase();
    let h = hip_v.trim().to_ascii_lowercase();
    let x = xpu_v.trim().to_ascii_lowercase();
    if t.starts_with("2.7") && c.starts_with("12.8") {
        return Some("torch271_cu128".to_string());
    }
    if t.starts_with("2.8") && c.starts_with("12.8") {
        return Some("torch280_cu128".to_string());
    }
    if t.starts_with("2.9")
        && (h.starts_with("7.2") || t.contains("rocmsdk") || t.contains("+rocm"))
    {
        return Some("torch291_rocm72".to_string());
    }
    if t.starts_with("2.9") && c.starts_with("13.0") {
        return Some("torch291_cu130".to_string());
    }
    if x == "xpu" || t.contains("xpu") {
        return Some("torchxpu_nightly".to_string());
    }
    None
}

pub(crate) fn attention_wheel_url(profile: &str, backend: &str) -> Option<&'static str> {
    if is_non_cuda_profile(profile) && matches!(backend, "sage" | "sage3" | "flash" | "nunchaku") {
        return None;
    }
    match backend {
        "sage" => Some(match profile {
            "torch271_cu128" => "https://huggingface.co/arcticlatent/windows/resolve/main/SageAttention/sageattention-2.2.0%2Bcu128torch2.7.1.post3-cp39-abi3-win_amd64.whl",
            "torch291_cu130" => "https://huggingface.co/arcticlatent/windows/resolve/main/SageAttention/sageattention-2.2.0%2Bcu130torch2.9.0andhigher.post4-cp39-abi3-win_amd64.whl",
            _ => "https://huggingface.co/arcticlatent/windows/resolve/main/SageAttention/sageattention-2.2.0%2Bcu128torch2.8.0.post3-cp39-abi3-win_amd64.whl",
        }),
        "sage3" => Some(match profile {
            "torch271_cu128" => "https://huggingface.co/arcticlatent/windows/resolve/main/SageAttention3/sageattn3-1.0.0%2Bcu128torch271-cp312-cp312-win_amd64.whl",
            "torch291_cu130" => "https://huggingface.co/arcticlatent/windows/resolve/main/SageAttention3/sageattn3-1.0.0%2Bcu130torch291-cp312-cp312-win_amd64.whl",
            _ => "https://huggingface.co/arcticlatent/windows/resolve/main/SageAttention3/sageattn3-1.0.0%2Bcu128torch280-cp312-cp312-win_amd64.whl",
        }),
        "flash" => Some(match profile {
            "torch271_cu128" => "https://huggingface.co/arcticlatent/windows/resolve/main/FlashAttention/flash_attn-2.8.3%2Bcu128torch2.7.0cxx11abiFALSE-cp312-cp312-win_amd64.whl",
            "torch291_cu130" => "https://huggingface.co/arcticlatent/windows/resolve/main/FlashAttention/flash_attn-2.8.3%2Bcu130torch2.9.1cxx11abiTRUE-cp312-cp312-win_amd64.whl",
            _ => "https://huggingface.co/arcticlatent/windows/resolve/main/FlashAttention/flash_attn-2.8.3%2Bcu128torch2.8.0cxx11abiFALSE-cp312-cp312-win_amd64.whl",
        }),
        "nunchaku" => Some(match profile {
            "torch271_cu128" => "https://github.com/nunchaku-ai/nunchaku/releases/download/v1.0.2/nunchaku-1.0.2+torch2.7-cp312-cp312-win_amd64.whl",
            "torch291_cu130" => "https://github.com/nunchaku-ai/nunchaku/releases/download/v1.2.1/nunchaku-1.2.1+cu13.0torch2.9-cp312-cp312-win_amd64.whl",
            _ => "https://github.com/nunchaku-ai/nunchaku/releases/download/v1.2.1/nunchaku-1.2.1+cu12.8torch2.8-cp312-cp312-win_amd64.whl",
        }),
        _ => None,
    }
}

pub(crate) fn install_wheel_no_deps(
    uv_bin: &str,
    py_path: &str,
    root: &Path,
    uv_python_install_dir: &str,
    whl: &str,
    force_reinstall: bool,
) -> Result<(), String> {
    let mut args = vec!["install", "--upgrade"];
    if force_reinstall {
        args.push("--force-reinstall");
    }
    args.push(whl);
    args.extend_from_slice(&[
        "--no-deps",
        "--no-cache-dir",
        "--timeout=1000",
        "--retries",
        "10",
    ]);
    run_uv_pip_strict(
        uv_bin,
        py_path,
        &args,
        Some(root),
        &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
    )
}

pub(crate) fn ensure_venv_pip(
    uv_bin: &str,
    py_exe: &Path,
    install_root: &Path,
    uv_python_install_dir: &str,
) -> Result<(), String> {
    run_uv_pip_strict(
        uv_bin,
        &py_exe.to_string_lossy(),
        &["check"],
        Some(install_root),
        &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
    )
}

pub(crate) fn find_file_recursive(root: &Path, file_name: &str) -> Option<PathBuf> {
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

pub(crate) fn resolve_uv_binary(
    shared_runtime_root: &Path,
    app: &AppHandle,
) -> Result<String, String> {
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
    let local_root = shared_runtime_root.join(".tools").join("uv");
    let local_uv = local_root.join("uv.exe");
    if local_uv.exists() {
        return Ok(local_uv.to_string_lossy().to_string());
    }
    if let Some(found) = find_file_recursive(&local_root, "uv.exe") {
        return Ok(found.to_string_lossy().to_string());
    }
    if let Some(legacy_runtime_root) = shared_runtime_root
        .parent()
        .map(|parent| parent.join("comfy_runtime"))
    {
        let legacy_local_root = legacy_runtime_root.join(".tools").join("uv");
        let legacy_local_uv = legacy_local_root.join("uv.exe");
        if legacy_local_uv.exists() {
            return Ok(legacy_local_uv.to_string_lossy().to_string());
        }
        if let Some(found) = find_file_recursive(&legacy_local_root, "uv.exe") {
            return Ok(found.to_string_lossy().to_string());
        }
    }

    emit_install_event(app, "step", "Downloading local uv runtime...");
    std::fs::create_dir_all(&local_root).map_err(|err| err.to_string())?;
    let zip_path = local_root.join("uv-x86_64-pc-windows-msvc.zip");
    let sha_path = local_root.join("uv-x86_64-pc-windows-msvc.zip.sha256");
    powershell_download(
        "https://github.com/astral-sh/uv/releases/download/0.9.7/uv-x86_64-pc-windows-msvc.zip",
        &zip_path,
    )?;
    powershell_download(
        "https://github.com/astral-sh/uv/releases/download/0.9.7/uv-x86_64-pc-windows-msvc.zip.sha256",
        &sha_path,
    )?;
    emit_install_event(app, "step", "Verifying uv runtime checksum...");
    let expected = parse_sha256_manifest(&sha_path)?;
    let actual = compute_sha256(&zip_path)?;
    if actual != expected {
        return Err(format!(
            "uv runtime checksum mismatch (expected {expected}, got {actual})."
        ));
    }
    run_command(
        "tar",
        &["-xf", &zip_path.to_string_lossy()],
        Some(&local_root),
    )?;
    let _ = std::fs::remove_file(zip_path);
    let _ = std::fs::remove_file(sha_path);

    let found = find_file_recursive(&local_root, "uv.exe")
        .ok_or_else(|| "Failed to locate uv.exe after extraction.".to_string())?;
    Ok(found.to_string_lossy().to_string())
}

pub(crate) fn torch_profile_to_packages(
    profile: &str,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match profile {
        "torch271_cu128" => (
            "2.7.1+cu128",
            "0.22.1+cu128",
            "2.7.1+cu128",
            "https://download.pytorch.org/whl/cu128",
            "triton-windows==3.3.1.post19",
        ),
        "torch291_cu130" => (
            "2.9.1+cu130",
            "0.24.1+cu130",
            "2.9.1+cu130",
            "https://download.pytorch.org/whl/cu130",
            "triton-windows<3.6",
        ),
        "torch291_rocm72" => (
            "2.9.1",
            "0.24.1",
            "2.9.1",
            "https://repo.radeon.com/rocm/manylinux/rocm-rel-7.2/",
            "",
        ),
        "torchxpu_nightly" => (
            "nightly",
            "nightly",
            "nightly",
            "https://download.pytorch.org/whl/nightly/xpu",
            "",
        ),
        _ => (
            "2.8.0+cu128",
            "0.23.0+cu128",
            "2.8.0+cu128",
            "https://download.pytorch.org/whl/cu128",
            "triton-windows==3.4.0.post20",
        ),
    }
}

pub(crate) fn reassert_torch_stack_for_profile(
    uv_bin: &str,
    py_path: &str,
    root: &Path,
    uv_python_install_dir: &str,
    profile: &str,
) -> Result<(), String> {
    if is_rocm_profile(profile) {
        install_windows_rocm_torch_stack(uv_bin, py_path, root, uv_python_install_dir)?;
    } else if is_xpu_profile(profile) {
        install_windows_xpu_torch_stack(uv_bin, py_path, root, uv_python_install_dir)?;
    } else {
        let (torch_v, tv_v, ta_v, index_url, triton_pkg) = torch_profile_to_packages(profile);
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &[
                "install",
                "--upgrade",
                "--force-reinstall",
                &format!("torch=={torch_v}"),
                &format!("torchvision=={tv_v}"),
                &format!("torchaudio=={ta_v}"),
                "--index-url",
                index_url,
                "--no-cache-dir",
                "--timeout=1000",
                "--retries",
                "10",
            ],
            Some(root),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &[
                "install",
                "--upgrade",
                "--force-reinstall",
                triton_pkg,
                "--no-cache-dir",
                "--timeout=1000",
                "--retries",
                "10",
            ],
            Some(root),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
    }
    let mut verify_cmd = std::process::Command::new(py_path);
    verify_cmd.arg("-c").arg(
        "import torch, importlib.metadata as m; \
         print(getattr(torch, '__version__', '')); \
         print(getattr(torch.version, 'cuda', '') or ''); \
         print(getattr(torch.version, 'hip', '') or ''); \
         print('xpu' if hasattr(torch, 'xpu') and torch.xpu.is_available() else ''); \
         print(m.version('torchvision')); \
         print(m.version('torchaudio'))",
    );
    verify_cmd.current_dir(root);
    apply_background_command_flags(&mut verify_cmd);
    apply_torch_allocator_env_compat(&mut verify_cmd);
    let verify = verify_cmd
        .output()
        .map_err(|err| format!("Failed to verify torch profile with {py_path}: {err}"))?;
    if !verify.status.success() {
        return Err("Torch profile verification command failed after reinstall.".to_string());
    }
    let text = String::from_utf8_lossy(&verify.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let installed_torch = lines.next().unwrap_or_default();
    let installed_cuda = lines.next().unwrap_or_default();
    let installed_hip = lines.next().unwrap_or_default();
    let installed_xpu = lines.next().unwrap_or_default();
    let installed_tv = lines.next().unwrap_or_default();
    let installed_ta = lines.next().unwrap_or_default();
    let actual_profile = torch_profile_from_versions(
        installed_torch,
        installed_cuda,
        installed_hip,
        installed_xpu,
    );
    if actual_profile.as_deref() != Some(profile) {
        return Err(format!(
            "Torch profile enforce mismatch for {profile}: got torch={installed_torch}, cuda={installed_cuda}, hip={installed_hip}, xpu={installed_xpu}, torchvision={installed_tv}, torchaudio={installed_ta}"
        ));
    }
    Ok(())
}

pub(crate) fn selected_attention_backend(request: &ComfyInstallRequest) -> Option<&'static str> {
    if request.include_flash_attention {
        Some("flash")
    } else if request.include_sage_attention || request.include_sage_attention3 {
        Some("sage")
    } else {
        None
    }
}

pub(crate) fn is_rocm_profile(profile: &str) -> bool {
    matches!(profile, "torch291_rocm72")
}

pub(crate) fn is_xpu_profile(profile: &str) -> bool {
    matches!(profile, "torchxpu_nightly")
}

pub(crate) fn is_non_cuda_profile(profile: &str) -> bool {
    is_rocm_profile(profile) || is_xpu_profile(profile)
}

fn windows_rocm_sdk_pytorch_packages() -> [&'static str; 4] {
    [
        "https://repo.radeon.com/rocm/windows/rocm-rel-7.2/torch-2.9.1%2Brocmsdk20260116-cp312-cp312-win_amd64.whl",
        "https://repo.radeon.com/rocm/windows/rocm-rel-7.2/torchvision-0.24.1%2Brocmsdk20260116-cp312-cp312-win_amd64.whl",
        "https://repo.radeon.com/rocm/windows/rocm-rel-7.2/torchaudio-2.9.1%2Brocmsdk20260116-cp312-cp312-win_amd64.whl",
        "",
    ]
}

fn windows_rocm_sdk_bootstrap_packages() -> [&'static str; 4] {
    [
        "https://repo.radeon.com/rocm/windows/rocm-rel-7.2/rocm_sdk_core-7.2.0.dev0-py3-none-win_amd64.whl",
        "https://repo.radeon.com/rocm/windows/rocm-rel-7.2/rocm_sdk_devel-7.2.0.dev0-py3-none-win_amd64.whl",
        "https://repo.radeon.com/rocm/windows/rocm-rel-7.2/rocm_sdk_libraries_custom-7.2.0.dev0-py3-none-win_amd64.whl",
        "https://repo.radeon.com/rocm/windows/rocm-rel-7.2/rocm-7.2.0.dev0.tar.gz",
    ]
}

pub(crate) fn install_windows_rocm_torch_stack(
    uv_bin: &str,
    py_path: &str,
    root: &Path,
    uv_python_install_dir: &str,
) -> Result<(), String> {
    let _ = uv_bin;
    let _ = uv_python_install_dir;
    let sdk = windows_rocm_sdk_bootstrap_packages();
    run_command_env(
        py_path,
        &[
            "-m",
            "pip",
            "install",
            "--no-cache-dir",
            sdk[0],
            sdk[1],
            sdk[2],
            sdk[3],
        ],
        Some(root),
        &[],
    )?;
    let pkgs = windows_rocm_sdk_pytorch_packages();
    run_command_env(
        py_path,
        &[
            "-m",
            "pip",
            "install",
            "--no-cache-dir",
            pkgs[0],
            pkgs[1],
            pkgs[2],
        ],
        Some(root),
        &[],
    )?;
    run_command_env(
        py_path,
        &["-m", "pip", "install", "--no-cache-dir", "numpy==1.26.4"],
        Some(root),
        &[],
    )
}

pub(crate) fn install_windows_xpu_torch_stack(
    uv_bin: &str,
    py_path: &str,
    root: &Path,
    uv_python_install_dir: &str,
) -> Result<(), String> {
    let py_exe = PathBuf::from(py_path);
    uv_pip_uninstall_best_effort(
        uv_bin,
        &py_exe,
        root,
        uv_python_install_dir,
        &[
            "torch",
            "torchvision",
            "torchaudio",
            "intel-extension-for-pytorch",
            "pytorch-triton-xpu",
        ],
    )?;
    run_uv_pip_strict(
        uv_bin,
        py_path,
        &[
            "install",
            "--pre",
            "--upgrade",
            "--force-reinstall",
            "--no-cache-dir",
            "torch",
            "torchvision",
            "torchaudio",
            "--index-url",
            "https://download.pytorch.org/whl/nightly/xpu",
        ],
        Some(root),
        &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
    )
}

pub(crate) fn apply_intel_xpu_launch_env(cmd: &mut std::process::Command, profile: &str) {
    if is_xpu_profile(profile) {
        cmd.env("UR_L0_ENABLE_RELAXED_ALLOCATION_LIMITS", "1");
        cmd.env("SYCL_CACHE_PERSISTENT", "1");
    }
}

fn detect_attention_backend_for_root(root: &Path) -> Option<String> {
    let has_flash = pip_has_package(root, "flash-attn")
        || pip_has_package(root, "flash_attn")
        || python_module_importable(root, "flash_attn");
    if has_flash {
        return Some("flash".to_string());
    }
    let has_sage3 =
        pip_has_package(root, "sageattn3") || python_module_importable(root, "sageattn3");
    if has_sage3 {
        return Some("sage3".to_string());
    }
    let has_sage =
        pip_has_package(root, "sageattention") || python_module_importable(root, "sageattention");
    if has_sage {
        return Some("sage".to_string());
    }
    if nunchaku_backend_present(root) {
        return Some("nunchaku".to_string());
    }
    None
}

pub(crate) fn detect_launch_attention_backend_for_root(root: &Path) -> Option<String> {
    detect_attention_backend_for_root(root)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn comfyui_launch_args(
    listen_enabled: bool,
    pinned_memory_enabled: bool,
    lowvram_enabled: bool,
    bf16_unet_enabled: bool,
    async_offload_enabled: bool,
    disable_smart_memory_enabled: bool,
    attention_backend: Option<&str>,
    custom_launch_args: &[String],
) -> Vec<String> {
    let mut args = vec!["--windows-standalone-build".to_string()];
    if listen_enabled {
        args.push("--listen".to_string());
    }
    if !pinned_memory_enabled {
        args.push("--disable-pinned-memory".to_string());
    }
    if lowvram_enabled {
        args.push("--lowvram".to_string());
    }
    if bf16_unet_enabled {
        args.push("--bf16-unet".to_string());
    }
    if async_offload_enabled {
        args.push("--async-offload".to_string());
    }
    if disable_smart_memory_enabled {
        args.push("--disable-smart-memory".to_string());
    }
    append_attention_launch_arg(&mut args, attention_backend);
    args.extend(custom_launch_args.iter().cloned());
    args
}

pub(crate) fn python_module_importable(root: &Path, module: &str) -> bool {
    let mut cmd = python_for_root(root);
    cmd.arg("-c").arg(format!(
        "import importlib.util, sys; sys.exit(0 if importlib.util.find_spec({module:?}) else 1)"
    ));
    cmd.current_dir(root);
    cmd.output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

pub(crate) fn python_module_import_error(root: &Path, module: &str) -> Option<String> {
    let mut cmd = python_for_root(root);
    cmd.arg("-c").arg(format!(
        "import importlib; importlib.import_module({module:?})"
    ));
    cmd.current_dir(root);
    let output = cmd.output().ok()?;
    if output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let mut message = String::new();
    if !stdout.is_empty() {
        message.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !message.is_empty() {
            message.push_str(" | ");
        }
        message.push_str(&stderr);
    }
    if message.is_empty() {
        Some(format!("Failed to import module: {module}"))
    } else {
        Some(message)
    }
}

pub(crate) fn nunchaku_backend_present(root: &Path) -> bool {
    python_module_importable(root, "nunchaku")
        || pip_has_package(root, "nunchaku")
        || custom_node_exists(root, "nunchaku_nodes")
        || custom_node_exists(root, "ComfyUI-nunchaku")
}
