use crate::shared::{
    amd_gpu_details_cache, append_attention_launch_arg, apply_torch_allocator_env_compat,
    choose_install_folder, clear_directory_contents, comfyui_runtime_running, custom_node_exists,
    custom_node_installed, default_true, detect_amd_gpu_name, detect_existing_comfyui_root,
    detect_gpu_details_cached, detect_intel_gpu_name, detect_nvidia_gpu,
    emit_comfyui_runtime_event, emit_comfyui_runtime_log_event, emit_install_event,
    fake_amd_enabled, fake_intel_enabled,
    git_current_branch, has_dns, install_custom_node_and_record, intel_gpu_details_cache,
    is_empty_dir, is_recoverable_preclone_dir, kill_managed_comfyui_child, nerdstats_enabled,
    normalize_optional_path, normalize_release_version, parse_custom_launch_args,
    parse_hf_env_value, parse_semver_triplet, parse_yaml_bool, parse_yaml_scalar,
    path_name_is_comfyui, push_preflight, read_comfyui_installed_version, recover_lock,
    remove_custom_node_dirs, resolve_comfyui_instance_name, show_main_window,
    spawn_progress_emitter, start_comfyui_root_background, stop_comfyui_for_mutation,
    wait_for_comfyui_start, write_install_state, write_install_summary, yaml_single_quote,
    AmdGpuDetails, AppState, ComfyExtraModelConfig, CustomNodeSpec, DownloadProgressEvent,
    InstallSummaryItem, IntelGpuDetails, PreflightItem,
};
use arctic_downloader::{
    app::{build_context, drain_lossy_lines, AppContext},
    config::AppSettings,
    env_flags::{auto_update_enabled, external_package_manager},
    ram::detect_ram_profile,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::BufRead,
    io::IsTerminal,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
#[cfg(feature = "desktop-tray")]
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Serialize)]
struct AppSnapshot {
    version: String,
    total_ram_gb: Option<f64>,
    ram_tier: Option<String>,
    nvidia_gpu_name: Option<String>,
    nvidia_gpu_vram_mb: Option<u64>,
    amd_gpu_name: Option<String>,
    intel_gpu_name: Option<String>,
    gpu_detection_pending: bool,
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
struct HfXetPreflightResponse {
    xet_enabled: bool,
    hf_cli_available: bool,
    hf_backend: String,
    hf_xet_installed: bool,
    hub_version: Option<String>,
    detail: String,
}

#[derive(Debug, Serialize)]
struct ComfyInstallRecommendation {
    gpu_name: Option<String>,
    driver_version: Option<String>,
    torch_profile: String,
    torch_label: String,
    reason: String,
    detection_pending: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComfyInstallRequest {
    install_root: String,
    #[serde(default)]
    extra_model_root: Option<String>,
    #[serde(default)]
    extra_model_use_default: bool,
    torch_profile: Option<String>,
    include_sage_attention: bool,
    include_sage_attention3: bool,
    include_flash_attention: bool,
    include_insight_face: bool,
    include_nunchaku: bool,
    #[serde(default)]
    include_trellis2: bool,
    #[serde(default = "default_true")]
    include_pinned_memory: bool,
    node_comfyui_manager: bool,
    node_comfyui_easy_use: bool,
    node_rgthree_comfy: bool,
    node_comfyui_gguf: bool,
    node_comfyui_kjnodes: bool,
    #[serde(default)]
    node_comfyui_crystools: bool,
    #[serde(default)]
    force_fresh: bool,
}

#[derive(Debug, Serialize)]
struct ComfyPreflightResponse {
    ok: bool,
    summary: String,
    items: Vec<PreflightItem>,
}

#[derive(Debug, Serialize)]
struct RocmGuidedStatus {
    distro_family: String,
    distro_label: String,
    supported: bool,
    amd_detected: bool,
    gpu_name: Option<String>,
    ready: bool,
    requires_relogin: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct XpuGuidedStatus {
    distro_family: String,
    distro_label: String,
    supported: bool,
    intel_detected: bool,
    gpu_name: Option<String>,
    ready: bool,
    requires_relogin: bool,
    detail: String,
}

#[derive(Clone, Debug)]
struct LinuxPrereqScan {
    distro: String,
    missing_required: Vec<String>,
    missing_optional: Vec<String>,
}

#[derive(Clone, Debug, Default)]
struct LinuxOsRelease {
    id: String,
    id_like: String,
    version_id: String,
    version_codename: String,
    ubuntu_codename: String,
    pretty_name: String,
}

#[derive(Debug, Serialize)]
struct ComfyPathInspection {
    selected: String,
    detected_root: Option<String>,
}

#[derive(Debug, Serialize)]
struct ComfyInstallationEntry {
    name: String,
    root: String,
}

const UV_PYTHON_VERSION: &str = "3.12.10";

#[tauri::command]
fn get_app_snapshot(state: State<'_, AppState>) -> AppSnapshot {
    let catalog = state.context.catalog.catalog_snapshot();
    let (mut nvidia_gpu_name, mut nvidia_gpu_vram_mb) = detect_nvidia_gpu();
    let mut amd_gpu_name = detect_amd_gpu_name();
    let mut intel_gpu_name = detect_intel_gpu_name();
    let gpu_detection_pending = gpu_detection_pending();
    // A fast lspci probe may finish between the calls above. Re-read completed
    // caches so one snapshot cannot preserve a stale AMD-only view.
    if !gpu_detection_pending {
        if nvidia_gpu_name.is_none() {
            (nvidia_gpu_name, nvidia_gpu_vram_mb) = detect_nvidia_gpu();
        }
        if amd_gpu_name.is_none() {
            amd_gpu_name = detect_amd_gpu_name();
        }
        if intel_gpu_name.is_none() {
            intel_gpu_name = detect_intel_gpu_name();
        }
    }
    let ram_profile = state.context.ram_profile.or_else(detect_ram_profile);
    AppSnapshot {
        version: state.context.display_version.clone(),
        total_ram_gb: ram_profile.map(|profile| profile.total_gb),
        ram_tier: ram_profile.map(|profile| profile.tier.label().to_string()),
        nvidia_gpu_name,
        nvidia_gpu_vram_mb,
        amd_gpu_name,
        intel_gpu_name,
        gpu_detection_pending,
        model_count: catalog.models.len(),
        lora_count: catalog.loras.len(),
    }
}

// Not moved to `shared.rs`: Windows' `NvidiaGpuDetails` lacks
// `compute_capability` (used here for Hopper/SM90 detection), so the two
// platforms' structs are genuinely different, not just duplicated text.
#[derive(Clone, Debug, Default)]
pub(crate) struct NvidiaGpuDetails {
    pub(crate) name: Option<String>,
    pub(crate) vram_mb: Option<u64>,
    driver_version: Option<String>,
    compute_capability: Option<String>,
}

static GPU_DETAILS_CACHE: OnceLock<Mutex<Option<NvidiaGpuDetails>>> = OnceLock::new();
static GPU_DETAILS_PROBE_STARTED: AtomicBool = AtomicBool::new(false);
static AMD_GPU_DETAILS_PROBE_STARTED: AtomicBool = AtomicBool::new(false);
static INTEL_GPU_DETAILS_PROBE_STARTED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "desktop-tray")]
static TRAY_MENU_ITEMS: OnceLock<Mutex<Option<TrayMenuItems>>> = OnceLock::new();
static LINUX_PREREQ_CACHE: OnceLock<Mutex<Option<LinuxPrereqScan>>> = OnceLock::new();

#[cfg(feature = "desktop-tray")]
struct TrayMenuItems {
    start: MenuItem<tauri::Wry>,
    stop: MenuItem<tauri::Wry>,
}

#[cfg(feature = "desktop-tray")]
fn tray_menu_items() -> &'static Mutex<Option<TrayMenuItems>> {
    TRAY_MENU_ITEMS.get_or_init(|| Mutex::new(None))
}

fn gpu_details_cache() -> &'static Mutex<Option<NvidiaGpuDetails>> {
    GPU_DETAILS_CACHE.get_or_init(|| Mutex::new(None))
}

fn linux_prereq_cache() -> &'static Mutex<Option<LinuxPrereqScan>> {
    LINUX_PREREQ_CACHE.get_or_init(|| Mutex::new(None))
}

fn detect_linux_distro_family() -> String {
    let os_release = detect_linux_os_release();
    let id = os_release.id;
    let id_like = os_release.id_like;
    let haystack = format!("{id} {id_like}");
    if haystack.contains("nixos") {
        "nixos".to_string()
    } else if haystack.contains("arch") {
        "arch".to_string()
    } else if haystack.contains("debian") || haystack.contains("ubuntu") {
        "debian".to_string()
    } else if haystack.contains("fedora")
        || haystack.contains("rhel")
        || haystack.contains("centos")
    {
        "fedora".to_string()
    } else {
        "unknown".to_string()
    }
}

fn detect_linux_os_release() -> LinuxOsRelease {
    #[cfg(not(target_os = "linux"))]
    {
        LinuxOsRelease::default()
    }

    #[cfg(target_os = "linux")]
    {
        let os_release = if running_in_flatpak() {
            run_command_capture("cat", &["/etc/os-release"], None)
                .map(|(stdout, _)| stdout)
                .unwrap_or_default()
        } else {
            std::fs::read_to_string("/etc/os-release").unwrap_or_default()
        };
        let mut info = LinuxOsRelease::default();
        for line in os_release.lines() {
            if let Some(value) = line.strip_prefix("ID=") {
                info.id = value.trim_matches('"').to_ascii_lowercase();
            } else if let Some(value) = line.strip_prefix("ID_LIKE=") {
                info.id_like = value.trim_matches('"').to_ascii_lowercase();
            } else if let Some(value) = line.strip_prefix("VERSION_ID=") {
                info.version_id = value.trim_matches('"').to_ascii_lowercase();
            } else if let Some(value) = line.strip_prefix("VERSION_CODENAME=") {
                info.version_codename = value.trim_matches('"').to_ascii_lowercase();
            } else if let Some(value) = line.strip_prefix("UBUNTU_CODENAME=") {
                info.ubuntu_codename = value.trim_matches('"').to_ascii_lowercase();
            } else if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
                info.pretty_name = value.trim_matches('"').to_string();
            }
        }
        info
    }
}

fn linux_package_sets(distro: &str) -> (Vec<&'static str>, Vec<&'static str>) {
    match distro {
        "nixos" => (
            vec![
                "git", "curl", "wget", "python3", "gcc", "make", "cmake", "ninja",
            ],
            Vec::new(),
        ),
        "arch" => (
            vec![
                "git",
                "curl",
                "wget",
                "python",
                "base-devel",
                "cmake",
                "ninja",
            ],
            vec!["libglvnd", "mesa"],
        ),
        "debian" => (
            vec![
                "git",
                "curl",
                "wget",
                "python3",
                "build-essential",
                "cmake",
                "ninja-build",
            ],
            vec!["libgl1"],
        ),
        "fedora" => (
            vec![
                "git",
                "curl",
                "wget",
                "python3",
                "gcc",
                "gcc-c++",
                "make",
                "cmake",
                "ninja-build",
            ],
            vec!["mesa-libGL"],
        ),
        _ => (vec!["git", "curl", "wget", "python3"], Vec::new()),
    }
}

fn linux_package_installed(distro: &str, package: &str) -> bool {
    if package == "wget" && command_available("wget", &["--version"]) {
        return true;
    }
    let probe = match distro {
        "nixos" => {
            let args: &[&str] = match package {
                "git" => &["--version"],
                "curl" => &["--version"],
                "wget" => &["--version"],
                "python3" => &["--version"],
                "gcc" => &["--version"],
                "make" => &["--version"],
                "cmake" => &["--version"],
                "ninja" => &["--version"],
                _ => return false,
            };
            return command_available(package, args);
        }
        "arch" => run_command_capture("pacman", &["-Q", package], None),
        "debian" => run_command_capture("dpkg", &["-s", package], None),
        "fedora" => run_command_capture("rpm", &["-q", package], None),
        _ => {
            // Distro not recognized (openSUSE, Alpine, Void, Gentoo, ...):
            // we don't know that distro's package-query tool or package
            // names, but `linux_package_sets`'s wildcard arm only ever asks
            // about bare command names (git/curl/wget/python3) for this
            // branch, so check PATH directly instead of assuming everything
            // is already installed.
            return command_available(package, &["--version"]);
        }
    };
    probe.is_ok()
}

fn scan_linux_prereqs() -> Result<LinuxPrereqScan, String> {
    let distro = detect_linux_distro_family();
    let (required, optional) = linux_package_sets(&distro);
    let missing_required = required
        .into_iter()
        .filter(|pkg| !linux_package_installed(&distro, pkg))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let missing_optional = optional
        .into_iter()
        .filter(|pkg| !linux_package_installed(&distro, pkg))
        .map(str::to_string)
        .collect::<Vec<_>>();
    Ok(LinuxPrereqScan {
        distro,
        missing_required,
        missing_optional,
    })
}

fn get_linux_prereq_cache_or_scan() -> Result<LinuxPrereqScan, String> {
    if let Ok(cache) = linux_prereq_cache().lock() {
        if let Some(cached) = cache.clone() {
            return Ok(cached);
        }
    }
    refresh_linux_prereq_cache()
}

fn refresh_linux_prereq_cache() -> Result<LinuxPrereqScan, String> {
    let scan = scan_linux_prereqs()?;
    if let Ok(mut cache) = linux_prereq_cache().lock() {
        *cache = Some(scan.clone());
    }
    Ok(scan)
}

fn warm_linux_prereq_cache_background() {
    std::thread::spawn(|| {
        let _ = refresh_linux_prereq_cache();
    });
}

fn install_missing_linux_prereqs(scan: &LinuxPrereqScan) -> Result<(), String> {
    if scan.missing_required.is_empty() {
        return Ok(());
    }
    let mut package_args: Vec<&str> = scan.missing_required.iter().map(String::as_str).collect();
    match scan.distro.as_str() {
        "arch" => {
            run_privileged_command("pacman", &["-Sy"], None)?;
            let mut args = vec!["-S", "--needed", "--noconfirm"];
            args.append(&mut package_args);
            run_privileged_command("pacman", &args, None)?;
        }
        "debian" => {
            run_privileged_command("apt", &["update"], None)?;
            let mut args = vec!["install", "-y"];
            args.append(&mut package_args);
            run_privileged_command("apt", &args, None)?;
        }
        "fedora" => {
            run_privileged_command("dnf", &["makecache"], None)?;
            let mut args = vec!["install", "-y"];
            args.append(&mut package_args);
            run_privileged_command("dnf", &args, None)?;
        }
        _ => {
            if scan.distro == "nixos" {
                return Err(
                    "This NixOS package supplies the required tools through its wrapper. Reinstall or update the Arctic Helper Nix package instead of installing system packages imperatively."
                        .to_string(),
                );
            }
            return Err(
                "Unsupported Linux distribution for automatic package install. Install required packages manually."
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn query_nvidia_gpu_details_blocking() -> NvidiaGpuDetails {
    let detailed = run_command_capture(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total,driver_version,compute_cap",
            "--format=csv,noheader,nounits",
        ],
        None,
    );
    let (stdout, has_compute_capability) = match detailed {
        Ok((stdout, _)) => (stdout, true),
        Err(_) => match run_command_capture(
            "nvidia-smi",
            &[
                "--query-gpu=name,memory.total,driver_version",
                "--format=csv,noheader,nounits",
            ],
            None,
        ) {
            Ok((stdout, _)) => (stdout, false),
            Err(_) => {
                return NvidiaGpuDetails {
                    name: query_nvidia_gpu_name_from_lspci(),
                    ..NvidiaGpuDetails::default()
                };
            }
        },
    };
    let first = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    if first.is_empty() {
        return NvidiaGpuDetails {
            name: query_nvidia_gpu_name_from_lspci(),
            ..NvidiaGpuDetails::default()
        };
    }

    let mut parts = first.split(',').map(str::trim);
    let name = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let vram_mb = parts.next().and_then(|value| value.parse::<u64>().ok());
    let driver_version = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let compute_capability = has_compute_capability
        .then(|| parts.next())
        .flatten()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    NvidiaGpuDetails {
        name,
        vram_mb,
        driver_version,
        compute_capability,
    }
}

fn query_nvidia_gpu_name_from_lspci() -> Option<String> {
    #[cfg(not(target_os = "linux"))]
    {
        None
    }

    #[cfg(target_os = "linux")]
    {
        let (stdout, _) = run_command_capture("lspci", &["-nn"], None).ok()?;
        find_nvidia_gpu_name_in_lspci(&stdout)
    }
}

fn find_nvidia_gpu_name_in_lspci(stdout: &str) -> Option<String> {
    stdout.lines().map(str::trim).find_map(|line| {
        let lower = line.to_ascii_lowercase();
        let is_display_controller = lower.contains("vga compatible controller")
            || lower.contains("3d controller")
            || lower.contains("display controller");
        let is_nvidia =
            lower.contains("nvidia") || lower.contains("[10de:") || lower.contains(" 10de:");
        if !is_display_controller || !is_nvidia {
            return None;
        }
        line.split_once(": ")
            .map(|(_, name)| name.trim().to_string())
            .filter(|name| !name.is_empty())
    })
}

fn query_amd_gpu_details_blocking() -> AmdGpuDetails {
    #[cfg(not(target_os = "linux"))]
    {
        return AmdGpuDetails::default();
    }

    #[cfg(target_os = "linux")]
    {
        let (stdout, _) = match run_command_capture("lspci", &["-nn"], None) {
            Ok(out) => out,
            Err(_) => return AmdGpuDetails::default(),
        };
        let line = stdout
            .lines()
            .map(str::trim)
            .find(|line| {
                let lower = line.to_ascii_lowercase();
                (lower.contains("vga compatible controller")
                    || lower.contains("3d controller")
                    || lower.contains("display controller"))
                    && (lower.contains("advanced micro devices")
                        || lower.contains("amd/ati")
                        || lower.contains("radeon")
                        || lower.contains("amdgpu"))
            })
            .unwrap_or_default();
        if line.is_empty() {
            return AmdGpuDetails::default();
        }
        let name = line
            .split(": ")
            .nth(1)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        AmdGpuDetails { name }
    }
}

fn query_intel_gpu_details_blocking() -> IntelGpuDetails {
    #[cfg(not(target_os = "linux"))]
    {
        return IntelGpuDetails::default();
    }

    #[cfg(target_os = "linux")]
    {
        let (stdout, _) = match run_command_capture("lspci", &["-nn"], None) {
            Ok(out) => out,
            Err(_) => return IntelGpuDetails::default(),
        };
        let line = stdout
            .lines()
            .map(str::trim)
            .find(|line| {
                let lower = line.to_ascii_lowercase();
                (lower.contains("vga compatible controller")
                    || lower.contains("3d controller")
                    || lower.contains("display controller"))
                    && (lower.contains("intel corporation")
                        || lower.contains("intel arc")
                        || lower.contains("iris xe")
                        || lower.contains("uhd graphics"))
            })
            .unwrap_or_default();
        if line.is_empty() {
            return IntelGpuDetails::default();
        }
        let name = line
            .split(": ")
            .nth(1)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        IntelGpuDetails { name }
    }
}

fn is_nvidia_hopper_sm90() -> bool {
    let gpu = detect_nvidia_gpu_details();
    if gpu
        .compute_capability
        .as_deref()
        .map(str::trim)
        .map(|cc| cc == "9.0")
        .unwrap_or(false)
    {
        return true;
    }

    gpu.name
        .as_deref()
        .map(|name| {
            let n = name.to_ascii_lowercase();
            n.contains("h100") || n.contains("h200") || n.contains("gh200") || n.contains("hopper")
        })
        .unwrap_or(false)
}

pub(crate) fn detect_nvidia_gpu_details() -> NvidiaGpuDetails {
    detect_gpu_details_cached(
        gpu_details_cache(),
        &GPU_DETAILS_PROBE_STARTED,
        |d| d.name.is_some() || d.vram_mb.is_some() || d.driver_version.is_some(),
        query_nvidia_gpu_details_blocking,
    )
}

pub(crate) fn detect_amd_gpu_details() -> AmdGpuDetails {
    detect_gpu_details_cached(
        amd_gpu_details_cache(),
        &AMD_GPU_DETAILS_PROBE_STARTED,
        |d| d.name.is_some(),
        query_amd_gpu_details_blocking,
    )
}

pub(crate) fn detect_intel_gpu_details() -> IntelGpuDetails {
    detect_gpu_details_cached(
        intel_gpu_details_cache(),
        &INTEL_GPU_DETAILS_PROBE_STARTED,
        |d| d.name.is_some(),
        query_intel_gpu_details_blocking,
    )
}

fn gpu_detection_pending() -> bool {
    GPU_DETAILS_PROBE_STARTED.load(Ordering::SeqCst)
        || AMD_GPU_DETAILS_PROBE_STARTED.load(Ordering::SeqCst)
        || INTEL_GPU_DETAILS_PROBE_STARTED.load(Ordering::SeqCst)
}

fn fake_amd_allow_rocm_setup_enabled() -> bool {
    std::env::var("ARCTIC_FAKE_AMD_ALLOW_ROCM_SETUP")
        .map(|value| value == "1")
        .unwrap_or(false)
}

fn fake_intel_allow_xpu_setup_enabled() -> bool {
    std::env::var("ARCTIC_FAKE_INTEL_ALLOW_XPU_SETUP")
        .map(|value| value == "1")
        .unwrap_or(false)
}

fn user_in_group(group: &str) -> bool {
    let stdout = run_command_capture("id", &["-nG"], None)
        .ok()
        .map(|(stdout, _)| stdout)
        .or_else(|| {
            let user = std::env::var("USER").unwrap_or_default();
            if user.is_empty() {
                None
            } else {
                run_command_capture("id", &["-nG", &user], None)
                    .ok()
                    .map(|(stdout, _)| stdout)
            }
        })
        .unwrap_or_default();
    stdout
        .split_whitespace()
        .any(|value| value.trim().eq_ignore_ascii_case(group))
}

fn rocminfo_command() -> Option<String> {
    if command_available("rocminfo", &["--help"]) {
        return Some("rocminfo".to_string());
    }
    let alt = "/opt/rocm/bin/rocminfo";
    if Path::new(alt).exists() {
        return Some(alt.to_string());
    }
    None
}

fn rocm_runtime_ready() -> (bool, bool, Vec<String>) {
    let mut notes: Vec<String> = Vec::new();
    let rocminfo_cmd = rocminfo_command();
    let has_rocminfo_bin = rocminfo_cmd.is_some();
    if !has_rocminfo_bin {
        notes.push("`rocminfo` is not installed.".to_string());
    }

    let has_dev_kfd = Path::new("/dev/kfd").exists();
    if !has_dev_kfd {
        notes.push("/dev/kfd is missing.".to_string());
    }

    let render_ok = user_in_group("render");
    let video_ok = user_in_group("video");
    if !render_ok || !video_ok {
        notes.push("Current user is not yet in both `render` and `video` groups.".to_string());
    }

    let rocminfo_ok = if let Some(cmd) = rocminfo_cmd.as_deref() {
        run_command_capture(cmd, &[], None)
            .map(|(stdout, _)| stdout.to_ascii_lowercase().contains("agent"))
            .unwrap_or(false)
    } else {
        false
    };
    if has_rocminfo_bin && !rocminfo_ok {
        notes.push("`rocminfo` did not report a usable ROCm agent.".to_string());
    }

    let runtime_partially_present = has_rocminfo_bin || has_dev_kfd;
    let requires_relogin = runtime_partially_present && (!render_ok || !video_ok);

    (
        has_rocminfo_bin && has_dev_kfd && rocminfo_ok,
        requires_relogin,
        notes,
    )
}

fn dri_render_nodes() -> Vec<PathBuf> {
    let mut nodes = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev/dri") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .file_name()
                .map(|name| name.to_string_lossy().starts_with("renderD"))
                .unwrap_or(false)
            {
                nodes.push(path);
            }
        }
    }
    nodes
}

fn path_has_rw_access(path: &Path) -> bool {
    let shell_check = format!(
        "test -r {} && test -w {}",
        shell_single_quote(&path.to_string_lossy()),
        shell_single_quote(&path.to_string_lossy())
    );
    run_command_capture("sh", &["-c", &shell_check], None).is_ok()
}

fn linux_package_installed_any(distro: &str, packages: &[&str]) -> bool {
    packages
        .iter()
        .any(|package| linux_package_installed(distro, package))
}

fn xpu_runtime_ready_for_distro(distro: &str) -> (bool, bool, Vec<String>) {
    let mut notes: Vec<String> = Vec::new();
    let runtime_packages_ready = match distro {
        "arch" => {
            let compute = linux_package_installed("arch", "intel-compute-runtime");
            let level_zero =
                linux_package_installed_any("arch", &["level-zero-loader", "level-zero"]);
            if !compute {
                notes.push("`intel-compute-runtime` is not installed.".to_string());
            }
            if !level_zero {
                notes.push("Level Zero runtime is not installed.".to_string());
            }
            compute && level_zero
        }
        "fedora" => {
            let compute = linux_package_installed("fedora", "intel-compute-runtime");
            let level_zero =
                linux_package_installed_any("fedora", &["level-zero", "level-zero-loader"]);
            if !compute {
                notes.push("`intel-compute-runtime` is not installed.".to_string());
            }
            if !level_zero {
                notes.push("Level Zero runtime is not installed.".to_string());
            }
            compute && level_zero
        }
        "debian" => {
            let opencl = linux_package_installed("debian", "intel-opencl-icd");
            let level_zero = linux_package_installed_any(
                "debian",
                &["intel-level-zero-gpu", "libze-intel-gpu1", "level-zero"],
            );
            if !opencl {
                notes.push("`intel-opencl-icd` is not installed.".to_string());
            }
            if !level_zero {
                notes.push("Intel Level Zero runtime is not installed.".to_string());
            }
            opencl && level_zero
        }
        _ => false,
    };

    let render_nodes = dri_render_nodes();
    let has_render_node = !render_nodes.is_empty();
    if !has_render_node {
        notes.push("No `/dev/dri/renderD*` device was found.".to_string());
    }

    let render_ok = user_in_group("render");
    let video_ok = user_in_group("video");
    let has_render_access = render_nodes.iter().any(|path| path_has_rw_access(path));
    if !has_render_access {
        if !render_ok {
            notes.push("Current user does not yet have access to `/dev/dri/renderD*`. Log out and back in if group changes were just applied.".to_string());
        } else {
            notes.push("Current user is in the `render` group but still cannot access `/dev/dri/renderD*`.".to_string());
        }
    } else if !video_ok {
        notes.push("Current user is not in the `video` group. This is optional for Intel XPU checks, but some media features may still expect it.".to_string());
    }

    let requires_relogin = runtime_packages_ready && has_render_node && !has_render_access;
    (
        runtime_packages_ready && has_render_node && has_render_access,
        requires_relogin,
        notes,
    )
}

fn rocm_supported_for_distro(os: &LinuxOsRelease, family: &str) -> bool {
    match family {
        "arch" | "fedora" => true,
        "debian" => {
            let code = if !os.ubuntu_codename.is_empty() {
                os.ubuntu_codename.as_str()
            } else {
                os.version_codename.as_str()
            };
            matches!(code, "jammy" | "noble")
                || matches!(os.version_id.as_str(), "12" | "13" | "22.04" | "24.04")
        }
        _ => false,
    }
}

fn xpu_supported_for_distro(_os: &LinuxOsRelease, family: &str) -> bool {
    matches!(family, "arch" | "fedora" | "debian")
}

/// Emits a `comfyui-install-progress` event for the ROCm/XPU guided-setup
/// flows. (Both used to be separate, byte-for-byte identical functions.)
fn emit_guided_setup_event(app: &AppHandle, phase: &str, message: &str) {
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

fn stream_command_output(
    app: &AppHandle,
    phase: &'static str,
    stream_name: &'static str,
    reader: impl std::io::Read + Send + 'static,
    tail: std::sync::Arc<Mutex<VecDeque<String>>>,
) -> std::thread::JoinHandle<()> {
    let app = app.clone();
    std::thread::spawn(move || {
        let buffered = std::io::BufReader::new(reader);
        for line in buffered.lines().map_while(Result::ok) {
            let text = line.trim_end().to_string();
            if text.is_empty() {
                continue;
            }
            if let Ok(mut lines) = tail.lock() {
                if lines.len() >= 12 {
                    lines.pop_front();
                }
                lines.push_back(format!("[{stream_name}] {text}"));
            }
            emit_guided_setup_event(&app, phase, &text);
        }
    })
}

fn run_command_streaming_with_env(
    app: &AppHandle,
    phase: &'static str,
    program: &str,
    args: &[&str],
    working_dir: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<(), String> {
    log::debug!(
        "run_command_streaming_with_env: {} {}",
        program,
        args.join(" ")
    );
    let mut cmd = build_command(program, args, working_dir, envs)?;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|err| format!("Failed to run {program}: {err}"))?;

    let tail = std::sync::Arc::new(Mutex::new(VecDeque::<String>::new()));
    let stdout_handle = child
        .stdout
        .take()
        .map(|stdout| stream_command_output(app, phase, "stdout", stdout, tail.clone()));
    let stderr_handle = child
        .stderr
        .take()
        .map(|stderr| stream_command_output(app, phase, "stderr", stderr, tail.clone()));

    let status = child
        .wait()
        .map_err(|err| format!("Failed to wait for {program}: {err}"))?;

    if let Some(handle) = stdout_handle {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }

    if !status.success() {
        let detail = tail
            .lock()
            .ok()
            .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join(" | "))
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| "no command output captured".to_string());
        return Err(format!(
            "Command failed: {} {} :: {}",
            program,
            args.join(" "),
            detail
        ));
    }
    Ok(())
}

fn install_rocm_guided_internal(app: &AppHandle) -> Result<RocmGuidedStatus, String> {
    let os = detect_linux_os_release();
    let family = detect_linux_distro_family();
    let gpu_name = detect_amd_gpu_name();
    let supported = rocm_supported_for_distro(&os, &family);
    let distro_label = if os.pretty_name.trim().is_empty() {
        family.clone()
    } else {
        os.pretty_name.clone()
    };

    if gpu_name.is_none() {
        return Ok(RocmGuidedStatus {
            distro_family: family,
            distro_label,
            supported,
            amd_detected: false,
            gpu_name: None,
            ready: false,
            requires_relogin: false,
            detail: "AMD GPU not detected on this system.".to_string(),
        });
    }

    if fake_amd_enabled() && !fake_amd_allow_rocm_setup_enabled() {
        emit_guided_setup_event(
            app,
            "warn",
            "Fake AMD mode enabled. Guided ROCm setup is disabled to avoid modifying a non-AMD system.",
        );
        return Ok(RocmGuidedStatus {
            distro_family: family,
            distro_label,
            supported: false,
            amd_detected: true,
            gpu_name,
            ready: false,
            requires_relogin: false,
            detail:
                "Fake AMD mode is active. UI testing is enabled, but guided ROCm setup is disabled."
                    .to_string(),
        });
    }

    if !supported {
        return Ok(RocmGuidedStatus {
            distro_family: family,
            distro_label,
            supported: false,
            amd_detected: true,
            gpu_name,
            ready: false,
            requires_relogin: false,
            detail: "Guided ROCm setup is currently supported only for Debian-based, Fedora, and Arch Linux families.".to_string(),
        });
    }

    match family.as_str() {
        "arch" => {
            let mut steps = vec![
                "echo Refreshing pacman package metadata for ROCm setup...".to_string(),
                "pacman -Sy".to_string(),
                "echo Installing ROCm packages with pacman...".to_string(),
                "pacman -S --needed --noconfirm rocminfo rocm-hip-sdk rocm-opencl-sdk".to_string(),
            ];
            if let Ok(user) = std::env::var("USER") {
                if !user.trim().is_empty() {
                    steps.extend(rocm_group_setup_steps(&user));
                }
            }
            run_privileged_shell_streaming(app, &steps.join(" && "), None)?;
        }
        "fedora" => {
            let mut steps = vec![
                "echo Refreshing dnf metadata for ROCm setup...".to_string(),
                "dnf makecache".to_string(),
                "echo Installing ROCm packages with dnf...".to_string(),
                "dnf install -y rocminfo rocm-hip rocm-opencl".to_string(),
            ];
            if let Ok(user) = std::env::var("USER") {
                if !user.trim().is_empty() {
                    steps.extend(rocm_group_setup_steps(&user));
                }
            }
            run_privileged_shell_streaming(app, &steps.join(" && "), None)?;
        }
        "debian" => {
            let ubuntu_code = if !os.ubuntu_codename.is_empty() {
                os.ubuntu_codename.clone()
            } else if matches!(os.version_id.as_str(), "12") {
                "jammy".to_string()
            } else if matches!(os.version_id.as_str(), "13") {
                "noble".to_string()
            } else {
                os.version_codename.clone()
            };
            if !matches!(ubuntu_code.as_str(), "jammy" | "noble") {
                return Err(format!(
                    "Guided ROCm setup currently supports Ubuntu-compatible codenames jammy/noble for Debian-family systems. Detected '{}'.",
                    ubuntu_code
                ));
            }
            let installer_url = format!(
                "https://repo.radeon.com/amdgpu-install/7.2.3/ubuntu/{ubuntu_code}/amdgpu-install_7.2.3.70203-1_all.deb"
            );
            let deb_path = "/tmp/amdgpu-install_7.2.3.70203-1_all.deb";
            emit_guided_setup_event(app, "step", "Downloading AMD amdgpu-install package...");
            run_command_with_retry("wget", &["-O", deb_path, &installer_url], None, 2)?;
            let mut steps = vec![
                "echo Refreshing apt package metadata for ROCm setup...".to_string(),
                "apt update".to_string(),
                "echo Installing amdgpu-install package...".to_string(),
                format!("apt install -y {}", shell_single_quote(deb_path)),
                "echo Running AMD ROCm guided installer...".to_string(),
                "amdgpu-install -y --usecase=rocm --no-dkms".to_string(),
            ];
            if let Ok(user) = std::env::var("USER") {
                if !user.trim().is_empty() {
                    steps.extend(rocm_group_setup_steps(&user));
                }
            }
            run_privileged_shell_streaming(app, &steps.join(" && "), None)?;
        }
        _ => {
            return Err("Unsupported distro family for guided ROCm setup.".to_string());
        }
    }

    if fake_amd_enabled() && fake_amd_allow_rocm_setup_enabled() {
        emit_guided_setup_event(
            app,
            "step",
            "Fake AMD install-test mode is active. Runtime validation is skipped because no real AMD GPU is present. If guided setup changed your groups, log out and back in before real use.",
        );
        return Ok(RocmGuidedStatus {
            distro_family: family,
            distro_label,
            supported,
            amd_detected: true,
            gpu_name,
            ready: true,
            requires_relogin: false,
            detail: "ROCm package installation finished. Runtime validation is skipped in fake AMD install-test mode. If guided setup changed your groups, log out and back in before real use.".to_string(),
        });
    }

    emit_guided_setup_event(app, "step", "Checking ROCm runtime readiness...");
    let (ready, requires_relogin, notes) = rocm_runtime_ready();
    let detail = if ready {
        "ROCm runtime looks ready for use. If guided setup changed your groups, log out and back in before launching ComfyUI.".to_string()
    } else if requires_relogin {
        "ROCm packages installed. Log out and back in, or reboot, then run the ROCm check again."
            .to_string()
    } else if notes.is_empty() {
        "ROCm guided setup finished. Log out and back in, then run the ROCm check again."
            .to_string()
    } else {
        format!(
            "ROCm guided setup finished. Log out and back in, then run the ROCm check again. {}",
            notes.join(" ")
        )
    };

    Ok(RocmGuidedStatus {
        distro_family: family,
        distro_label,
        supported,
        amd_detected: true,
        gpu_name,
        ready,
        requires_relogin,
        detail,
    })
}

fn install_xpu_guided_internal(app: &AppHandle) -> Result<XpuGuidedStatus, String> {
    let os = detect_linux_os_release();
    let family = detect_linux_distro_family();
    let gpu_name = detect_intel_gpu_name();
    let supported = xpu_supported_for_distro(&os, &family);
    let distro_label = if os.pretty_name.trim().is_empty() {
        family.clone()
    } else {
        os.pretty_name.clone()
    };

    if gpu_name.is_none() {
        return Ok(XpuGuidedStatus {
            distro_family: family,
            distro_label,
            supported,
            intel_detected: false,
            gpu_name: None,
            ready: false,
            requires_relogin: false,
            detail: "Intel GPU not detected on this system.".to_string(),
        });
    }

    if fake_intel_enabled() && !fake_intel_allow_xpu_setup_enabled() {
        emit_guided_setup_event(
            app,
            "warn",
            "Fake Intel mode enabled. Guided Intel setup is disabled to avoid modifying a non-Intel system.",
        );
        return Ok(XpuGuidedStatus {
            distro_family: family,
            distro_label,
            supported: false,
            intel_detected: true,
            gpu_name,
            ready: false,
            requires_relogin: false,
            detail: "Fake Intel mode is active. UI testing is enabled, but guided Intel setup is disabled.".to_string(),
        });
    }

    if !supported {
        return Ok(XpuGuidedStatus {
            distro_family: family,
            distro_label,
            supported: false,
            intel_detected: true,
            gpu_name,
            ready: false,
            requires_relogin: false,
            detail: "Guided Intel setup is currently supported only for Debian-based, Fedora, and Arch Linux families.".to_string(),
        });
    }

    match family.as_str() {
        "arch" => {
            let mut steps = vec![
                "echo Refreshing pacman package metadata for Intel XPU setup...".to_string(),
                "pacman -Sy".to_string(),
                "echo Installing Intel XPU runtime packages with pacman...".to_string(),
                "pacman -S --needed --noconfirm intel-compute-runtime level-zero-loader"
                    .to_string(),
            ];
            if let Ok(user) = std::env::var("USER") {
                if !user.trim().is_empty() {
                    steps.extend(rocm_group_setup_steps(&user));
                }
            }
            run_privileged_shell_streaming(app, &steps.join(" && "), None)?;
        }
        "fedora" => {
            let mut steps = vec![
                "echo Refreshing dnf metadata for Intel XPU setup...".to_string(),
                "dnf makecache".to_string(),
                "echo Installing Intel XPU runtime packages with dnf...".to_string(),
                "(dnf install -y intel-compute-runtime level-zero || dnf install -y intel-compute-runtime level-zero-loader)".to_string(),
            ];
            if let Ok(user) = std::env::var("USER") {
                if !user.trim().is_empty() {
                    steps.extend(rocm_group_setup_steps(&user));
                }
            }
            run_privileged_shell_streaming(app, &steps.join(" && "), None)?;
        }
        "debian" => {
            let mut steps = vec![
                "echo Refreshing apt package metadata for Intel XPU setup...".to_string(),
                "apt update".to_string(),
                "echo Installing Intel XPU runtime packages with apt...".to_string(),
                "apt install -y intel-opencl-icd".to_string(),
                "if apt-cache show intel-level-zero-gpu >/dev/null 2>&1; then apt install -y intel-level-zero-gpu; elif apt-cache show libze-intel-gpu1 >/dev/null 2>&1; then apt install -y libze-intel-gpu1; elif apt-cache show level-zero >/dev/null 2>&1; then apt install -y level-zero; else echo Warning: no supported Intel Level Zero package was found in apt repositories.; fi".to_string(),
            ];
            if let Ok(user) = std::env::var("USER") {
                if !user.trim().is_empty() {
                    steps.extend(rocm_group_setup_steps(&user));
                }
            }
            run_privileged_shell_streaming(app, &steps.join(" && "), None)?;
        }
        _ => return Err("Unsupported distro family for guided Intel setup.".to_string()),
    }

    if fake_intel_enabled() && fake_intel_allow_xpu_setup_enabled() {
        emit_guided_setup_event(
            app,
            "step",
            "Fake Intel install-test mode is active. Runtime validation is skipped because no real Intel GPU is present. If guided setup changed your groups, log out and back in before real use.",
        );
        return Ok(XpuGuidedStatus {
            distro_family: family,
            distro_label,
            supported,
            intel_detected: true,
            gpu_name,
            ready: true,
            requires_relogin: false,
            detail: "Intel package installation finished. Runtime validation is skipped in fake Intel install-test mode. If guided setup changed your groups, log out and back in before real use.".to_string(),
        });
    }

    emit_guided_setup_event(app, "step", "Checking Intel XPU runtime readiness...");
    let (ready, requires_relogin, notes) = xpu_runtime_ready_for_distro(&family);
    let detail = if ready {
        "Intel XPU runtime looks ready for use. If guided setup changed your groups, log out and back in before launching ComfyUI.".to_string()
    } else if requires_relogin {
        "Intel GPU packages installed. Log out and back in, or reboot, then run the Intel XPU check again.".to_string()
    } else if notes.is_empty() {
        "Intel guided setup finished. Log out and back in, then run the Intel XPU check again."
            .to_string()
    } else {
        format!(
            "Intel guided setup finished. Log out and back in, then run the Intel XPU check again. {}",
            notes.join(" ")
        )
    };

    Ok(XpuGuidedStatus {
        distro_family: family,
        distro_label,
        supported,
        intel_detected: true,
        gpu_name,
        ready,
        requires_relogin,
        detail,
    })
}

#[tauri::command]
fn get_rocm_guided_status() -> RocmGuidedStatus {
    let os = detect_linux_os_release();
    let family = detect_linux_distro_family();
    let gpu_name = detect_amd_gpu_name();
    let supported = rocm_supported_for_distro(&os, &family);
    let distro_label = if os.pretty_name.trim().is_empty() {
        family.clone()
    } else {
        os.pretty_name.clone()
    };
    if fake_amd_enabled() {
        let allow_real_setup = fake_amd_allow_rocm_setup_enabled();
        return RocmGuidedStatus {
            distro_family: family,
            distro_label,
            supported: allow_real_setup,
            amd_detected: true,
            gpu_name,
            ready: true,
            requires_relogin: false,
            detail: if allow_real_setup {
                "Fake AMD install-test mode is active. Runtime validation is simulated because no real AMD GPU is present.".to_string()
            } else {
                "Fake AMD mode is active. ROCm readiness is being simulated for UI/install testing on a non-AMD system.".to_string()
            },
        };
    }
    let (ready, requires_relogin, notes) = rocm_runtime_ready();
    let detail = if gpu_name.is_none() {
        "AMD GPU not detected on this system.".to_string()
    } else if !supported {
        "Guided ROCm setup is not available for this Linux distribution family.".to_string()
    } else if ready {
        "ROCm runtime looks ready for use.".to_string()
    } else if requires_relogin {
        "ROCm install needs a logout/login or reboot, then Check ROCm again.".to_string()
    } else {
        let _ = notes;
        "ROCm not ready. Run Guided ROCm Setup, then Check ROCm again.".to_string()
    };
    RocmGuidedStatus {
        distro_family: family,
        distro_label,
        supported,
        amd_detected: gpu_name.is_some(),
        gpu_name,
        ready,
        requires_relogin,
        detail,
    }
}

#[tauri::command]
fn get_xpu_guided_status() -> XpuGuidedStatus {
    let os = detect_linux_os_release();
    let family = detect_linux_distro_family();
    let gpu_name = detect_intel_gpu_name();
    let supported = xpu_supported_for_distro(&os, &family);
    let distro_label = if os.pretty_name.trim().is_empty() {
        family.clone()
    } else {
        os.pretty_name.clone()
    };
    if fake_intel_enabled() {
        let allow_real_setup = fake_intel_allow_xpu_setup_enabled();
        return XpuGuidedStatus {
            distro_family: family,
            distro_label,
            supported: allow_real_setup,
            intel_detected: true,
            gpu_name,
            ready: true,
            requires_relogin: false,
            detail: if allow_real_setup {
                "Fake Intel install-test mode is active. Runtime validation is simulated because no real Intel GPU is present.".to_string()
            } else {
                "Fake Intel mode is active. Intel XPU readiness is being simulated for UI/install testing on a non-Intel system.".to_string()
            },
        };
    }
    let (ready, requires_relogin, notes) = xpu_runtime_ready_for_distro(&family);
    let detail = if gpu_name.is_none() {
        "Intel GPU not detected on this system.".to_string()
    } else if !supported {
        "Guided Intel setup is not available for this Linux distribution family.".to_string()
    } else if ready {
        "Intel XPU runtime looks ready for use.".to_string()
    } else if requires_relogin {
        "Intel GPU setup needs a logout/login or reboot, then Check Intel XPU again.".to_string()
    } else {
        let _ = notes;
        "Intel XPU is not ready. Run Guided Intel Setup, then Check Intel XPU again.".to_string()
    };
    XpuGuidedStatus {
        distro_family: family,
        distro_label,
        supported,
        intel_detected: gpu_name.is_some(),
        gpu_name,
        ready,
        requires_relogin,
        detail,
    }
}

#[tauri::command]
async fn install_rocm_guided(app: AppHandle) -> Result<RocmGuidedStatus, String> {
    tokio::task::spawn_blocking(move || install_rocm_guided_internal(&app))
        .await
        .map_err(|err| format!("ROCm guided setup task failed: {err}"))?
}

#[tauri::command]
async fn install_xpu_guided(app: AppHandle) -> Result<XpuGuidedStatus, String> {
    tokio::task::spawn_blocking(move || install_xpu_guided_internal(&app))
        .await
        .map_err(|err| format!("Intel guided setup task failed: {err}"))?
}

#[tauri::command]
fn get_comfyui_install_recommendation(
    gpu_selection: Option<String>,
) -> ComfyInstallRecommendation {
    let mut gpu = detect_nvidia_gpu_details();
    let mut amd_name = detect_amd_gpu_name();
    let mut intel_name = detect_intel_gpu_name();
    let detection_pending = gpu_detection_pending();
    if !detection_pending {
        if gpu.name.is_none() {
            gpu = detect_nvidia_gpu_details();
        }
        if amd_name.is_none() {
            amd_name = detect_amd_gpu_name();
        }
        if intel_name.is_none() {
            intel_name = detect_intel_gpu_name();
        }
    }
    comfy_install_recommendation_for(
        gpu,
        amd_name,
        intel_name,
        gpu_selection.as_deref(),
        detection_pending,
    )
}

fn comfy_install_recommendation_for(
    gpu: NvidiaGpuDetails,
    amd_name: Option<String>,
    intel_name: Option<String>,
    gpu_selection: Option<&str>,
    detection_pending: bool,
) -> ComfyInstallRecommendation {
    let selection = gpu_selection.unwrap_or("auto").trim().to_ascii_lowercase();
    let gpu_name = gpu.name.clone().unwrap_or_default().to_ascii_lowercase();
    if selection == "amd" || (selection == "auto" && gpu.name.is_none()) {
        if let Some(amd_name) = amd_name.clone() {
            return ComfyInstallRecommendation {
                gpu_name: Some(amd_name),
                driver_version: None,
                torch_profile: "torch211_rocm72".to_string(),
                torch_label: "Torch 2.11.0 + ROCm 7.2".to_string(),
                reason: if selection == "amd" {
                    "Selected AMD GPU; using ROCm install profile.".to_string()
                } else {
                    "Detected AMD GPU; selecting ROCm install profile.".to_string()
                },
                detection_pending,
            };
        }
        if selection == "amd" {
            return ComfyInstallRecommendation {
                gpu_name: None,
                driver_version: None,
                torch_profile: "torch211_rocm72".to_string(),
                torch_label: "Torch 2.11.0 + ROCm 7.2".to_string(),
                reason: "Selected AMD GPU is still being detected.".to_string(),
                detection_pending,
            };
        }
    }
    if selection == "intel"
        || (selection == "auto" && gpu.name.is_none() && amd_name.is_none())
    {
        if let Some(intel_name) = intel_name {
            return ComfyInstallRecommendation {
                gpu_name: Some(intel_name),
                driver_version: None,
                torch_profile: "torch291_xpu".to_string(),
                torch_label: "Torch 2.9.1 + XPU".to_string(),
                reason: if selection == "intel" {
                    "Selected Intel GPU; using XPU install profile.".to_string()
                } else {
                    "Detected Intel GPU; selecting XPU install profile.".to_string()
                },
                detection_pending,
            };
        }
        if selection == "intel" {
            return ComfyInstallRecommendation {
                gpu_name: None,
                driver_version: None,
                torch_profile: "torch291_xpu".to_string(),
                torch_label: "Torch 2.9.1 + XPU".to_string(),
                reason: "Selected Intel GPU is still being detected.".to_string(),
                detection_pending,
            };
        }
    }
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
            reason: "Selected NVIDIA RTX 3000 series (Ampere).".to_string(),
            detection_pending,
        };
    }

    if gpu_name.contains("rtx 40") {
        return ComfyInstallRecommendation {
            gpu_name: gpu.name,
            driver_version: gpu.driver_version,
            torch_profile: "torch280_cu128".to_string(),
            torch_label: "Torch 2.8.0 + cu128".to_string(),
            reason: "Selected NVIDIA RTX 4000 series (Ada).".to_string(),
            detection_pending,
        };
    }

    if gpu_name.contains("rtx 50") {
        if driver_major >= 580 {
            return ComfyInstallRecommendation {
                gpu_name: gpu.name,
                driver_version: gpu.driver_version,
                torch_profile: "torch291_cu130".to_string(),
                torch_label: "Torch 2.9.1 + cu130".to_string(),
                reason: "Selected NVIDIA RTX 5000 series with driver >= 580.".to_string(),
                detection_pending,
            };
        }

        return ComfyInstallRecommendation {
            gpu_name: gpu.name,
            driver_version: gpu.driver_version,
            torch_profile: "torch280_cu128".to_string(),
            torch_label: "Torch 2.8.0 + cu128".to_string(),
            reason: "Selected NVIDIA RTX 5000 series with older driver; using safer fallback."
                .to_string(),
            detection_pending,
        };
    }

    let reason = if gpu.name.is_some() {
        "Detected NVIDIA GPU; using default CUDA recommendation.".to_string()
    } else {
        "Unknown or non-NVIDIA GPU; using default recommendation.".to_string()
    };
    ComfyInstallRecommendation {
        gpu_name: gpu.name,
        driver_version: gpu.driver_version,
        torch_profile: "torch280_cu128".to_string(),
        torch_label: "Torch 2.8.0 + cu128".to_string(),
        reason,
        detection_pending,
    }
}

pub(crate) fn normalize_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Install folder is required.".to_string());
    }
    let normalized_input = trimmed.replace('\\', "/");

    let mut path = PathBuf::from(normalized_input);
    if !path.is_absolute() {
        path = std::env::current_dir()
            .map_err(|err| err.to_string())?
            .join(path);
    }
    Ok(normalize_canonical_path(&path))
}

pub(crate) fn write_extra_model_paths_yaml(
    comfy_dir: &Path,
    base_path: &Path,
    is_default: bool,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(base_path).map_err(|err| {
        format!(
            "failed to prepare extra models folder '{}': {err}",
            base_path.display()
        )
    })?;

    let target = comfy_dir.join("extra_model_paths.yaml");
    let example = comfy_dir.join("extra_model_paths.yaml.example");
    if !target.exists() {
        if example.exists() {
            std::fs::rename(&example, &target).map_err(|err| {
                format!(
                    "failed to rename '{}' to '{}': {err}",
                    example.display(),
                    target.display()
                )
            })?;
        } else {
            return Err(
                "extra_model_paths.yaml.example was not found in ComfyUI install folder."
                    .to_string(),
            );
        }
    }

    let base = yaml_single_quote(&normalize_canonical_path(base_path).to_string_lossy());
    let default_value = if is_default { "true" } else { "false" };
    let yaml = [
        "# Managed by Arctic ComfyUI Helper.".to_string(),
        "comfyui:".to_string(),
        format!("  base_path: {base}"),
        format!("  is_default: {default_value}"),
        "  checkpoints: models/checkpoints/".to_string(),
        "  text_encoders: |".to_string(),
        "    models/text_encoders/".to_string(),
        "    models/clip/".to_string(),
        "  clip_vision: models/clip_vision/".to_string(),
        "  configs: models/configs/".to_string(),
        "  controlnet: models/controlnet/".to_string(),
        "  diffusion_models: |".to_string(),
        "    models/diffusion_models/".to_string(),
        "    models/unet/".to_string(),
        "  embeddings: models/embeddings/".to_string(),
        "  loras: models/loras/".to_string(),
        "  upscale_models: models/upscale_models/".to_string(),
        "  vae: models/vae/".to_string(),
        "  audio_encoders: models/audio_encoders/".to_string(),
        "  model_patches: models/model_patches/".to_string(),
    ]
    .join("\n")
        + "\n";

    std::fs::write(&target, yaml).map_err(|err| {
        format!(
            "failed to write extra model paths config '{}': {err}",
            target.display()
        )
    })?;

    Ok(target)
}

fn is_forbidden_install_path(path: &Path) -> bool {
    // Mirrors app_windows.rs's is_forbidden_install_path: block the
    // filesystem root and well-known system directories so an install/base
    // folder can't be pointed at a location whose contents the app might
    // later create/clear/overwrite (see run_comfyui_preflight,
    // set_comfyui_install_base).
    let normalized = path.to_string_lossy().trim_end_matches('/').to_string();
    let normalized = if normalized.is_empty() {
        "/".to_string()
    } else {
        normalized
    };

    if normalized == "/" {
        return true;
    }

    const BLOCKED_EXACT: &[&str] = &[
        "/bin", "/boot", "/dev", "/etc", "/lib", "/lib32", "/lib64", "/libx32", "/proc", "/root",
        "/run", "/sbin", "/sys", "/usr", "/var",
    ];
    if BLOCKED_EXACT
        .iter()
        .any(|entry| normalized == *entry || normalized.starts_with(&format!("{entry}/")))
    {
        return true;
    }

    // Also refuse the user's home directory itself (subfolders like
    // ~/ComfyUI remain allowed) so a stray/empty install-path value can't
    // resolve to "wipe everything under my home directory".
    if let Ok(home) = std::env::var("HOME") {
        let home = home.trim_end_matches('/');
        if !home.is_empty() && normalized == home {
            return true;
        }
    }

    false
}

fn normalize_canonical_path(path: &Path) -> PathBuf {
    // Best-effort canonicalize: most call sites already canonicalize before
    // calling this (so this is a harmless no-op re-resolve for them), but
    // `normalize_path` did not, which meant an install root containing a
    // symlink component (e.g. `~/ComfyUI` under a symlinked home directory,
    // or a bind-mounted models dir) could be stored one way at install time
    // and resolved a different way at launch time by `resolve_root_path`
    // (which does canonicalize), silently discarding the saved launch
    // settings because the two forms compared unequal. Falls back to the
    // input unchanged when the path doesn't exist yet (e.g. a fresh install
    // target that hasn't been created), matching the previous behavior for
    // that case.
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn command_available(program: &str, args: &[&str]) -> bool {
    let mut cmd = match build_command(program, args, None, &[]) {
        Ok(cmd) => cmd,
        Err(_) => return false,
    };
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

fn apply_background_command_flags(_cmd: &mut std::process::Command) {
    let _ = _cmd;
}

fn running_in_flatpak() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists()
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

fn build_command(
    program: &str,
    args: &[&str],
    working_dir: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<std::process::Command, String> {
    let mut cmd = if running_in_flatpak() && program != "flatpak-spawn" {
        let mut wrapped = std::process::Command::new("flatpak-spawn");
        wrapped.arg("--host");
        if let Some(dir) = working_dir {
            wrapped.arg(format!("--directory={}", dir.to_string_lossy()));
        }
        for (key, value) in envs {
            wrapped.arg(format!("--env={key}={value}"));
        }
        wrapped.arg(program);
        wrapped.args(args);
        wrapped
    } else {
        let mut direct = std::process::Command::new(program);
        direct.args(args);
        if let Some(dir) = working_dir {
            direct.current_dir(dir);
        }
        for (key, value) in envs {
            direct.env(key, value);
        }
        direct
    };
    if !(running_in_flatpak() && program != "flatpak-spawn") {
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }
        for (key, value) in envs {
            cmd.env(key, value);
        }
    }
    apply_background_command_flags(&mut cmd);
    Ok(cmd)
}

fn valid_shell_env_value() -> Option<String> {
    let shell_from_env = std::env::var("SHELL").ok();
    let shells_file = std::fs::read_to_string("/etc/shells").ok();
    if let (Some(shell), Some(shells)) = (shell_from_env.as_deref(), shells_file.as_deref()) {
        if shells.lines().map(str::trim).any(|line| line == shell) {
            return Some(shell.to_string());
        }
    }
    ["/bin/bash", "/bin/sh"]
        .into_iter()
        .find(|path| Path::new(path).exists())
        .map(str::to_string)
}

fn try_attach_parent_console() {}

fn ensure_git_available(app: &AppHandle) -> Result<(), String> {
    let _ = app;
    if command_available("git", &["--version"]) {
        return Ok(());
    }
    Err("Git is not available in PATH. Install Git and retry.".to_string())
}

fn get_hf_xet_preflight_internal(xet_enabled: bool) -> HfXetPreflightResponse {
    let uvx_hf_available = command_available("uvx", &["hf", "--help"]);
    let hf_native_available = command_available("hf", &["--help"]);
    let hf_cli_available = uvx_hf_available || hf_native_available;
    let hf_backend = if uvx_hf_available {
        "uvx hf".to_string()
    } else if hf_native_available {
        "hf".to_string()
    } else {
        "none".to_string()
    };

    if !hf_cli_available {
        return HfXetPreflightResponse {
            xet_enabled,
            hf_cli_available,
            hf_backend,
            hf_xet_installed: false,
            hub_version: None,
            detail: "HF CLI backend not found. Install uv (`https://docs.astral.sh/uv/`) for `uvx hf`, or install `hf` (`pip install -U huggingface_hub hf_xet`).".to_string(),
        };
    }

    let env_probe = if uvx_hf_available {
        run_command_capture("uvx", &["hf", "env"], None)
    } else {
        run_command_capture("hf", &["env"], None)
    };

    match env_probe {
        Ok((stdout, _stderr)) => {
            let hf_xet_raw = parse_hf_env_value(&stdout, "hf_xet").unwrap_or_default();
            let hub_version = parse_hf_env_value(&stdout, "huggingface_hub version");
            let hf_xet_installed = {
                let normalized = hf_xet_raw.trim().to_ascii_lowercase();
                !normalized.is_empty() && normalized != "n/a" && normalized != "none"
            };

            let detail = if !xet_enabled {
                format!(
                    "Xet is installed but disabled in app settings (backend: {}).",
                    hf_backend
                )
            } else if hf_xet_installed {
                format!(
                    "HF/Xet preflight OK via {} (huggingface_hub {}, hf_xet {}).",
                    hf_backend,
                    hub_version.clone().unwrap_or_else(|| "unknown".to_string()),
                    hf_xet_raw
                )
            } else {
                format!(
                    "HF backend {} found, but hf_xet is missing. Run `pip install -U huggingface_hub hf_xet`.",
                    hf_backend
                )
            };

            HfXetPreflightResponse {
                xet_enabled,
                hf_cli_available,
                hf_backend,
                hf_xet_installed,
                hub_version,
                detail,
            }
        }
        Err(err) => HfXetPreflightResponse {
            xet_enabled,
            hf_cli_available,
            hf_backend,
            hf_xet_installed: false,
            hub_version: None,
            detail: format!("Could not run HF env probe: {err}"),
        },
    }
}

#[tauri::command]
fn get_hf_xet_preflight(state: State<'_, AppState>) -> HfXetPreflightResponse {
    let xet_enabled = state.context.config.settings().hf_xet_enabled;
    get_hf_xet_preflight_internal(xet_enabled)
}

fn ensure_hf_xet_runtime_installed(always_upgrade: bool) -> Result<(), String> {
    let before = get_hf_xet_preflight_internal(true);

    let mut attempts: Vec<String> = Vec::new();
    if !command_available("uv", &["--version"]) {
        return Err("uv is required for HF/Xet setup but was not found in PATH.".to_string());
    }
    if always_upgrade || !before.hf_xet_installed {
        match run_command_capture(
            "uv",
            &[
                "tool",
                "install",
                "--upgrade",
                "--force",
                "huggingface_hub[hf_xet]",
            ],
            None,
        ) {
            Ok(_) => attempts.push(
                "uv tool install --upgrade --force huggingface_hub[hf_xet] => ok".to_string(),
            ),
            Err(err) => {
                attempts.push(format!(
                    "uv tool install --upgrade --force huggingface_hub[hf_xet] => {err}"
                ));
            }
        }
    }

    let after = get_hf_xet_preflight_internal(true);
    if after.hf_cli_available && after.hf_xet_installed {
        Ok(())
    } else {
        Err(format!(
            "Could not prepare HF/Xet runtime. {}. attempts: {}",
            after.detail,
            attempts.join(" | ")
        ))
    }
}

#[tauri::command]
async fn set_hf_xet_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppSettings, String> {
    if enabled {
        // `uv tool install ... huggingface_hub[hf_xet]` fetches and
        // installs a package over the network; run it off the command
        // dispatch thread like the rest of this file's install/mutation
        // commands.
        tauri::async_runtime::spawn_blocking(|| ensure_hf_xet_runtime_installed(true))
            .await
            .map_err(|err| format!("HF/Xet setup task failed: {err}"))??;
    }
    state
        .context
        .config
        .update_settings(|settings| settings.hf_xet_enabled = enabled)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn run_comfyui_preflight(
    state: State<'_, AppState>,
    request: ComfyInstallRequest,
) -> ComfyPreflightResponse {
    let mut items: Vec<PreflightItem> = Vec::new();
    let mut ok = true;

    if request.install_root.trim().is_empty() {
        push_preflight(
            &mut items,
            "warn",
            "Install base folder",
            "Select an install folder to run full preflight checks.",
        );
        return ComfyPreflightResponse {
            ok: false,
            summary: "Install folder not selected yet.".to_string(),
            items,
        };
    }

    let base_root = match normalize_path(&request.install_root) {
        Ok(path) => path,
        Err(err) => {
            push_preflight(&mut items, "fail", "Install base folder", err);
            return ComfyPreflightResponse {
                ok: false,
                summary: "Preflight failed.".to_string(),
                items,
            };
        }
    };

    if is_forbidden_install_path(&base_root) {
        ok = false;
        push_preflight(
            &mut items,
            "fail",
            "Install base folder",
            "Folder is blocked (avoid system directories).",
        );
    } else {
        push_preflight(
            &mut items,
            "pass",
            "Install base folder",
            format!("Using {}", base_root.display()),
        );
    }

    if std::fs::create_dir_all(&base_root).is_ok() {
        let probe = base_root.join(".arctic-write-test");
        match std::fs::write(&probe, b"ok") {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
                push_preflight(
                    &mut items,
                    "pass",
                    "Write permission",
                    "Folder is writable.",
                );
            }
            Err(err) => {
                ok = false;
                push_preflight(
                    &mut items,
                    "fail",
                    "Write permission",
                    format!("Cannot write to selected folder: {err}"),
                );
            }
        }
    } else {
        ok = false;
        push_preflight(
            &mut items,
            "fail",
            "Write permission",
            "Could not create selected base folder.",
        );
    }

    match fs2::available_space(&base_root) {
        Ok(bytes) => {
            let gb = bytes as f64 / 1024f64 / 1024f64 / 1024f64;
            if gb < 40.0 {
                ok = false;
                push_preflight(
                    &mut items,
                    "fail",
                    "Disk space",
                    format!("Only {gb:.1} GB free. Recommended at least 40 GB."),
                );
            } else if gb < 80.0 {
                push_preflight(
                    &mut items,
                    "warn",
                    "Disk space",
                    format!(
                        "{gb:.1} GB free. Installation should work but more free space is safer."
                    ),
                );
            } else {
                push_preflight(
                    &mut items,
                    "pass",
                    "Disk space",
                    format!("{gb:.1} GB free."),
                );
            }
        }
        Err(err) => {
            push_preflight(
                &mut items,
                "warn",
                "Disk space",
                format!("Unable to check free space: {err}"),
            );
        }
    }

    if command_available("git", &["--version"]) {
        push_preflight(&mut items, "pass", "Git", "Git is available.");
    } else {
        ok = false;
        push_preflight(
            &mut items,
            "fail",
            "Git",
            "Git is not available in PATH. Install Git and retry.",
        );
    }

    let dns_ok = has_dns("github.com", 443) && has_dns("pypi.org", 443);
    if dns_ok {
        push_preflight(
            &mut items,
            "pass",
            "Network",
            "DNS lookup for required hosts is available.",
        );
    } else {
        push_preflight(
            &mut items,
            "warn",
            "Network",
            "Could not resolve one or more hosts (github.com, pypi.org). Install may fail offline.",
        );
    }

    if let Some(found) = discover_uv_binary() {
        let detail = if found == "uv" {
            "System uv detected.".to_string()
        } else {
            format!("uv detected at {}.", found)
        };
        push_preflight(&mut items, "pass", "uv runtime", detail);
    } else {
        push_preflight(
            &mut items,
            "warn",
            "uv runtime",
            "uv not found. Installer will auto-install uv for current user during ComfyUI install.",
        );
    }

    let hf_xet = get_hf_xet_preflight_internal(state.context.config.settings().hf_xet_enabled);
    if !hf_xet.hf_cli_available {
        push_preflight(&mut items, "warn", "HF/Xet acceleration", hf_xet.detail);
    } else if hf_xet.hf_xet_installed && hf_xet.xet_enabled {
        push_preflight(&mut items, "pass", "HF/Xet acceleration", hf_xet.detail);
    } else {
        push_preflight(&mut items, "warn", "HF/Xet acceleration", hf_xet.detail);
    }

    match get_linux_prereq_cache_or_scan() {
        Ok(scan) => {
            if scan.missing_required.is_empty() {
                push_preflight(
                    &mut items,
                    "pass",
                    "Linux system packages",
                    format!("{} prerequisites are installed.", scan.distro),
                );
            } else {
                push_preflight(
                    &mut items,
                    "warn",
                    "Linux system packages",
                    format!(
                        "Missing required packages for {}: {}. Installer will attempt to install them automatically.",
                        scan.distro,
                        scan.missing_required.join(", ")
                    ),
                );
            }
            if !scan.missing_optional.is_empty() {
                push_preflight(
                    &mut items,
                    "warn",
                    "Linux optional packages",
                    format!(
                        "Missing optional packages (installer may continue): {}",
                        scan.missing_optional.join(", ")
                    ),
                );
            }
        }
        Err(err) => {
            push_preflight(
                &mut items,
                "warn",
                "Linux system packages",
                format!("Could not evaluate distro package prerequisites: {err}"),
            );
        }
    }

    let selected_attention = [
        request.include_sage_attention,
        request.include_sage_attention3,
        request.include_flash_attention,
        request.include_nunchaku,
    ]
    .into_iter()
    .filter(|v| *v)
    .count();
    if selected_attention > 1 {
        ok = false;
        push_preflight(
            &mut items,
            "fail",
            "Attention add-on selection",
            "Select only one of SageAttention / SageAttention3 / FlashAttention / Nunchaku.",
        );
    } else {
        push_preflight(
            &mut items,
            "pass",
            "Attention add-on selection",
            "Selection is valid.",
        );
    }

    if request.include_sage_attention3 {
        let gpu = detect_nvidia_gpu_details();
        let allowed = gpu
            .name
            .as_deref()
            .map(|n| n.to_ascii_lowercase().contains("rtx 50"))
            .unwrap_or(false);
        if allowed {
            push_preflight(
                &mut items,
                "pass",
                "SageAttention3 compatibility",
                "RTX 50-series detected.",
            );
        } else {
            ok = false;
            push_preflight(
                &mut items,
                "fail",
                "SageAttention3 compatibility",
                "SageAttention3 requires NVIDIA RTX 50-series.",
            );
        }
    }

    let selected_profile = request
        .torch_profile
        .clone()
        .unwrap_or_else(|| get_comfyui_install_recommendation(None).torch_profile);
    if torch_profile_is_rocm(&selected_profile) {
        let status = get_rocm_guided_status();
        if status.ready {
            push_preflight(&mut items, "pass", "ROCm runtime", status.detail);
        } else if status.requires_relogin {
            ok = false;
            push_preflight(
                &mut items,
                "fail",
                "ROCm runtime",
                "ROCm install appears incomplete for the current session. Log out and back in, or reboot, then run the ROCm check again.",
            );
        } else {
            ok = false;
            push_preflight(&mut items, "fail", "ROCm runtime", status.detail);
        }
    }
    if torch_profile_is_xpu(&selected_profile) {
        let status = get_xpu_guided_status();
        if status.ready {
            push_preflight(&mut items, "pass", "Intel XPU runtime", status.detail);
        } else if status.requires_relogin {
            ok = false;
            push_preflight(
                &mut items,
                "fail",
                "Intel XPU runtime",
                "Intel GPU setup appears incomplete for the current session. Log out and back in, or reboot, then run the Intel XPU check again.",
            );
        } else {
            ok = false;
            push_preflight(&mut items, "fail", "Intel XPU runtime", status.detail);
        }
    }
    if torch_profile_is_rocm(&selected_profile) {
        let incompatible: Vec<&str> = [
            (request.include_sage_attention, "SageAttention"),
            (request.include_sage_attention3, "SageAttention3"),
            (request.include_flash_attention, "FlashAttention"),
            (request.include_nunchaku, "Nunchaku"),
            (request.include_trellis2, "Trellis2"),
        ]
        .into_iter()
        .filter_map(|(enabled, name)| enabled.then_some(name))
        .collect();
        if incompatible.is_empty() {
            push_preflight(
                &mut items,
                "pass",
                "ROCm add-on compatibility",
                "Selected add-ons are compatible with the ROCm install profile.",
            );
        } else {
            ok = false;
            push_preflight(
                &mut items,
                "fail",
                "ROCm add-on compatibility",
                format!(
                    "These options are currently CUDA-only in this app and must be disabled for ROCm installs: {}.",
                    incompatible.join(", ")
                ),
            );
        }
    }
    if torch_profile_is_xpu(&selected_profile) {
        let incompatible: Vec<&str> = [
            (request.include_sage_attention, "SageAttention"),
            (request.include_sage_attention3, "SageAttention3"),
            (request.include_flash_attention, "FlashAttention"),
            (request.include_nunchaku, "Nunchaku"),
            (request.include_trellis2, "Trellis2"),
        ]
        .into_iter()
        .filter_map(|(enabled, name)| enabled.then_some(name))
        .collect();
        if incompatible.is_empty() {
            push_preflight(
                &mut items,
                "pass",
                "Intel XPU add-on compatibility",
                "Selected add-ons are compatible with the Intel XPU install profile.",
            );
        } else {
            ok = false;
            push_preflight(
                &mut items,
                "fail",
                "Intel XPU add-on compatibility",
                format!(
                    "These options are currently CUDA-only in this app and must be disabled for Intel XPU installs: {}.",
                    incompatible.join(", ")
                ),
            );
        }
    }

    if request.include_trellis2 {
        let trellis_supported = matches!(selected_profile.as_str(), "torch280_cu128");
        if trellis_supported {
            push_preflight(
                &mut items,
                "pass",
                "Trellis2 compatibility",
                "Compatible Torch profile selected.",
            );
        } else {
            ok = false;
            push_preflight(
                &mut items,
                "fail",
                "Trellis2 compatibility",
                "Trellis2 currently requires Torch 2.8.0 + cu128 (Torch280 wheel set).",
            );
        }
    }

    let summary = if ok {
        "Preflight passed.".to_string()
    } else {
        "Preflight has blocking issues.".to_string()
    };
    ComfyPreflightResponse { ok, summary, items }
}


fn download_http_file(url: &str, out_file: &Path) -> Result<(), String> {
    if let Some(parent) = out_file.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create download directory {}: {err}",
                parent.display()
            )
        })?;
    }

    let tmp_file = out_file.with_extension("download");
    let user_agent = "ArcticComfyUIHelper/0.3.4";

    let curl_output = std::process::Command::new("curl")
        .arg("-fL")
        .arg("--retry")
        .arg("3")
        .arg("--connect-timeout")
        .arg("20")
        .arg("-A")
        .arg(user_agent)
        .arg("-o")
        .arg(&tmp_file)
        .arg(url)
        .output();

    let downloaded = match curl_output {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let wget_output = std::process::Command::new("wget")
                .arg("--tries=3")
                .arg("--timeout=20")
                .arg("--user-agent")
                .arg(user_agent)
                .arg("-O")
                .arg(&tmp_file)
                .arg(url)
                .output();
            match wget_output {
                Ok(wget) if wget.status.success() => true,
                Ok(wget) => {
                    let wget_err = String::from_utf8_lossy(&wget.stderr).trim().to_string();
                    return Err(format!(
                        "HTTP download failed for {url}. curl: {stderr}. wget: {wget_err}"
                    ));
                }
                Err(wget_err) => {
                    return Err(format!(
                        "HTTP download failed for {url}. curl: {stderr}. wget launch failed: {wget_err}"
                    ));
                }
            }
        }
        Err(_) => {
            let wget_output = std::process::Command::new("wget")
                .arg("--tries=3")
                .arg("--timeout=20")
                .arg("--user-agent")
                .arg(user_agent)
                .arg("-O")
                .arg(&tmp_file)
                .arg(url)
                .output();
            match wget_output {
                Ok(wget) if wget.status.success() => true,
                Ok(wget) => {
                    let wget_err = String::from_utf8_lossy(&wget.stderr).trim().to_string();
                    return Err(format!(
                        "HTTP download failed for {url}. curl and wget failed. wget: {wget_err}"
                    ));
                }
                Err(wget_err) => {
                    return Err(format!(
                        "HTTP download failed for {url}. Neither curl nor wget is available: {wget_err}"
                    ));
                }
            }
        }
    };

    if downloaded {
        std::fs::rename(&tmp_file, out_file).map_err(|err| {
            format!(
                "Failed to finalize download {} -> {}: {err}",
                tmp_file.display(),
                out_file.display()
            )
        })?;
    }

    Ok(())
}

fn run_command(program: &str, args: &[&str], working_dir: Option<&Path>) -> Result<(), String> {
    log::debug!("run_command: {} {}", program, args.join(" "));
    let mut cmd = build_command(program, args, working_dir, &[])?;
    let status = cmd
        .status()
        .map_err(|err| format!("Failed to run {program}: {err}"))?;
    if !status.success() {
        return Err(format!("Command failed: {} {}", program, args.join(" ")));
    }
    Ok(())
}

fn run_command_with_env(
    program: &str,
    args: &[&str],
    working_dir: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<(), String> {
    log::debug!("run_command_with_env: {} {}", program, args.join(" "));
    let mut cmd = build_command(program, args, working_dir, envs)?;
    let status = cmd
        .status()
        .map_err(|err| format!("Failed to run {program}: {err}"))?;
    if !status.success() {
        return Err(format!("Command failed: {} {}", program, args.join(" ")));
    }
    Ok(())
}

fn can_use_interactive_sudo() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

fn run_privileged_command(
    program: &str,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<(), String> {
    let mut sudo_non_interactive: Vec<&str> = vec!["-n", program];
    sudo_non_interactive.extend_from_slice(args);
    if run_command("sudo", &sudo_non_interactive, working_dir).is_ok() {
        return Ok(());
    }

    let mut pkexec_args: Vec<&str> = vec![program];
    pkexec_args.extend_from_slice(args);
    if run_command("pkexec", &pkexec_args, working_dir).is_ok() {
        return Ok(());
    }

    if can_use_interactive_sudo() {
        let mut sudo_args: Vec<&str> = vec![program];
        sudo_args.extend_from_slice(args);
        if run_command("sudo", &sudo_args, working_dir).is_ok() {
            return Ok(());
        }
    }

    Err(format!(
        "Privilege escalation failed for {} {}. If running from desktop GUI, ensure a PolicyKit agent is active; otherwise run with --nerdstats from a terminal so sudo can prompt.",
        program,
        args.join(" ")
    ))
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn rocm_group_setup_steps(user: &str) -> Vec<String> {
    vec![
        "echo Adding current user to render/video groups...".to_string(),
        "getent group render >/dev/null || groupadd -f render".to_string(),
        "getent group video >/dev/null || groupadd -f video".to_string(),
        format!(
            "usermod -aG render,video {} || echo Warning: could not update render/video groups automatically.",
            shell_single_quote(user)
        ),
    ]
}

fn run_privileged_shell_streaming(
    app: &AppHandle,
    script: &str,
    working_dir: Option<&Path>,
) -> Result<(), String> {
    let shell_env_value = valid_shell_env_value();
    let shell_env: Vec<(&str, &str)> = shell_env_value
        .as_deref()
        .map(|shell| vec![("SHELL", shell)])
        .unwrap_or_default();
    let sudo_err = if can_use_interactive_sudo() {
        emit_guided_setup_event(
            app,
            "step",
            "Requesting sudo authentication for guided ROCm setup...",
        );
        run_command_with_env("sudo", &["-v"], working_dir, &shell_env)?;
        if run_command_streaming_with_env(
            app,
            "step",
            "sudo",
            &["-n", "sh", "-lc", script],
            working_dir,
            &shell_env,
        )
        .is_ok()
        {
            return Ok(());
        } else {
            "sudo credentials were not accepted for non-interactive execution.".to_string()
        }
    } else {
        match run_command_streaming_with_env(
            app,
            "step",
            "sudo",
            &["-n", "sh", "-lc", script],
            working_dir,
            &shell_env,
        ) {
            Ok(()) => return Ok(()),
            Err(err) => err,
        }
    };

    emit_guided_setup_event(
        app,
        "warn",
        "Trying PolicyKit authentication for guided ROCm setup...",
    );
    let pkexec_result = run_command_streaming_with_env(
        app,
        "step",
        "pkexec",
        &["sh", "-lc", script],
        working_dir,
        &shell_env,
    );
    if pkexec_result.is_ok() {
        return Ok(());
    }

    let pkexec_err = pkexec_result.err().unwrap_or_default();
    let lower_pkexec = pkexec_err.to_ascii_lowercase();
    let lower_sudo = sudo_err.to_ascii_lowercase();
    if lower_sudo.contains("password is required")
        && (lower_pkexec.contains("error getting authority")
            || lower_pkexec.contains("could not connect"))
    {
        return Err(
            "Guided ROCm setup could not authenticate in this environment. PolicyKit is unavailable, and sudo cannot prompt here. In distrobox or terminal testing, run `sudo -v` in the same shell first, then retry Guided ROCm Setup."
                .to_string(),
        );
    }

    Err(format!(
        "Privilege escalation failed for guided ROCm setup. sudo: {} pkexec: {}",
        sudo_err, pkexec_err
    ))
}

pub(crate) fn run_command_capture(
    program: &str,
    args: &[&str],
    working_dir: Option<&Path>,
) -> Result<(String, String), String> {
    log::debug!("run_command_capture: {} {}", program, args.join(" "));
    let mut cmd = build_command(program, args, working_dir, &[])?;
    let output = cmd
        .output()
        .map_err(|err| format!("Failed to run {program}: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        let tail = if stderr.trim().is_empty() {
            stdout
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            stderr
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        };
        return Err(format!(
            "Command failed: {} {} :: {}",
            program,
            args.join(" "),
            tail
        ));
    }
    Ok((stdout, stderr))
}

fn run_command_with_retry(
    program: &str,
    args: &[&str],
    working_dir: Option<&Path>,
    retries: usize,
) -> Result<(), String> {
    let attempts = retries.max(1);
    let mut last_err = String::new();
    for attempt in 1..=attempts {
        match run_command_capture(program, args, working_dir) {
            Ok(_) => return Ok(()),
            Err(err) => {
                last_err = err;
                if attempt < attempts {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }
    }
    Err(last_err)
}

fn run_command_env(
    program: &str,
    args: &[&str],
    working_dir: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<(), String> {
    log::debug!("run_command_env: {} {}", program, args.join(" "));
    let mut cmd = build_command(program, args, working_dir, envs)?;
    let status = cmd
        .status()
        .map_err(|err| format!("Failed to run {program}: {err}"))?;
    if !status.success() {
        return Err(format!("Command failed: {} {}", program, args.join(" ")));
    }
    Ok(())
}

fn pip_uninstall_best_effort(root: &Path, py_path: &str, packages: &[&str]) {
    let uv_bin = discover_uv_binary();
    for package in packages {
        if let Some(uv) = uv_bin.as_deref() {
            let _ = run_uv_pip_strict(uv, py_path, &["uninstall", package], Some(root), &[]);
        } else {
            let _ = run_command_capture(
                py_path,
                &["-m", "pip", "uninstall", "-y", package],
                Some(root),
            );
        }
    }
}

fn normalize_pkg_token(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn remove_site_packages_artifacts_with_markers(
    root: &Path,
    markers: &[String],
) -> Result<(), String> {
    for venv_name in [".venv", "venv"] {
        let venv_lib = root.join(venv_name).join("lib");
        let py_entries = match std::fs::read_dir(&venv_lib) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for py in py_entries.flatten() {
            let site = py.path().join("site-packages");
            let entries = match std::fs::read_dir(&site) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let token = normalize_pkg_token(&name);
                if !markers.iter().any(|marker| token.contains(marker)) {
                    continue;
                }
                if path.is_dir() {
                    let _ = std::fs::remove_dir_all(&path);
                } else {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
    Ok(())
}

fn remove_attention_site_packages_artifacts(root: &Path) -> Result<(), String> {
    let markers = vec![
        normalize_pkg_token("sageattention"),
        normalize_pkg_token("sageattn3"),
        normalize_pkg_token("flash_attn"),
        normalize_pkg_token("nunchaku"),
    ];
    remove_site_packages_artifacts_with_markers(root, &markers)
}

fn remove_insightface_site_packages_artifacts(root: &Path) -> Result<(), String> {
    let markers = vec![
        normalize_pkg_token("insightface"),
        normalize_pkg_token("facexlib"),
        normalize_pkg_token("filterpywhl"),
    ];
    remove_site_packages_artifacts_with_markers(root, &markers)
}

fn force_cleanup_attention_backends(root: &Path, py_path: &str) -> Result<(), String> {
    let pkg_names = [
        "sageattention",
        "sageattn3",
        "flash-attn",
        "flash_attn",
        "nunchaku",
    ];
    pip_uninstall_best_effort(root, py_path, &pkg_names);
    remove_attention_site_packages_artifacts(root)?;
    for folder in ["ComfyUI-nunchaku", "nunchaku_nodes"] {
        let path = root.join("custom_nodes").join(folder);
        if path.exists() {
            let _ = std::fs::remove_dir_all(path);
        }
    }

    let mut lingering: Vec<&str> = Vec::new();
    for pkg in pkg_names {
        if pip_has_package(root, pkg) {
            lingering.push(pkg);
        }
    }
    let mut lingering_modules: Vec<&str> = Vec::new();
    for module in ["sageattention", "sageattn3", "flash_attn", "nunchaku"] {
        if python_module_importable(root, module) {
            lingering_modules.push(module);
        }
    }
    let mut lingering_nodes: Vec<&str> = Vec::new();
    for node in ["ComfyUI-nunchaku", "nunchaku_nodes"] {
        if custom_node_exists(root, node) {
            lingering_nodes.push(node);
        }
    }
    if lingering.is_empty() && lingering_modules.is_empty() && lingering_nodes.is_empty() {
        return Ok(());
    }

    let mut parts: Vec<String> = Vec::new();
    if !lingering.is_empty() {
        parts.push(format!(
            "packages still installed: {}",
            lingering.join(", ")
        ));
    }
    if !lingering_modules.is_empty() {
        parts.push(format!(
            "modules still importable: {}",
            lingering_modules.join(", ")
        ));
    }
    if !lingering_nodes.is_empty() {
        parts.push(format!(
            "nodes still present: {}",
            lingering_nodes.join(", ")
        ));
    }
    Err(format!(
        "Failed to fully remove previous attention backends ({}). Stop ComfyUI and retry.",
        parts.join("; ")
    ))
}

fn insightface_present(root: &Path) -> bool {
    pip_has_package(root, "insightface") || python_module_importable(root, "insightface")
}

fn linux_wheel_url(profile: &str, wheel_kind: &str, hopper_sm90: bool) -> Option<&'static str> {
    match (profile, wheel_kind, hopper_sm90) {
        ("torch271_cu128", "flash", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312-sm90/flash_attn-2.8.3-cp312-cp312-linux_x86_64.whl"),
        ("torch271_cu128", "insightface", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312-sm90/insightface-0.7.3-cp312-cp312-linux_x86_64.whl"),
        ("torch271_cu128", "nunchaku", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312-sm90/nunchaku-1.3.0.dev20260215%2Bcu12.8torch2.7-cp312-cp312-linux_x86_64.whl"),
        ("torch271_cu128", "sage", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312-sm90/sageattention-2.2.0-cp312-cp312-linux_x86_64.whl"),
        ("torch271_cu128", "sage3", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312-sm90/sageattn3-1.0.0-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "flash", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312-sm90/flash_attn-2.8.3-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "insightface", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312-sm90/insightface-0.7.3-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "nunchaku", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312-sm90/nunchaku-1.3.0.dev20260215%2Bcu12.8torch2.8-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "sage", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312-sm90/sageattention-2.2.0-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "sage3", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312-sm90/sageattn3-1.0.0-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "flash", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312-sm90/flash_attn-2.8.3-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "insightface", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312-sm90/insightface-0.7.3-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "nunchaku", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312-sm90/nunchaku-1.3.0.dev20260215%2Bcu13.0torch2.9-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "sage", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312-sm90/sageattention-2.2.0-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "sage3", true) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312-sm90/sageattn3-1.0.0-cp312-cp312-linux_x86_64.whl"),
        ("torch271_cu128", "flash", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312/flash_attn-2.8.3-cp312-cp312-linux_x86_64.whl"),
        ("torch271_cu128", "insightface", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312/insightface-0.7.3-cp312-cp312-linux_x86_64.whl"),
        ("torch271_cu128", "nunchaku", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312/nunchaku-1.3.0.dev20260215%2Bcu12.8torch2.7-cp312-cp312-linux_x86_64.whl"),
        ("torch271_cu128", "sage", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312/sageattention-2.2.0-cp312-cp312-linux_x86_64.whl"),
        ("torch271_cu128", "sage3", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch271-py312/sageattn3-1.0.0-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "flash", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312/flash_attn-2.8.3-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "insightface", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312/insightface-0.7.3-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "nunchaku", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312/nunchaku-1.3.0.dev20260215%2Bcu12.8torch2.8-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "sage", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312/sageattention-2.2.0-cp312-cp312-linux_x86_64.whl"),
        ("torch280_cu128", "sage3", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu128-torch280-py312/sageattn3-1.0.0-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "flash", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312/flash_attn-2.8.3-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "insightface", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312/insightface-0.7.3-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "nunchaku", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312/nunchaku-1.3.0.dev20260215%2Bcu13.0torch2.9-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "sage", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312/sageattention-2.2.0-cp312-cp312-linux_x86_64.whl"),
        ("torch291_cu130", "sage3", false) => Some("https://huggingface.co/arcticlatent/accelerator/resolve/main/cu130-torch291-py312/sageattn3-1.0.0-cp312-cp312-linux_x86_64.whl"),
        _ => None,
    }
}

fn install_linux_wheel_for_profile(
    root: &Path,
    py_path: &str,
    profile: &str,
    wheel_kind: &str,
    hopper_sm90: bool,
    force_reinstall: bool,
) -> Result<(), String> {
    let wheel = linux_wheel_url(profile, wheel_kind, hopper_sm90).ok_or_else(|| {
        format!("No Linux wheel mapping for profile '{profile}' and wheel '{wheel_kind}'.")
    })?;
    let uv_bin = discover_uv_binary().ok_or_else(|| {
        "uv runtime not found. Install uv first or run Install ComfyUI to auto-bootstrap."
            .to_string()
    })?;
    let mut args: Vec<&str> = vec!["install", "--upgrade"];
    if force_reinstall {
        args.push("--reinstall");
    }
    // These are precompiled stack-pinned wheels; let selected torch profile stay authoritative.
    args.push("--no-deps");
    args.push(wheel);
    run_uv_pip_strict(&uv_bin, py_path, &args, Some(root), &[])
}

fn install_sageattention_linux(
    root: &Path,
    py_path: &str,
    profile: &str,
    hopper_sm90: bool,
) -> Result<(), String> {
    install_linux_wheel_for_profile(root, py_path, profile, "sage", hopper_sm90, true)
}

fn install_flashattention_linux(
    root: &Path,
    py_path: &str,
    profile: &str,
    hopper_sm90: bool,
) -> Result<(), String> {
    install_linux_wheel_for_profile(root, py_path, profile, "flash", hopper_sm90, true)
}

fn install_nunchaku_node_requirements(
    root: &Path,
    uv_bin: &str,
    py_path: &str,
    uv_python_install_dir: &str,
    nunchaku_node: &Path,
) -> Result<(), String> {
    let req = nunchaku_node.join("requirements.txt");
    if req.exists() {
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &["install", "-r", &req.to_string_lossy()],
            Some(root),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
    }
    // ComfyUI-nunchaku imports these directly for multiple nodes (Flux/IPAdapter/PuLID).
    run_uv_pip_strict(
        uv_bin,
        py_path,
        &["install", "--upgrade", "accelerate", "diffusers"],
        Some(root),
        &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
    )?;
    if !python_module_importable(root, "accelerate") {
        return Err("Nunchaku install incomplete: missing 'accelerate' module.".to_string());
    }
    if !python_module_importable(root, "diffusers") {
        return Err("Nunchaku install incomplete: missing 'diffusers' module.".to_string());
    }
    prewarm_matplotlib_cache(root, py_path);
    Ok(())
}

fn clone_or_update_repo(root: &Path, target_dir: &Path, repo_url: &str) -> Result<(), String> {
    if target_dir.join(".git").exists() {
        run_command(
            "git",
            &["-C", &target_dir.to_string_lossy(), "pull", "--ff-only"],
            Some(root),
        )
    } else if target_dir.exists() {
        Err(format!(
            "Path exists and is not a git repository: {}",
            target_dir.display()
        ))
    } else {
        run_command(
            "git",
            &[
                "clone",
                "--depth=1",
                repo_url,
                &target_dir.to_string_lossy(),
            ],
            Some(root),
        )
    }
}

fn run_uv_pip_strict(
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
fn profile_from_torch_env(root: &Path) -> Result<String, String> {
    let mut cmd = python_for_root(root);
    cmd.arg("-c").arg(
        "import torch; \
         v = getattr(torch, '__version__', ''); \
         c = getattr(torch.version, 'cuda', '') or getattr(torch.version, 'hip', '') or ('xpu' if hasattr(torch, 'xpu') else ''); \
         print(v); print(c)",
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

    if let Some(profile) = torch_profile_from_versions(&torch_v, &cuda_v) {
        return Ok(profile);
    }

    Err(format!(
        "Unsupported installed torch runtime combo: torch={torch_v}, runtime={cuda_v}"
    ))
}

fn discover_uv_binary() -> Option<String> {
    if command_available("uv", &["--version"]) {
        return Some("uv".to_string());
    }

    if let Ok(home) = std::env::var("HOME") {
        for candidate in [
            PathBuf::from(&home).join(".local").join("bin").join("uv"),
            PathBuf::from(&home).join(".cargo").join("bin").join("uv"),
        ] {
            if candidate.exists()
                && run_command_capture(&candidate.to_string_lossy(), &["--version"], None).is_ok()
            {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    None
}

fn resolve_uv_binary(shared_runtime_root: &Path, app: &AppHandle) -> Result<String, String> {
    if let Some(found) = discover_uv_binary() {
        return Ok(found);
    }

    let _ = shared_runtime_root;
    emit_install_event(
        app,
        "step",
        "uv not found. Installing uv runtime for current user...",
    );
    let install_cmd = "curl -LsSf https://astral.sh/uv/install.sh | sh";
    if let Err(err) = run_command("sh", &["-c", install_cmd], None) {
        return Err(format!("Failed to install uv automatically: {err}"));
    }
    if let Some(found) = discover_uv_binary() {
        return Ok(found);
    }
    Err(
        "uv install completed but executable was not found. Add ~/.local/bin to PATH and retry."
            .to_string(),
    )
}

fn torch_profile_to_packages_linux(
    profile: &str,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match profile {
        "torch271_cu128" => (
            "2.7.1",
            "0.22.1",
            "2.7.1",
            "https://download.pytorch.org/whl/cu128",
        ),
        "torch291_rocm64" => (
            "2.9.1",
            "0.24.1",
            "2.9.1",
            "https://download.pytorch.org/whl/rocm6.4",
        ),
        "torch211_rocm72" => (
            "2.11.0",
            "0.26.0",
            "2.11.0",
            "https://download.pytorch.org/whl/rocm7.2",
        ),
        "torch291_xpu" => (
            "2.9.1",
            "0.24.1",
            "2.9.1",
            "https://download.pytorch.org/whl/xpu",
        ),
        "torch291_cu130" => (
            "2.9.1",
            "0.24.1",
            "2.9.1",
            "https://download.pytorch.org/whl/cu130",
        ),
        _ => (
            "2.8.0",
            "0.23.0",
            "2.8.0",
            "https://download.pytorch.org/whl/cu128",
        ),
    }
}

fn torch_profile_from_versions(torch_v: &str, cuda_v: &str) -> Option<String> {
    let t = torch_v.trim().to_ascii_lowercase();
    let c = cuda_v.trim().to_ascii_lowercase();
    if t.starts_with("2.7") && c.starts_with("12.8") {
        return Some("torch271_cu128".to_string());
    }
    if t.starts_with("2.8") && c.starts_with("12.8") {
        return Some("torch280_cu128".to_string());
    }
    if t.starts_with("2.9") && c.starts_with("6.4") {
        return Some("torch291_rocm64".to_string());
    }
    if t.starts_with("2.11") && c.starts_with("7.2") {
        return Some("torch211_rocm72".to_string());
    }
    if t.starts_with("2.9") && c == "xpu" {
        return Some("torch291_xpu".to_string());
    }
    if t.starts_with("2.9") && c.starts_with("13.0") {
        return Some("torch291_cu130".to_string());
    }
    None
}

fn triton_package_for_profile_linux(profile: &str) -> &'static str {
    match profile {
        "torch271_cu128" => "triton==3.3.1",
        "torch291_cu130" => "triton<3.6",
        _ => "triton==3.4.0",
    }
}

fn triton_package_spec_for_xpu_linux(_profile: &str) -> &'static str {
    "https://download.pytorch.org/whl/triton_xpu-3.6.0-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl"
}

fn torch_profile_is_rocm(profile: &str) -> bool {
    profile.contains("_rocm")
}

fn torch_profile_is_xpu(profile: &str) -> bool {
    profile.contains("_xpu")
}

fn enforce_torch_profile_linux(
    uv_bin: &str,
    py_path: &str,
    root: &Path,
    profile: &str,
    uv_python_install_dir: &str,
) -> Result<(), String> {
    let (torch_v, tv_v, ta_v, index_url) = torch_profile_to_packages_linux(profile);
    run_uv_pip_strict(
        uv_bin,
        py_path,
        &[
            "install",
            "--upgrade",
            "--reinstall",
            &format!("torch=={torch_v}"),
            &format!("torchvision=={tv_v}"),
            &format!("torchaudio=={ta_v}"),
            "--index-url",
            index_url,
        ],
        Some(root),
        &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
    )?;
    if torch_profile_is_xpu(profile) {
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &[
                "install",
                "--upgrade",
                "--reinstall",
                triton_package_spec_for_xpu_linux(profile),
            ],
            Some(root),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
    } else if !torch_profile_is_rocm(profile) {
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &[
                "install",
                "--upgrade",
                "--reinstall",
                triton_package_for_profile_linux(profile),
            ],
            Some(root),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
    }
    let mut verify_cmd = std::process::Command::new(py_path);
    verify_cmd.arg("-c").arg(
        "import torch, importlib.metadata as m; \
         print(getattr(torch, '__version__', '')); \
         print(getattr(torch.version, 'cuda', '') or getattr(torch.version, 'hip', '') or ('xpu' if hasattr(torch, 'xpu') else '')); \
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
    let installed_tv = lines.next().unwrap_or_default();
    let installed_ta = lines.next().unwrap_or_default();
    let actual_profile = torch_profile_from_versions(installed_torch, installed_cuda);
    if actual_profile.as_deref() != Some(profile) {
        return Err(format!(
            "Torch profile enforce mismatch for {profile}: got torch={installed_torch}, cuda={installed_cuda}, torchvision={installed_tv}, torchaudio={installed_ta}"
        ));
    }
    Ok(())
}

fn infer_torch_profile_from_installed_packages(root: &Path) -> Option<String> {
    let mut cmd = python_for_root(root);
    cmd.arg("-c").arg(
        "import importlib.metadata as m, torch; \
         ta = m.version('torchaudio') if m else ''; \
         c = getattr(torch.version, 'cuda', '') or getattr(torch.version, 'hip', '') or ('xpu' if hasattr(torch, 'xpu') else ''); \
         print(ta); print(c)",
    );
    cmd.current_dir(root);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let ta_v = lines.next().unwrap_or_default().to_ascii_lowercase();
    let cuda_v = lines.next().unwrap_or_default().to_ascii_lowercase();
    if ta_v.starts_with("2.7") && cuda_v.starts_with("12.8") {
        return Some("torch271_cu128".to_string());
    }
    if ta_v.starts_with("2.8") && cuda_v.starts_with("12.8") {
        return Some("torch280_cu128".to_string());
    }
    if ta_v.starts_with("2.9") && cuda_v.starts_with("6.4") {
        return Some("torch291_rocm64".to_string());
    }
    if ta_v.starts_with("2.11") && cuda_v.starts_with("7.2") {
        return Some("torch211_rocm72".to_string());
    }
    if ta_v.starts_with("2.9") && cuda_v == "xpu" {
        return Some("torch291_xpu".to_string());
    }
    if ta_v.starts_with("2.9") && cuda_v.starts_with("13.0") {
        return Some("torch291_cu130".to_string());
    }
    None
}

fn detect_torch_profile_for_root(root: &Path) -> Option<String> {
    profile_from_torch_env(root)
        .or_else(|_| {
            infer_torch_profile_from_installed_packages(root)
                .ok_or_else(|| "no profile hint".to_string())
        })
        .ok()
}

fn resolve_desired_torch_profile(settings: &AppSettings, root: &Path) -> String {
    profile_from_torch_env(root)
        .or_else(|_| {
            infer_torch_profile_from_installed_packages(root)
                .ok_or_else(|| "no profile hint".to_string())
        })
        .or_else(|_| {
            settings
                .comfyui_torch_profile
                .clone()
                .ok_or_else(|| "no saved profile".to_string())
        })
        .unwrap_or_else(|_| get_comfyui_install_recommendation(None).torch_profile)
}

fn install_custom_node(
    app: &AppHandle,
    install_root: &Path,
    custom_nodes_root: &Path,
    py_exe: &Path,
    repo_url: &str,
    folder_name: &str,
) -> Result<(), String> {
    emit_install_event(
        app,
        "step",
        &format!("Installing custom node: {folder_name}..."),
    );
    let node_dir = custom_nodes_root.join(folder_name);
    if node_dir.exists() {
        let _ = std::fs::remove_dir_all(&node_dir);
    }
    run_command_with_retry(
        "git",
        &["clone", repo_url, &node_dir.to_string_lossy()],
        Some(install_root),
        2,
    )?;

    let req = node_dir.join("requirements.txt");
    if req.exists() {
        let non_empty = std::fs::metadata(&req)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
        if non_empty {
            let shared_runtime_root = app
                .state::<AppState>()
                .context
                .config
                .cache_path()
                .join("comfyui-runtime");
            let uv_bin = resolve_uv_binary(&shared_runtime_root, app)?;
            let uv_python_install_dir = shared_runtime_root
                .join(".python")
                .to_string_lossy()
                .to_string();
            run_uv_pip_strict(
                &uv_bin,
                &py_exe.to_string_lossy(),
                &[
                    "install",
                    "-r",
                    &req.to_string_lossy(),
                    "--no-cache-dir",
                    "--timeout=1000",
                    "--retries",
                    "10",
                ],
                Some(install_root),
                &[("UV_PYTHON_INSTALL_DIR", &uv_python_install_dir)],
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

/// The app's built-in custom nodes (Linux). See [`CustomNodeSpec`]'s doc
/// comment for why this table exists; referenced by `flag_key` from the
/// fresh-install flow (`run_comfyui_install_linux`), the addon-state check
/// (`get_comfyui_addon_state`), and the enable/disable toggle
/// (`apply_comfyui_component_toggle`) instead of each hand-copying the repo
/// URL/folder name.
const CUSTOM_NODES: &[CustomNodeSpec] = &[
    CustomNodeSpec {
        flag_key: "node_comfyui_manager",
        display_name: "ComfyUI-Manager",
        repo_url: "https://github.com/Comfy-Org/ComfyUI-Manager",
        install_folder_name: "ComfyUI-Manager",
        known_folder_names: &["ComfyUI-Manager", "comfyui-manager"],
    },
    CustomNodeSpec {
        flag_key: "node_comfyui_easy_use",
        display_name: "ComfyUI-Easy-Use",
        repo_url: "https://github.com/yolain/ComfyUI-Easy-Use",
        install_folder_name: "ComfyUI-Easy-Use",
        known_folder_names: &["ComfyUI-Easy-Use"],
    },
    CustomNodeSpec {
        flag_key: "node_rgthree_comfy",
        display_name: "rgthree-comfy",
        repo_url: "https://github.com/rgthree/rgthree-comfy",
        install_folder_name: "rgthree-comfy",
        known_folder_names: &["rgthree-comfy"],
    },
    CustomNodeSpec {
        flag_key: "node_comfyui_gguf",
        display_name: "ComfyUI-GGUF",
        repo_url: "https://github.com/city96/ComfyUI-GGUF",
        install_folder_name: "ComfyUI-GGUF",
        known_folder_names: &["ComfyUI-GGUF"],
    },
    CustomNodeSpec {
        flag_key: "node_comfyui_kjnodes",
        display_name: "comfyui-kjnodes",
        repo_url: "https://github.com/kijai/ComfyUI-KJNodes",
        install_folder_name: "comfyui-kjnodes",
        known_folder_names: &["comfyui-kjnodes", "ComfyUI-KJNodes"],
    },
    CustomNodeSpec {
        flag_key: "node_comfyui_crystools",
        display_name: "comfyui-crystools",
        repo_url: "https://github.com/crystian/comfyui-crystools.git",
        install_folder_name: "comfyui-crystools",
        known_folder_names: &["comfyui-crystools", "ComfyUI-Crystools"],
    },
];

fn custom_node_spec(flag_key: &str) -> Option<&'static CustomNodeSpec> {
    CUSTOM_NODES.iter().find(|spec| spec.flag_key == flag_key)
}

fn selected_attention_backend(request: &ComfyInstallRequest) -> &'static str {
    if request.include_flash_attention {
        "flash"
    } else if request.include_sage_attention3 {
        "sage3"
    } else if request.include_sage_attention {
        "sage"
    } else if request.include_nunchaku {
        "nunchaku"
    } else {
        "none"
    }
}

fn detect_launch_attention_backend_for_root(root: &Path) -> Option<String> {
    if python_module_importable(root, "flash_attn") {
        return Some("flash".to_string());
    }
    if python_module_importable(root, "sageattn3") {
        return Some("sage3".to_string());
    }
    if python_module_importable(root, "sageattention") {
        return Some("sage".to_string());
    }
    let has_nunchaku = python_module_importable(root, "nunchaku")
        || custom_node_exists(root, "nunchaku_nodes")
        || custom_node_exists(root, "ComfyUI-nunchaku")
        || pip_has_package(root, "nunchaku");
    if has_nunchaku {
        return Some("nunchaku".to_string());
    }
    None
}

fn nunchaku_backend_present(root: &Path) -> bool {
    python_module_importable(root, "nunchaku")
        || pip_has_package(root, "nunchaku")
        || custom_node_exists(root, "nunchaku_nodes")
        || custom_node_exists(root, "ComfyUI-nunchaku")
}

fn collect_cuda_runtime_library_paths(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push_unique = |p: PathBuf| {
        if p.exists() && !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    };

    for sys in [
        "/opt/cuda/lib64",
        "/usr/local/cuda/lib64",
        "/usr/lib/wsl/lib",
    ] {
        push_unique(PathBuf::from(sys));
    }

    for env_key in ["CUDA_PATH", "CUDA_HOME"] {
        if let Some(base) = std::env::var_os(env_key) {
            push_unique(PathBuf::from(base).join("lib64"));
        }
    }

    for venv_name in [".venv", "venv"] {
        let venv_lib = root.join(venv_name).join("lib");
        let py_dirs = std::fs::read_dir(&venv_lib)
            .ok()
            .into_iter()
            .flat_map(|iter| iter.flatten().map(|e| e.path()).collect::<Vec<_>>());
        for py_dir in py_dirs {
            let site = py_dir.join("site-packages").join("nvidia");
            for pkg in ["cuda_runtime", "cublas", "cudnn", "cusolver", "cusparse"] {
                push_unique(site.join(pkg).join("lib"));
            }
        }
    }
    dirs
}

fn apply_cuda_runtime_env_for_root(cmd: &mut std::process::Command, root: &Path) {
    let mut paths = collect_cuda_runtime_library_paths(root);
    if paths.is_empty() {
        return;
    }
    if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
        for p in std::env::split_paths(&existing) {
            if !paths.iter().any(|d| d == &p) {
                paths.push(p);
            }
        }
    }
    if let Ok(joined) = std::env::join_paths(paths) {
        cmd.env("LD_LIBRARY_PATH", joined);
    }
}

fn python_runtime_env_for_root(root: &Path) -> Vec<(String, String)> {
    let mut envs: Vec<(String, String)> = Vec::new();
    let mpl_cache = root.join(".venv").join("var").join("matplotlib");
    let _ = std::fs::create_dir_all(&mpl_cache);
    let mut paths = collect_cuda_runtime_library_paths(root);
    if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
        for p in std::env::split_paths(&existing) {
            if !paths.iter().any(|d| d == &p) {
                paths.push(p);
            }
        }
    }
    if !paths.is_empty() {
        if let Ok(joined) = std::env::join_paths(paths) {
            envs.push((
                "LD_LIBRARY_PATH".to_string(),
                joined.to_string_lossy().to_string(),
            ));
        }
    }

    if let Ok(value) = std::env::var("PYTORCH_CUDA_ALLOC_CONF") {
        if std::env::var_os("PYTORCH_ALLOC_CONF").is_none() {
            envs.push(("PYTORCH_ALLOC_CONF".to_string(), value));
        }
    }
    envs.push(("MPLBACKEND".to_string(), "Agg".to_string()));
    envs.push((
        "MPLCONFIGDIR".to_string(),
        mpl_cache.to_string_lossy().to_string(),
    ));
    envs
}

fn configure_python_runtime_env_for_root(cmd: &mut std::process::Command, root: &Path) {
    let mpl_cache = root.join(".venv").join("var").join("matplotlib");
    let _ = std::fs::create_dir_all(&mpl_cache);
    apply_torch_allocator_env_compat(cmd);
    cmd.env("MPLBACKEND", "Agg");
    cmd.env("MPLCONFIGDIR", mpl_cache.to_string_lossy().to_string());
}

fn prewarm_matplotlib_cache(root: &Path, py_path: &str) {
    let mpl_cache = root.join(".venv").join("var").join("matplotlib");
    let _ = std::fs::create_dir_all(&mpl_cache);
    let code = format!(
        "import os, logging; \
os.environ.setdefault('MPLBACKEND', 'Agg'); \
os.environ['MPLCONFIGDIR'] = r'''{}'''; \
logging.getLogger('matplotlib.font_manager').setLevel(logging.ERROR); \
import matplotlib; matplotlib.use('Agg', force=True); \
from matplotlib import font_manager as fm; \
fm._load_fontmanager(try_read_cache=False)",
        mpl_cache.to_string_lossy()
    );
    let _ = run_command_capture(py_path, &["-c", &code], Some(root));
}

fn python_module_importable(root: &Path, module: &str) -> bool {
    let mut cmd = python_for_root(root);
    cmd.arg("-c").arg(format!("import {module}"));
    cmd.current_dir(root);
    apply_cuda_runtime_env_for_root(&mut cmd, root);
    configure_python_runtime_env_for_root(&mut cmd, root);
    cmd.output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
fn comfyui_launch_args(
    listen_enabled: bool,
    pinned_memory_enabled: bool,
    attention_backend: Option<&str>,
    lowvram_enabled: bool,
    bf16_unet_enabled: bool,
    async_offload_enabled: bool,
    disable_smart_memory_enabled: bool,
    custom_launch_args: &[String],
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if listen_enabled {
        args.push("--listen".to_string());
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
    if !pinned_memory_enabled {
        args.push("--disable-pinned-memory".to_string());
    }
    append_attention_launch_arg(&mut args, attention_backend);
    args.extend(custom_launch_args.iter().cloned());
    args
}

fn run_comfyui_install(
    app: &AppHandle,
    request: &ComfyInstallRequest,
    shared_runtime_root: &Path,
    cancel: &CancellationToken,
) -> Result<PathBuf, String> {
    run_comfyui_install_linux(app, request, shared_runtime_root, cancel)
}
fn run_comfyui_install_linux(
    app: &AppHandle,
    request: &ComfyInstallRequest,
    shared_runtime_root: &Path,
    cancel: &CancellationToken,
) -> Result<PathBuf, String> {
    let mut summary: Vec<InstallSummaryItem> = Vec::new();
    let include_insight_face = request.include_insight_face || request.include_nunchaku;
    let selected_attention = [
        request.include_sage_attention,
        request.include_sage_attention3,
        request.include_flash_attention,
        request.include_nunchaku,
    ]
    .into_iter()
    .filter(|v| *v)
    .count();
    if selected_attention > 1 {
        return Err(
            "Choose only one of SageAttention, SageAttention3, FlashAttention, or Nunchaku."
                .to_string(),
        );
    }

    if cancel.is_cancelled() {
        return Err("Installation cancelled.".to_string());
    }

    let base_root = normalize_path(&request.install_root)?;
    let extra_model_root = normalize_optional_path(request.extra_model_root.as_deref())?;
    let selected_comfy_root = path_name_is_comfyui(&base_root);
    let comfy_dir = if selected_comfy_root {
        base_root.clone()
    } else {
        choose_install_folder(&base_root, request.force_fresh)
    };
    let install_root = comfy_dir.clone();

    std::fs::create_dir_all(&install_root).map_err(|err| err.to_string())?;
    write_install_state(&install_root, "in_progress", "init");
    emit_install_event(
        app,
        "info",
        &format!("Install folder selected: {}", install_root.display()),
    );

    let mut scan = get_linux_prereq_cache_or_scan()?;
    let distro = scan.distro.clone();
    emit_install_event(
        app,
        "step",
        &format!("Detected Linux distribution family: {distro}."),
    );
    write_install_state(&install_root, "in_progress", "linux_packages");
    if scan.missing_required.is_empty() && scan.missing_optional.is_empty() {
        emit_install_event(app, "info", "Linux system prerequisites already installed.");
    } else {
        emit_install_event(
            app,
            "step",
            &format!(
                "Installing missing Linux prerequisites for {}...",
                scan.distro
            ),
        );
        install_missing_linux_prereqs(&scan)?;
        scan = refresh_linux_prereq_cache()?;
        if !scan.missing_required.is_empty() {
            return Err(format!(
                "Required Linux packages are still missing after install attempt: {}",
                scan.missing_required.join(", ")
            ));
        }
    }

    ensure_git_available(app)?;
    if !comfy_dir.join("main.py").exists() {
        write_install_state(&install_root, "in_progress", "clone_comfyui");
        emit_install_event(app, "step", "Cloning ComfyUI...");
        if comfy_dir.exists() && !is_empty_dir(&comfy_dir) {
            if is_recoverable_preclone_dir(&comfy_dir) {
                clear_directory_contents(&comfy_dir)?;
            } else {
                return Err(format!(
                    "Selected ComfyUI folder already exists and is not empty: {}. Choose a new base folder or remove existing files.",
                    comfy_dir.display()
                ));
            }
        }
        run_command_with_retry(
            "git",
            &[
                "clone",
                "https://github.com/comfyanonymous/ComfyUI.git",
                &comfy_dir.to_string_lossy(),
            ],
            Some(&install_root),
            2,
        )?;
        // Pin fresh installs to latest release tag so users do not see an
        // immediate update prompt after a clean install.
        if let Some((latest_tag, latest_version)) = git_latest_release_tag(&comfy_dir) {
            if let Err(err) = run_command_with_retry(
                "git",
                &["checkout", "-B", "master", &latest_tag],
                Some(&comfy_dir),
                1,
            ) {
                emit_install_event(
                    app,
                    "warn",
                    &format!(
                        "ComfyUI cloned, but failed to pin to release tag {} (v{}): {}",
                        latest_tag, latest_version, err
                    ),
                );
            } else {
                emit_install_event(
                    app,
                    "info",
                    &format!(
                        "Pinned fresh ComfyUI install to latest release tag {} (v{}).",
                        latest_tag, latest_version
                    ),
                );
            }
        } else {
            emit_install_event(
                app,
                "warn",
                "ComfyUI cloned, but latest release tag could not be resolved during install.",
            );
        }
        summary.push(InstallSummaryItem {
            name: "ComfyUI core".to_string(),
            status: "ok".to_string(),
            detail: "ComfyUI cloned successfully.".to_string(),
        });
    } else {
        summary.push(InstallSummaryItem {
            name: "ComfyUI core".to_string(),
            status: "skipped".to_string(),
            detail: "Existing ComfyUI folder reused.".to_string(),
        });
    }

    if let Some(extra_root) = extra_model_root.as_ref() {
        write_install_state(&install_root, "in_progress", "extra_model_paths");
        emit_install_event(
            app,
            "step",
            &format!(
                "Configuring ComfyUI extra model paths from {}...",
                extra_root.display()
            ),
        );
        let config_path =
            write_extra_model_paths_yaml(&comfy_dir, extra_root, request.extra_model_use_default)?;
        summary.push(InstallSummaryItem {
            name: "extra_model_paths".to_string(),
            status: "ok".to_string(),
            detail: format!(
                "Configured {} with base path {}.",
                config_path.display(),
                extra_root.display()
            ),
        });
    }

    if cancel.is_cancelled() {
        return Err("Installation cancelled.".to_string());
    }

    write_install_state(&install_root, "in_progress", "python_venv");
    emit_install_event(app, "step", "Preparing uv-managed Python + local .venv...");
    let uv_bin = resolve_uv_binary(shared_runtime_root, app)?;
    let python_store = shared_runtime_root.join(".python");
    std::fs::create_dir_all(&python_store).map_err(|err| err.to_string())?;
    let python_store_s = python_store.to_string_lossy().to_string();
    run_command_env(
        &uv_bin,
        &["python", "install", UV_PYTHON_VERSION],
        Some(&comfy_dir),
        &[
            ("UV_PYTHON_INSTALL_DIR", &python_store_s),
            ("UV_PYTHON_INSTALL_BIN", "false"),
        ],
    )?;

    let venv_dir = comfy_dir.join(".venv");
    let py_exe = venv_dir.join("bin").join("python");
    if !py_exe.exists() {
        let venv_s = venv_dir.to_string_lossy().to_string();
        run_command_env(
            &uv_bin,
            &["venv", "--seed", "--python", UV_PYTHON_VERSION, &venv_s],
            Some(&comfy_dir),
            &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
        )?;
    } else {
        emit_install_event(app, "step", "Existing .venv found; reusing.");
    }
    run_uv_pip_strict(
        &uv_bin,
        &py_exe.to_string_lossy(),
        &["install", "--upgrade", "pip", "setuptools", "wheel"],
        Some(&comfy_dir),
        &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
    )?;

    let recommendation = get_comfyui_install_recommendation(None);
    let selected_profile = request
        .torch_profile
        .clone()
        .unwrap_or(recommendation.torch_profile);
    let hopper_sm90 = is_nvidia_hopper_sm90();
    write_install_state(&install_root, "in_progress", "torch_stack");
    emit_install_event(app, "step", "Installing Torch stack...");
    enforce_torch_profile_linux(
        &uv_bin,
        &py_exe.to_string_lossy(),
        &comfy_dir,
        &selected_profile,
        &python_store_s,
    )?;

    write_install_state(&install_root, "in_progress", "comfy_requirements");
    run_uv_pip_strict(
        &uv_bin,
        &py_exe.to_string_lossy(),
        &[
            "install",
            "-r",
            &comfy_dir.join("requirements.txt").to_string_lossy(),
        ],
        Some(&comfy_dir),
        &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
    )?;
    // Re-apply selected torch stack because requirements can drift torch/torchvision.
    enforce_torch_profile_linux(
        &uv_bin,
        &py_exe.to_string_lossy(),
        &comfy_dir,
        &selected_profile,
        &python_store_s,
    )?;
    run_uv_pip_strict(
        &uv_bin,
        &py_exe.to_string_lossy(),
        &["install", "--upgrade", "pyyaml", "nvidia-ml-py"],
        Some(&comfy_dir),
        &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
    )?;

    let addon_root = comfy_dir.join("custom_nodes");
    std::fs::create_dir_all(&addon_root).map_err(|err| err.to_string())?;

    if request.include_sage_attention {
        write_install_state(&install_root, "in_progress", "addon_sageattention");
        emit_install_event(app, "step", "Installing SageAttention...");
        install_sageattention_linux(
            &comfy_dir,
            &py_exe.to_string_lossy(),
            &selected_profile,
            hopper_sm90,
        )?;
    }
    if include_insight_face {
        write_install_state(&install_root, "in_progress", "addon_insightface");
        if request.include_nunchaku && !request.include_insight_face {
            emit_install_event(
                app,
                "step",
                "Installing InsightFace (required by Nunchaku)...",
            );
        } else {
            emit_install_event(app, "step", "Installing InsightFace...");
        }
        install_insightface(
            &comfy_dir,
            &uv_bin,
            &py_exe.to_string_lossy(),
            &python_store_s,
        )?;
    }

    if request.include_flash_attention {
        write_install_state(&install_root, "in_progress", "addon_flashattention");
        emit_install_event(app, "step", "Installing FlashAttention...");
        install_flashattention_linux(
            &comfy_dir,
            &py_exe.to_string_lossy(),
            &selected_profile,
            hopper_sm90,
        )?;
        summary.push(InstallSummaryItem {
            name: "flash-attention".to_string(),
            status: "ok".to_string(),
            detail: "Installed using Linux wheel stack.".to_string(),
        });
    }
    if request.include_sage_attention3 {
        write_install_state(&install_root, "in_progress", "addon_sageattention3");
        emit_install_event(app, "step", "Installing SageAttention3...");
        install_linux_wheel_for_profile(
            &comfy_dir,
            &py_exe.to_string_lossy(),
            &selected_profile,
            "sage3",
            hopper_sm90,
            true,
        )?;
        // Keep sageattention installed for ComfyUI --use-sage-attention compatibility checks.
        install_sageattention_linux(
            &comfy_dir,
            &py_exe.to_string_lossy(),
            &selected_profile,
            hopper_sm90,
        )?;
        summary.push(InstallSummaryItem {
            name: "sageattention3".to_string(),
            status: "ok".to_string(),
            detail: "Installed using Linux wheel stack.".to_string(),
        });
    }
    if request.include_nunchaku {
        write_install_state(&install_root, "in_progress", "addon_nunchaku");
        emit_install_event(app, "step", "Installing Nunchaku...");
        ensure_git_available(app)?;
        std::fs::create_dir_all(&addon_root).map_err(|err| err.to_string())?;
        let nunchaku_node = addon_root.join("ComfyUI-nunchaku");
        for folder in ["ComfyUI-nunchaku", "nunchaku_nodes"] {
            let path = addon_root.join(folder);
            if path.exists() {
                let _ = std::fs::remove_dir_all(path);
            }
        }
        clone_or_update_repo(
            &comfy_dir,
            &nunchaku_node,
            "https://github.com/nunchaku-ai/ComfyUI-nunchaku",
        )?;
        let versions_json = nunchaku_node.join("nunchaku_versions.json");
        let _ = download_http_file(
            "https://nunchaku.tech/cdn/nunchaku_versions.json",
            &versions_json,
        );
        install_nunchaku_node_requirements(
            &comfy_dir,
            &uv_bin,
            &py_exe.to_string_lossy(),
            &python_store_s,
            &nunchaku_node,
        )?;
        install_linux_wheel_for_profile(
            &comfy_dir,
            &py_exe.to_string_lossy(),
            &selected_profile,
            "nunchaku",
            hopper_sm90,
            true,
        )?;
        if !nunchaku_backend_present(&comfy_dir) {
            return Err(
                "Nunchaku install incomplete: module or custom node not detected after install."
                    .to_string(),
            );
        }
        summary.push(InstallSummaryItem {
            name: "nunchaku".to_string(),
            status: "ok".to_string(),
            detail: "Installed Linux nunchaku wheel and ComfyUI-nunchaku node.".to_string(),
        });
    }
    if request.include_trellis2 {
        write_install_state(&install_root, "in_progress", "addon_trellis2");
        emit_install_event(app, "step", "Installing Trellis2...");
        let custom_nodes_dir = comfy_dir.join("custom_nodes");
        std::fs::create_dir_all(&custom_nodes_dir).map_err(|err| err.to_string())?;
        let trellis_dir = custom_nodes_dir.join("ComfyUI-TRELLIS2");
        clone_or_update_repo(
            &comfy_dir,
            &trellis_dir,
            "https://github.com/ArcticLatent/ComfyUI-TRELLIS2",
        )?;
        let trellis_req = trellis_dir.join("requirements.txt");
        if trellis_req.exists() {
            run_uv_pip_strict(
                &uv_bin,
                &py_exe.to_string_lossy(),
                &["install", "-r", &trellis_req.to_string_lossy()],
                Some(&comfy_dir),
                &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
            )?;
        }

        let geometry_dir = custom_nodes_dir.join("ComfyUI-GeometryPack");
        clone_or_update_repo(
            &comfy_dir,
            &geometry_dir,
            "https://github.com/PozzettiAndrea/ComfyUI-GeometryPack",
        )?;
        let geometry_req = geometry_dir.join("requirements.txt");
        if geometry_req.exists() {
            run_uv_pip_strict(
                &uv_bin,
                &py_exe.to_string_lossy(),
                &["install", "-r", &geometry_req.to_string_lossy()],
                Some(&comfy_dir),
                &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
            )?;
        }
        run_uv_pip_strict(
            &uv_bin,
            &py_exe.to_string_lossy(),
            &["install", "--upgrade", "tomli"],
            Some(&comfy_dir),
            &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
        )?;

        let ultrashape_dir = custom_nodes_dir.join("ComfyUI-UltraShape1");
        clone_or_update_repo(
            &comfy_dir,
            &ultrashape_dir,
            "https://github.com/jtydhr88/ComfyUI-UltraShape1",
        )?;
        let ultrashape_req = ultrashape_dir.join("requirements.txt");
        if ultrashape_req.exists() {
            run_uv_pip_strict(
                &uv_bin,
                &py_exe.to_string_lossy(),
                &["install", "-r", &ultrashape_req.to_string_lossy()],
                Some(&ultrashape_dir),
                &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
            )?;
            run_uv_pip_strict(
                &uv_bin,
                &py_exe.to_string_lossy(),
                &["install", "-U", "accelerate"],
                Some(&ultrashape_dir),
                &[("UV_PYTHON_INSTALL_DIR", &python_store_s)],
            )?;
        }

        let ultrashape_models_dir = comfy_dir.join("models").join("UltraShape");
        std::fs::create_dir_all(&ultrashape_models_dir).map_err(|err| err.to_string())?;
        let ultrashape_model_file = ultrashape_models_dir.join("ultrashape_v1.pt");
        if !ultrashape_model_file.exists() {
            download_http_file(
                "https://huggingface.co/infinith/UltraShape/resolve/main/ultrashape_v1.pt",
                &ultrashape_model_file,
            )?;
        }
        summary.push(InstallSummaryItem {
            name: "trellis2".to_string(),
            status: "ok".to_string(),
            detail: "Installed TRELLIS2 + GeometryPack + UltraShape1 Linux flow.".to_string(),
        });
    }

    let requested_custom_nodes: [(bool, &CustomNodeSpec); 6] = [
        (request.node_comfyui_manager, &CUSTOM_NODES[0]),
        (request.node_comfyui_easy_use, &CUSTOM_NODES[1]),
        (request.node_rgthree_comfy, &CUSTOM_NODES[2]),
        (request.node_comfyui_gguf, &CUSTOM_NODES[3]),
        (request.node_comfyui_kjnodes, &CUSTOM_NODES[4]),
        (request.node_comfyui_crystools, &CUSTOM_NODES[5]),
    ];
    for (requested, spec) in requested_custom_nodes {
        if !requested {
            continue;
        }
        install_custom_node_and_record(app, &install_root, spec, &mut summary, || {
            install_custom_node(
                app,
                &comfy_dir,
                &addon_root,
                &py_exe,
                spec.repo_url,
                spec.install_folder_name,
            )
        });
    }

    // Final guard: custom-node requirements can drift torch deps.
    // Re-assert the selected stack before first launch.
    write_install_state(&install_root, "in_progress", "finalize_torch_stack");
    emit_install_event(
        app,
        "step",
        "Finalizing Torch stack for selected profile...",
    );
    enforce_torch_profile_linux(
        &uv_bin,
        &py_exe.to_string_lossy(),
        &comfy_dir,
        &selected_profile,
        &python_store_s,
    )?;

    write_install_summary(&install_root, &summary);
    write_install_state(&install_root, "completed", "done");
    Ok(comfy_dir)
}

#[tauri::command]
async fn start_comfyui_install(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ComfyInstallRequest,
) -> Result<(), String> {
    {
        let mut active = recover_lock(state.install_cancel.lock());
        if active.is_some() {
            return Err("ComfyUI installation is already active.".to_string());
        }
        *active = Some(CancellationToken::new());
    }

    let cancel = recover_lock(state.install_cancel.lock())
        .as_ref()
        .cloned()
        .ok_or_else(|| "Failed to initialize install cancellation token.".to_string())?;
    let shared_runtime_root = state.context.config.cache_path().join("comfyui-runtime");

    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_comfyui_install(&app_for_task, &request, &shared_runtime_root, &cancel);
        match result {
            Ok(comfy_root) => {
                let install_dir = comfy_root
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| comfy_root.clone());
                let managed = app_for_task.state::<AppState>();
                let normalized_shared_models =
                    normalize_optional_path(request.extra_model_root.as_deref())
                        .ok()
                        .flatten();
                let _ =
                    managed.context.config.update_settings(|settings| {
                        settings.comfyui_root = Some(comfy_root.clone());
                        settings.comfyui_last_install_dir = Some(install_dir.clone());
                        settings.comfyui_pinned_memory_enabled = request.include_pinned_memory;
                        settings.comfyui_torch_profile =
                            Some(request.torch_profile.clone().unwrap_or_else(|| {
                                get_comfyui_install_recommendation(None).torch_profile
                            }));
                        settings.comfyui_attention_backend =
                            Some(selected_attention_backend(&request).to_string());
                        settings.shared_models_root = normalized_shared_models.clone();
                        settings.shared_models_use_default = normalized_shared_models
                            .as_ref()
                            .is_some_and(|_| request.extra_model_use_default);
                    });
                let _ = app_for_task.emit(
                    "comfyui-install-progress",
                    DownloadProgressEvent {
                        kind: "comfyui_install".to_string(),
                        phase: "finished".to_string(),
                        artifact: Some(install_dir.to_string_lossy().to_string()),
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
        *recover_lock(managed.install_cancel.lock()) = None;
    });

    Ok(())
}

#[tauri::command]
fn set_comfyui_root(
    state: State<'_, AppState>,
    comfyui_root: String,
) -> Result<AppSettings, String> {
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
        Some(normalize_canonical_path(
            &std::fs::canonicalize(&path).unwrap_or(path),
        ))
    };
    if let Some(resolved) = normalized.as_ref() {
        if is_forbidden_install_path(resolved) {
            return Err(
                "That folder can't be used as a ComfyUI root (system directory).".to_string(),
            );
        }
    }
    let detected_attention = normalized.as_ref().map(|root| {
        detect_launch_attention_backend_for_root(root).unwrap_or_else(|| "none".to_string())
    });
    let detected_profile = normalized
        .as_ref()
        .and_then(|root| detect_torch_profile_for_root(root));

    state
        .context
        .config
        .update_settings(|settings| {
            settings.comfyui_root = normalized.clone();
            settings.comfyui_attention_backend = detected_attention
                .clone()
                .or_else(|| Some("none".to_string()));
            settings.comfyui_torch_profile = detected_profile.clone();
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
        let resolved = normalize_canonical_path(&std::fs::canonicalize(&path).unwrap_or(path));
        if is_forbidden_install_path(&resolved) {
            return Err("Install base folder is blocked. Avoid system directories.".to_string());
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
async fn check_updates_now(state: State<'_, AppState>) -> Result<UpdateCheckResponse, String> {
    if let Some(manager) = external_package_manager() {
        return Ok(UpdateCheckResponse {
            available: false,
            version: None,
            notes: Some(format!(
                "Updates are managed by {manager}. Update this application through your package manager."
            )),
        });
    }

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
    if let Some(manager) = external_package_manager() {
        return Ok(UpdateCheckResponse {
            available: false,
            version: None,
            notes: Some(format!(
                "Updates are managed by {manager}. Update this application through your package manager."
            )),
        });
    }

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
async fn download_lora_asset(
    app: AppHandle,
    state: State<'_, AppState>,
    lora_id: String,
    token: Option<String>,
    comfyui_root: Option<String>,
) -> Result<(), String> {
    let root = resolve_root_path(&state.context, comfyui_root)?;
    let effective_root = effective_download_root(&root);
    let lora = state
        .context
        .catalog
        .find_lora(&lora_id)
        .ok_or_else(|| "Selected LoRA was not found in catalog.".to_string())?;

    let cancel = CancellationToken::new();
    {
        let mut active = recover_lock(state.active_cancel.lock());
        if active.is_some() {
            return Err("A download is already active. Cancel it first.".to_string());
        }
        *active = Some(cancel.clone());
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let handle = state.context.downloads.download_lora_with_cancel(
        effective_root,
        lora,
        token,
        tx,
        Some(cancel),
    );
    *recover_lock(state.active_abort.lock()) = Some(handle.abort_handle());
    spawn_progress_emitter(app.clone(), "lora".to_string(), rx);
    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = handle.await;
        let managed = app_for_task.state::<AppState>();
        *recover_lock(managed.active_cancel.lock()) = None;
        *recover_lock(managed.active_abort.lock()) = None;

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



pub(crate) fn resolve_root_path(
    context: &AppContext,
    comfyui_root: Option<String>,
) -> Result<std::path::PathBuf, String> {
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
        let canonical = std::fs::canonicalize(&absolute).ok().or(Some(absolute))?;
        Some(normalize_canonical_path(&canonical).to_path_buf())
    }

    if let Some(root) = comfyui_root {
        let trimmed = root.trim();
        if !trimmed.is_empty() {
            let path = std::path::PathBuf::from(trimmed);
            if let Some(normalized) = normalize_existing(path) {
                // `comfyui_root` here is caller-supplied (any invoke from the
                // webview can pass an arbitrary existing path, not just the
                // app's own configured root), and this function feeds
                // filesystem-mutating commands (model/workflow downloads,
                // extra_model_paths.yaml writes). Refuse system directories
                // even though the path exists, same as the install-time
                // guard in `is_forbidden_install_path`.
                if is_forbidden_install_path(&normalized) {
                    return Err(
                        "That folder can't be used as a ComfyUI root (system directory)."
                            .to_string(),
                    );
                }
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

pub(crate) fn effective_download_root(root: &Path) -> PathBuf {
    match comfy_extra_model_config(root) {
        Some(config) if config.is_default => {
            log::info!(
                "Using extra model base path for downloads: {}",
                config.base_path.display()
            );
            config.base_path
        }
        _ => root.to_path_buf(),
    }
}

pub(crate) fn comfy_extra_model_config(comfy_root: &Path) -> Option<ComfyExtraModelConfig> {
    let path = comfy_root.join("extra_model_paths.yaml");
    let content = std::fs::read_to_string(path).ok()?;
    let mut in_comfyui = false;
    let mut base_path: Option<PathBuf> = None;
    let mut is_default = false;

    for line in content.lines() {
        let without_comment = line.split('#').next().unwrap_or_default();
        if without_comment.trim().is_empty() {
            continue;
        }

        let indent = without_comment
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();
        let trimmed = without_comment.trim();

        if trimmed == "comfyui:" {
            in_comfyui = true;
            continue;
        }

        if in_comfyui {
            if let Some(raw) = trimmed.strip_prefix("base_path:") {
                let scalar = parse_yaml_scalar(raw);
                if !scalar.trim().is_empty() {
                    let parsed = PathBuf::from(scalar.trim());
                    let resolved = if parsed.is_absolute() {
                        parsed
                    } else {
                        comfy_root.join(parsed)
                    };
                    base_path = Some(normalize_canonical_path(&resolved));
                }
                continue;
            }

            if let Some(raw) = trimmed.strip_prefix("is_default:") {
                let scalar = parse_yaml_scalar(raw);
                if let Some(parsed) = parse_yaml_bool(&scalar) {
                    is_default = parsed;
                }
                continue;
            }
        }

        if indent == 0 && trimmed.ends_with(':') {
            in_comfyui = false;
            continue;
        }

        if !in_comfyui {
            continue;
        }

        if let Some(raw) = trimmed.strip_prefix("base_path:") {
            let scalar = parse_yaml_scalar(raw);
            if scalar.trim().is_empty() {
                continue;
            }
            let parsed = PathBuf::from(scalar.trim());
            let resolved = if parsed.is_absolute() {
                parsed
            } else {
                comfy_root.join(parsed)
            };
            base_path = Some(normalize_canonical_path(&resolved));
            continue;
        }
    }

    base_path.map(|base| ComfyExtraModelConfig {
        base_path: base,
        is_default,
    })
}


#[tauri::command]
fn inspect_comfyui_path(path: String) -> Result<ComfyPathInspection, String> {
    let selected = path.trim();
    if selected.is_empty() {
        return Err("Folder is empty.".to_string());
    }
    let selected_path = PathBuf::from(selected);
    if !selected_path.exists() || !selected_path.is_dir() {
        return Err("Folder does not exist.".to_string());
    }
    let normalized = std::fs::canonicalize(&selected_path).unwrap_or(selected_path.clone());
    let normalized = normalize_canonical_path(&normalized).to_path_buf();
    let detected_root = detect_existing_comfyui_root(&normalized)
        .map(|p| normalize_canonical_path(&p).to_string_lossy().to_string());
    Ok(ComfyPathInspection {
        selected: normalize_canonical_path(&normalized)
            .to_string_lossy()
            .to_string(),
        detected_root,
    })
}

#[tauri::command]
fn list_comfyui_installations(
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

    let base = normalize_canonical_path(&base).to_path_buf();
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
        let root = normalize_canonical_path(&base)
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
            let root = normalize_canonical_path(&path)
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

    open::that(target).map_err(|err| format!("Failed to open folder: {err}"))?;
    Ok(path)
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err("Only http/https links are allowed.".to_string());
    }

    open::that(trimmed).map_err(|err| format!("Failed to open link: {err}"))?;
    Ok(())
}

pub(crate) fn start_comfyui_root_impl(
    app: &AppHandle,
    state: &AppState,
    comfyui_root: Option<String>,
) -> Result<(), String> {
    if comfyui_runtime_running(state) {
        return Ok(());
    }

    let root = if let Some(raw) = comfyui_root {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            state
                .context
                .config
                .settings()
                .comfyui_root
                .ok_or_else(|| "ComfyUI root is not configured.".to_string())?
        } else {
            PathBuf::from(trimmed)
        }
    } else {
        state
            .context
            .config
            .settings()
            .comfyui_root
            .ok_or_else(|| "ComfyUI root is not configured.".to_string())?
    };

    let root = normalize_canonical_path(&std::fs::canonicalize(&root).unwrap_or(root));
    let main_py = root.join("main.py");
    if !main_py.exists() {
        return Err(format!("ComfyUI main.py not found in {}", root.display()));
    }

    let py_exe = resolve_start_python_exe(app, state, &root)?;
    let settings = state.context.config.settings();
    let custom_launch_args = parse_custom_launch_args(&settings.comfyui_custom_launch_args)?;

    let configured_root_matches = settings
        .comfyui_root
        .as_ref()
        .map(|configured_root| {
            normalize_canonical_path(
                &std::fs::canonicalize(configured_root).unwrap_or_else(|_| configured_root.clone()),
            ) == root
        })
        .unwrap_or(false);

    let effective_attention = {
        let configured = if configured_root_matches {
            settings.comfyui_attention_backend.clone()
        } else {
            None
        };
        match configured.as_deref() {
            Some("none") => None,
            Some("sage3") => {
                if python_module_importable(&root, "sageattn3") {
                    Some("sage3".to_string())
                } else {
                    return Err(
                        "SageAttention3 is selected but not importable in this install. Re-apply SageAttention3 for this ComfyUI root."
                            .to_string(),
                    );
                }
            }
            Some("sage") => {
                if python_module_importable(&root, "sageattention")
                    || python_module_importable(&root, "sageattn3")
                {
                    Some("sage".to_string())
                } else {
                    return Err(
                        "SageAttention is selected but not importable in this install. Re-apply SageAttention for this ComfyUI root."
                            .to_string(),
                    );
                }
            }
            Some("flash") => {
                if python_module_importable(&root, "flash_attn") {
                    Some("flash".to_string())
                } else {
                    return Err(
                        "FlashAttention is selected but not importable in this install. Re-apply FlashAttention for this ComfyUI root."
                            .to_string(),
                    );
                }
            }
            Some("nunchaku") => {
                if nunchaku_backend_present(&root) {
                    Some("nunchaku".to_string())
                } else {
                    return Err(
                        "Nunchaku is selected but backend is not installed correctly for this ComfyUI root. Re-apply Nunchaku."
                            .to_string(),
                    );
                }
            }
            _ => detect_launch_attention_backend_for_root(&root),
        }
    };
    let main_py_string = main_py.to_string_lossy().to_string();
    let mut args_owned = vec![
        "-W".to_string(),
        "ignore::FutureWarning".to_string(),
        main_py_string,
    ];
    let launch_args = comfyui_launch_args(
        settings.comfyui_listen_enabled,
        settings.comfyui_pinned_memory_enabled,
        effective_attention.as_deref(),
        settings.comfyui_lowvram_enabled,
        settings.comfyui_bf16_unet_enabled,
        settings.comfyui_async_offload_enabled,
        settings.comfyui_disable_smart_memory_enabled,
        &custom_launch_args,
    );
    emit_comfyui_runtime_event(
        app,
        "launch_args",
        format!(
            "Launching with attention backend: {}",
            effective_attention
                .as_deref()
                .unwrap_or("PyTorch attention")
        ),
    );
    for arg in launch_args {
        args_owned.push(arg);
    }
    let arg_refs: Vec<&str> = args_owned.iter().map(String::as_str).collect();
    let py_exe_string = py_exe.to_string_lossy().to_string();
    let python_envs = python_runtime_env_for_root(&root);
    let env_refs: Vec<(&str, &str)> = python_envs
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let mut cmd = build_command(&py_exe_string, &arg_refs, Some(&root), &env_refs)?;

    if nerdstats_enabled() {
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else if settings.comfyui_show_runtime_logs {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    } else {
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
    }

    let mut child = cmd
        .spawn()
        .map_err(|err| format!("Failed to start ComfyUI: {err}"))?;
    if !nerdstats_enabled() && settings.comfyui_show_runtime_logs {
        if let Some(stdout) = child.stdout.take() {
            spawn_comfyui_runtime_log_stream(app.clone(), "stdout", stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_comfyui_runtime_log_stream(app.clone(), "stderr", stderr);
        }
    }
    *recover_lock(state.comfyui_process.lock()) = Some(child);
    Ok(())
}

pub(crate) fn spawn_comfyui_start_monitor(app: &AppHandle, instance_name: String) {
    let app_handle = app.clone();
    std::thread::spawn(move || {
        let state = app_handle.state::<AppState>();
        match wait_for_comfyui_start(&state, Duration::from_secs(45)) {
            Ok(()) => {
                update_tray_comfy_status(&app_handle, true);
                emit_comfyui_runtime_event(
                    &app_handle,
                    "started",
                    format!("{instance_name} started."),
                );
                if let Err(err) = open::that("http://127.0.0.1:8188") {
                    log::warn!("Failed to open ComfyUI in browser: {err}");
                }
            }
            Err(err) => {
                let running = comfyui_runtime_running(&state);
                update_tray_comfy_status(&app_handle, running);
                emit_comfyui_runtime_event(
                    &app_handle,
                    "start_failed",
                    format!("{instance_name} start failed: {err}"),
                );
            }
        }
    });
}

fn comfyui_listener_running() -> bool {
    let addr = ("127.0.0.1", 8188)
        .to_socket_addrs()
        .ok()
        .and_then(|mut iter| iter.next());
    let Some(addr) = addr else {
        return false;
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(180)).is_ok()
}

#[derive(Debug, Serialize)]
struct ComfyAddonState {
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

fn spawn_comfyui_runtime_log_stream(
    app: AppHandle,
    stream_name: &'static str,
    reader: impl std::io::Read + Send + 'static,
) {
    std::thread::spawn(move || {
        let _ = drain_lossy_lines(reader, |text| {
            emit_comfyui_runtime_log_event(&app, stream_name, text)
        });
    });
}

fn python_for_root(root: &Path) -> std::process::Command {
    let install_dir = root
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf());
    let linux_dot_venv_py = root.join(".venv").join("bin").join("python");
    let legacy_linux_dot_venv_py = install_dir.join(".venv").join("bin").join("python");
    let linux_venv_py = root.join("venv").join("bin").join("python");
    let legacy_linux_venv_py = install_dir.join("venv").join("bin").join("python");

    let mut cmd = if linux_dot_venv_py.exists() {
        std::process::Command::new(linux_dot_venv_py)
    } else if legacy_linux_dot_venv_py.exists() {
        std::process::Command::new(legacy_linux_dot_venv_py)
    } else if linux_venv_py.exists() {
        std::process::Command::new(linux_venv_py)
    } else if legacy_linux_venv_py.exists() {
        std::process::Command::new(legacy_linux_venv_py)
    } else if command_available("python3", &["--version"]) {
        std::process::Command::new("python3")
    } else {
        std::process::Command::new("python")
    };
    if !nerdstats_enabled() {
        apply_background_command_flags(&mut cmd);
    }
    apply_torch_allocator_env_compat(&mut cmd);
    cmd
}

fn python_exe_candidates_for_root(root: &Path) -> Vec<PathBuf> {
    let install_dir = root
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf());
    vec![
        root.join(".venv").join("bin").join("python"),
        install_dir.join(".venv").join("bin").join("python"),
        root.join("venv").join("bin").join("python"),
        install_dir.join("venv").join("bin").join("python"),
    ]
}

fn python_exe_works(py_exe: &Path, root: &Path) -> bool {
    if !py_exe.exists() {
        return false;
    }
    let mut cmd = std::process::Command::new(py_exe);
    cmd.arg("--version");
    cmd.current_dir(root);
    apply_background_command_flags(&mut cmd);
    apply_torch_allocator_env_compat(&mut cmd);
    cmd.output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn resolve_start_python_exe(
    _app: &AppHandle,
    _state: &AppState,
    root: &Path,
) -> Result<PathBuf, String> {
    let candidates = python_exe_candidates_for_root(root);
    for candidate in &candidates {
        if python_exe_works(candidate, root) {
            return Ok(candidate.clone());
        }
    }

    if candidates.iter().any(|c| c.exists()) {
        return Err(
            "Detected Python runtime candidates, but none are executable. Reinstall ComfyUI runtime."
                .to_string(),
        );
    }

    if command_available("python3", &["--version"]) {
        return Ok(PathBuf::from("python3"));
    }
    if command_available("python", &["--version"]) {
        return Ok(PathBuf::from("python"));
    }

    Err(
        "No working Python executable found for this ComfyUI install. Reinstall or run Install New once to bootstrap runtime."
            .to_string(),
    )
}
fn python_exe_for_root(root: &Path) -> Result<PathBuf, String> {
    let install_dir = root
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf());
    let candidates = [
        root.join(".venv").join("bin").join("python"),
        install_dir.join(".venv").join("bin").join("python"),
        root.join("venv").join("bin").join("python"),
        install_dir.join("venv").join("bin").join("python"),
    ];
    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("Python executable for this ComfyUI install was not found.".to_string())
}

fn pip_has_package(root: &Path, package: &str) -> bool {
    let mut cmd = python_for_root(root);
    cmd.arg("-m").arg("pip").arg("show").arg(package);
    cmd.current_dir(root);
    cmd.output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
struct GithubTagEntry {
    name: String,
}

fn comfyui_origin_github_repo(root: &Path) -> Option<(String, String)> {
    let config = std::fs::read_to_string(root.join(".git").join("config")).ok()?;
    let mut in_origin = false;

    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[remote ") {
            in_origin = trimmed == r#"[remote "origin"]"#;
            continue;
        }
        if !in_origin {
            continue;
        }
        if let Some(url) = trimmed.strip_prefix("url =") {
            return parse_github_repo_from_url(url.trim());
        }
    }

    None
}

fn parse_github_repo_from_url(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim().trim_end_matches('/');
    let path = if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("http://github.com/") {
        rest
    } else if let Some(rest) = trimmed.strip_prefix("ssh://git@github.com/") {
        rest
    } else {
        trimmed.strip_prefix("git@github.com:")?
    };

    let mut parts = path.trim_end_matches(".git").split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

fn github_latest_release_tag(owner: &str, repo: &str) -> Option<(String, String)> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!(
            "ArcticDownloader/{} ({})",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_NAME")
        ))
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;

    let mut best: Option<((u64, u64, u64), String, String)> = None;
    for page in 1..=5 {
        let url =
            format!("https://api.github.com/repos/{owner}/{repo}/tags?per_page=100&page={page}");
        let response = client.get(&url).send().ok()?.error_for_status().ok()?;
        let tags: Vec<GithubTagEntry> = response.json().ok()?;
        if tags.is_empty() {
            break;
        }
        for entry in tags {
            let tag = entry.name;
            let Some(version) = normalize_release_version(&tag) else {
                continue;
            };
            let Some(parsed) = parse_semver_triplet(&version) else {
                continue;
            };
            match &best {
                Some((current, _, _)) if *current >= parsed => {}
                _ => best = Some((parsed, tag, version)),
            }
        }
    }

    best.map(|(_, tag, version)| (tag, version))
}

pub(crate) fn git_latest_release_tag(root: &Path) -> Option<(String, String)> {
    if let Ok((stdout, _)) = run_command_capture(
        "git",
        &["ls-remote", "--tags", "--refs", "origin"],
        Some(root),
    ) {
        let mut best: Option<((u64, u64, u64), String, String)> = None;

        for line in stdout.lines() {
            let mut cols = line.split_whitespace();
            let Some(_sha) = cols.next() else {
                continue;
            };
            let Some(ref_name) = cols.next() else {
                continue;
            };
            let Some(tag) = ref_name.strip_prefix("refs/tags/") else {
                continue;
            };
            let Some(version) = normalize_release_version(tag) else {
                continue;
            };
            let Some(parsed) = parse_semver_triplet(&version) else {
                continue;
            };

            match &best {
                Some((current, _, _)) if *current >= parsed => {}
                _ => best = Some((parsed, tag.to_string(), version)),
            }
        }
        if let Some((_, tag, version)) = best {
            return Some((tag, version));
        }
    }

    let (owner, repo) = comfyui_origin_github_repo(root)
        .unwrap_or_else(|| ("comfyanonymous".to_string(), "ComfyUI".to_string()));
    github_latest_release_tag(&owner, &repo)
}

fn host_comfyui_running_for_needle(needle: &str) -> bool {
    match run_command_capture("pgrep", &["-f", needle], None) {
        Ok((stdout, _)) => stdout.lines().any(|line| !line.trim().is_empty()),
        Err(_) => false,
    }
}

fn signal_host_pids(pids: &[String], signal: &str) {
    for pid in pids {
        let _ = run_command_capture("kill", &[signal, pid.as_str()], None);
    }
}

fn kill_host_comfyui_for_root(root: &Path) -> Result<bool, String> {
    let main_py = root.join("main.py");
    if !main_py.exists() {
        return Ok(false);
    }
    let needle = main_py.to_string_lossy().to_string();
    let (stdout, _) = match run_command_capture("pgrep", &["-f", &needle], None) {
        Ok(output) => output,
        Err(_) => return Ok(false),
    };

    let pids: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if pids.is_empty() {
        return Ok(false);
    }

    signal_host_pids(&pids, "-TERM");

    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        let still_running = host_comfyui_running_for_needle(&needle);
        if !still_running || !comfyui_listener_running() {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    let remaining: Vec<String> = match run_command_capture("pgrep", &["-f", &needle], None) {
        Ok((stdout, _)) => stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
        Err(_) => Vec::new(),
    };
    if remaining.is_empty() || !comfyui_listener_running() {
        return Ok(true);
    }

    signal_host_pids(&remaining, "-KILL");
    let force_deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if !host_comfyui_running_for_needle(&needle) || !comfyui_listener_running() {
            return Ok(true);
        }
        if Instant::now() >= force_deadline {
            return Err("Failed to stop host ComfyUI process cleanly.".to_string());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn kill_python_processes_for_root(_root: &Path, py_exe: &Path) -> Result<bool, String> {
    // Match the installation's exact virtual-environment interpreter rather
    // than the broader root path, so unrelated Python processes cannot be
    // selected merely because one of their arguments mentions the folder.
    //
    // `py_exe` can be a bare command name like "python3" when no venv
    // interpreter exists for this root (see `python_for_root`'s system
    // fallback). Canonicalizing a bare name that doesn't resolve against the
    // current directory would previously fall back to that same unqualified
    // string, turning the `pgrep -f` needle below into a wildcard that
    // matches every process on the machine whose command line happens to
    // contain "python3" (IDEs, language servers, unrelated scripts) --
    // refuse instead of ever degrading to that.
    let Ok(interpreter) = std::fs::canonicalize(py_exe) else {
        log::warn!(
            "Refusing to search for lingering ComfyUI Python processes: '{}' is not a resolvable interpreter path (no venv found for this install).",
            py_exe.display()
        );
        return Ok(false);
    };
    if !interpreter.is_absolute() {
        return Ok(false);
    }
    let needle = interpreter.to_string_lossy().to_string();
    if needle.trim().is_empty() {
        return Ok(false);
    }

    let matching_pids = || -> Vec<String> {
        run_command_capture("pgrep", &["-f", &needle], None)
            .map(|(stdout, _)| {
                stdout
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };

    let pids = matching_pids();
    if pids.is_empty() {
        return Ok(false);
    }
    signal_host_pids(&pids, "-TERM");

    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        if matching_pids().is_empty() {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    let remaining = matching_pids();
    signal_host_pids(&remaining, "-KILL");
    std::thread::sleep(Duration::from_millis(250));
    if matching_pids().is_empty() {
        Ok(true)
    } else {
        Err("Failed to stop lingering ComfyUI Python processes.".to_string())
    }
}

fn restart_comfyui_after_mutation(
    app: &AppHandle,
    state: &AppState,
    was_running: bool,
) -> Result<(), String> {
    if !was_running {
        return Ok(());
    }
    start_comfyui_root_impl(app, state, None)?;
    wait_for_comfyui_start(state, Duration::from_secs(45))?;
    update_tray_comfy_status(app, true);
    emit_comfyui_runtime_event(
        app,
        "restarted_after_changes",
        "ComfyUI restarted after install/remove operation.",
    );
    Ok(())
}

#[tauri::command]
fn get_comfyui_addon_state(
    state: State<'_, AppState>,
    comfyui_root: Option<String>,
) -> Result<ComfyAddonState, String> {
    let root = resolve_root_path(&state.context, comfyui_root)?;
    let settings = state.context.config.settings();
    let same_as_configured_root =
        settings.comfyui_root.as_ref().map(|p| {
            normalize_canonical_path(&std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        }) == Some(root.clone());
    let has_sage3 = python_module_importable(&root, "sageattn3");
    let has_sage_pkg = python_module_importable(&root, "sageattention");
    let has_sage = has_sage_pkg && !has_sage3;
    let has_flash = python_module_importable(&root, "flash_attn");
    let has_nunchaku = python_module_importable(&root, "nunchaku")
        || pip_has_package(&root, "nunchaku")
        || custom_node_exists(&root, "nunchaku_nodes")
        || custom_node_exists(&root, "ComfyUI-nunchaku");
    let launch_attention: String = if same_as_configured_root {
        match settings.comfyui_attention_backend.as_deref() {
            Some("none") => "none".to_string(),
            Some("flash") if has_flash => "flash".to_string(),
            Some("sage3") if has_sage3 => "sage3".to_string(),
            Some("sage") if has_sage_pkg || has_sage3 => "sage".to_string(),
            Some("nunchaku") if has_nunchaku => "nunchaku".to_string(),
            _ => detect_launch_attention_backend_for_root(&root)
                .unwrap_or_else(|| "none".to_string()),
        }
    } else {
        detect_launch_attention_backend_for_root(&root).unwrap_or_else(|| "none".to_string())
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
        launch_sage_attention: launch_attention == "sage",
        launch_sage_attention3: launch_attention == "sage3",
        launch_flash_attention: launch_attention == "flash",
        sage_attention: has_sage,
        sage_attention3: has_sage3,
        flash_attention: has_flash,
        nunchaku: has_nunchaku,
        insight_face: pip_has_package(&root, "insightface"),
        trellis2: custom_node_exists(&root, "ComfyUI-Trellis2")
            || custom_node_exists(&root, "ComfyUI-TRELLIS2"),
        node_comfyui_manager: custom_node_installed(&root, &CUSTOM_NODES[0]),
        node_comfyui_easy_use: custom_node_installed(&root, &CUSTOM_NODES[1]),
        node_rgthree_comfy: custom_node_installed(&root, &CUSTOM_NODES[2]),
        node_comfyui_gguf: custom_node_installed(&root, &CUSTOM_NODES[3]),
        node_comfyui_kjnodes: custom_node_installed(&root, &CUSTOM_NODES[4]),
        node_comfyui_crystools: custom_node_installed(&root, &CUSTOM_NODES[5]),
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AttentionBackendChangeRequest {
    #[serde(default)]
    comfyui_root: Option<String>,
    target_backend: String, // none | sage | sage3 | flash | nunchaku
    #[serde(default)]
    torch_profile: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchAttentionFlagRequest {
    #[serde(default)]
    comfyui_root: Option<String>,
    target_backend: String, // none | sage | sage3 | flash
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComfyComponentToggleRequest {
    #[serde(default)]
    comfyui_root: Option<String>,
    component: String,
    enabled: bool,
}

#[tauri::command]
async fn apply_attention_backend_change(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AttentionBackendChangeRequest,
) -> Result<String, String> {
    let was_running = stop_comfyui_for_mutation(&app, &state)?;
    let root = resolve_root_path(&state.context, request.comfyui_root)?;
    let target = request.target_backend.trim().to_ascii_lowercase();
    if !matches!(
        target.as_str(),
        "none" | "sage" | "sage3" | "flash" | "nunchaku"
    ) {
        return Err("Unknown attention backend target.".to_string());
    }
    if target == "sage3" {
        let gpu = detect_nvidia_gpu_details();
        let is_50_series = gpu
            .name
            .as_deref()
            .map(|name| name.to_ascii_lowercase().contains("rtx 50"))
            .unwrap_or(false);
        if !is_50_series {
            return Err(
                "SageAttention3 is available only for NVIDIA RTX 50-series GPUs.".to_string(),
            );
        }
    }

    let py_path = {
        let probe = python_for_root(&root);
        probe.get_program().to_string_lossy().to_string()
    };
    let py_exe = PathBuf::from(&py_path);
    let _ = kill_python_processes_for_root(&root, &py_exe);

    let shared_runtime_root = state.context.config.cache_path().join("comfyui-runtime");
    let uv_bin = resolve_uv_binary(&shared_runtime_root, &app)?;
    let uv_python_install_dir = shared_runtime_root
        .join(".python")
        .to_string_lossy()
        .to_string();
    let profile = if let Some(profile) = request.torch_profile.clone() {
        profile
    } else {
        profile_from_torch_env(&root)?
    };
    let hopper_sm90 = is_nvidia_hopper_sm90();
    let triton_pkg = triton_package_for_profile_linux(&profile);

    // Everything below is git clone + `uv pip install` + wheel downloads --
    // potentially minutes of network and subprocess I/O -- so it runs off
    // the command dispatch thread, matching
    // apply_comfyui_component_toggle's equivalent branch. The originals are
    // still available below the `.await` -- only clones are moved in.
    {
        let root = root.clone();
        let py_path = py_path.clone();
        let profile = profile.clone();
        let target = target.clone();
        let uv_bin = uv_bin.clone();
        let uv_python_install_dir = uv_python_install_dir.clone();
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            force_cleanup_attention_backends(&root, &py_path)?;

            match target.as_str() {
                "none" => {}
                "sage" => {
                    run_uv_pip_strict(
                        &uv_bin,
                        &py_path,
                        &["install", "--upgrade", "--force-reinstall", triton_pkg],
                        Some(&root),
                        &[("UV_PYTHON_INSTALL_DIR", &uv_python_install_dir)],
                    )?;
                    install_sageattention_linux(&root, &py_path, &profile, hopper_sm90)?;
                }
                "flash" => {
                    run_uv_pip_strict(
                        &uv_bin,
                        &py_path,
                        &["install", "--upgrade", "--force-reinstall", triton_pkg],
                        Some(&root),
                        &[("UV_PYTHON_INSTALL_DIR", &uv_python_install_dir)],
                    )?;
                    install_flashattention_linux(&root, &py_path, &profile, hopper_sm90)?;
                }
                "sage3" => {
                    run_uv_pip_strict(
                        &uv_bin,
                        &py_path,
                        &["install", "--upgrade", "--force-reinstall", triton_pkg],
                        Some(&root),
                        &[("UV_PYTHON_INSTALL_DIR", &uv_python_install_dir)],
                    )?;
                    install_linux_wheel_for_profile(
                        &root, &py_path, &profile, "sage3", hopper_sm90, true,
                    )?;
                    // Keep sageattention installed for ComfyUI --use-sage-attention compatibility checks.
                    install_sageattention_linux(&root, &py_path, &profile, hopper_sm90)?;
                }
                "nunchaku" => {
                    ensure_git_available(&app)?;
                    let custom_nodes_root = root.join("custom_nodes");
                    std::fs::create_dir_all(&custom_nodes_root).map_err(|err| err.to_string())?;
                    let nunchaku_node = root.join("custom_nodes").join("ComfyUI-nunchaku");
                    clone_or_update_repo(
                        &root,
                        &nunchaku_node,
                        "https://github.com/nunchaku-ai/ComfyUI-nunchaku",
                    )?;
                    let versions_json = nunchaku_node.join("nunchaku_versions.json");
                    let _ = download_http_file(
                        "https://nunchaku.tech/cdn/nunchaku_versions.json",
                        &versions_json,
                    );
                    run_uv_pip_strict(
                        &uv_bin,
                        &py_path,
                        &["install", "--upgrade", "--force-reinstall", triton_pkg],
                        Some(&root),
                        &[("UV_PYTHON_INSTALL_DIR", &uv_python_install_dir)],
                    )?;
                    install_insightface(&root, &uv_bin, &py_path, &uv_python_install_dir)?;
                    install_nunchaku_node_requirements(
                        &root,
                        &uv_bin,
                        &py_path,
                        &uv_python_install_dir,
                        &nunchaku_node,
                    )?;
                    install_linux_wheel_for_profile(
                        &root,
                        &py_path,
                        &profile,
                        "nunchaku",
                        hopper_sm90,
                        true,
                    )?;
                    if !nunchaku_backend_present(&root) {
                        return Err(
                            "Nunchaku backend install incomplete: module or custom node not detected."
                                .to_string(),
                        );
                    }
                }
                _ => return Err("Unknown attention backend target.".to_string()),
            }

            if target == "none" {
                let mut lingering: Vec<&str> = Vec::new();
                for pkg in [
                    "sageattention",
                    "sageattn3",
                    "flash-attn",
                    "flash_attn",
                    "nunchaku",
                ] {
                    if pip_has_package(&root, pkg) {
                        lingering.push(pkg);
                    }
                }
                let mut lingering_nodes: Vec<&str> = Vec::new();
                for node in ["ComfyUI-nunchaku", "nunchaku_nodes"] {
                    if custom_node_exists(&root, node) {
                        lingering_nodes.push(node);
                    }
                }
                if !lingering.is_empty() || !lingering_nodes.is_empty() {
                    let mut detail = String::new();
                    if !lingering.is_empty() {
                        detail.push_str(&format!(
                            "packages still installed: {}",
                            lingering.join(", ")
                        ));
                    }
                    if !lingering_nodes.is_empty() {
                        if !detail.is_empty() {
                            detail.push_str("; ");
                        }
                        detail.push_str(&format!(
                            "nodes still present: {}",
                            lingering_nodes.join(", ")
                        ));
                    }
                    return Err(format!(
                        "Attention backend removal incomplete ({detail}). Stop ComfyUI and retry."
                    ));
                }
            }

            Ok(())
        })
        .await
        .map_err(|err| format!("Attention backend change task failed: {err}"))??;
    }

    let target_setting = match target.as_str() {
        "sage" => Some("sage".to_string()),
        "sage3" => Some("sage3".to_string()),
        "flash" => Some("flash".to_string()),
        "nunchaku" => Some("nunchaku".to_string()),
        _ => Some("none".to_string()),
    };
    let _ = state.context.config.update_settings(|settings| {
        settings.comfyui_attention_backend = target_setting;
        settings.comfyui_torch_profile = Some(profile.clone());
    });

    restart_comfyui_after_mutation(&app, &state, was_running)?;
    Ok(format!("Applied attention backend: {target}"))
}

#[tauri::command]
async fn set_comfyui_launch_attention_backend(
    app: AppHandle,
    state: State<'_, AppState>,
    request: LaunchAttentionFlagRequest,
) -> Result<String, String> {
    let was_running = stop_comfyui_for_mutation(&app, &state)?;
    let root = resolve_root_path(&state.context, request.comfyui_root)?;
    let target = request.target_backend.trim().to_ascii_lowercase();
    if !matches!(target.as_str(), "none" | "sage" | "sage3" | "flash") {
        return Err("Unknown launch attention backend target.".to_string());
    }

    // Module-importability checks shell out to Python; run off the command
    // dispatch thread like this file's other install/mutation commands.
    let unavailable = {
        let root = root.clone();
        let target = target.clone();
        tauri::async_runtime::spawn_blocking(move || -> Option<&'static str> {
            match target.as_str() {
                "sage"
                    if !(python_module_importable(&root, "sageattention")
                        || python_module_importable(&root, "sageattn3")) =>
                {
                    Some("SageAttention launch flag is unavailable because SageAttention is not installed.")
                }
                "sage3" if !python_module_importable(&root, "sageattn3") => Some(
                    "SageAttention3 launch flag is unavailable because SageAttention3 is not installed.",
                ),
                "flash" if !python_module_importable(&root, "flash_attn") => Some(
                    "FlashAttention launch flag is unavailable because FlashAttention is not installed.",
                ),
                _ => None,
            }
        })
        .await
        .map_err(|err| format!("Launch attention backend check task failed: {err}"))?
    };
    if let Some(message) = unavailable {
        return Err(message.to_string());
    }

    let target_setting = match target.as_str() {
        "sage" => Some("sage".to_string()),
        "sage3" => Some("sage3".to_string()),
        "flash" => Some("flash".to_string()),
        _ => Some("none".to_string()),
    };
    state
        .context
        .config
        .update_settings(|settings| settings.comfyui_attention_backend = target_setting)
        .map_err(|err| err.to_string())?;

    restart_comfyui_after_mutation(&app, &state, was_running)?;
    Ok(match target.as_str() {
        "none" => "ComfyUI launch attention flags disabled.".to_string(),
        "sage" => "ComfyUI will launch with SageAttention.".to_string(),
        "sage3" => "ComfyUI will launch with SageAttention3.".to_string(),
        "flash" => "ComfyUI will launch with FlashAttention.".to_string(),
        _ => unreachable!(),
    })
}

fn install_insightface(
    root: &Path,
    uv_bin: &str,
    py_path: &str,
    uv_python_install_dir: &str,
) -> Result<(), String> {
    let profile = profile_from_torch_env(root)?;
    install_linux_wheel_for_profile(
        root,
        py_path,
        &profile,
        "insightface",
        is_nvidia_hopper_sm90(),
        true,
    )?;
    run_uv_pip_strict(
        uv_bin,
        py_path,
        &["install", "--upgrade", "onnx", "onnxruntime"],
        Some(root),
        &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
    )?;
    if !python_module_importable(root, "onnx") {
        return Err("InsightFace install incomplete: missing 'onnx' module.".to_string());
    }
    if !insightface_present(root) {
        return Err("InsightFace install incomplete: package/module not detected.".to_string());
    }
    Ok(())
}

fn uninstall_insightface(
    root: &Path,
    _uv_bin: &str,
    py_path: &str,
    _uv_python_install_dir: &str,
) -> Result<(), String> {
    pip_uninstall_best_effort(root, py_path, &["insightface", "filterpywhl", "facexlib"]);
    remove_insightface_site_packages_artifacts(root)?;
    if insightface_present(root)
        || pip_has_package(root, "facexlib")
        || pip_has_package(root, "filterpywhl")
    {
        return Err(
            "Failed to fully remove InsightFace dependencies. Stop ComfyUI and retry.".to_string(),
        );
    }
    Ok(())
}

fn install_trellis2(
    root: &Path,
    uv_bin: &str,
    py_path: &str,
    uv_python_install_dir: &str,
) -> Result<(), String> {
    // Trellis2 stack is pinned to torch280_cu128 in this app.
    enforce_torch_profile_linux(
        uv_bin,
        py_path,
        root,
        "torch280_cu128",
        uv_python_install_dir,
    )?;

    let custom_nodes_dir = root.join("custom_nodes");
    std::fs::create_dir_all(&custom_nodes_dir).map_err(|err| err.to_string())?;

    let trellis_dir = custom_nodes_dir.join("ComfyUI-TRELLIS2");
    clone_or_update_repo(
        root,
        &trellis_dir,
        "https://github.com/ArcticLatent/ComfyUI-TRELLIS2",
    )?;
    let trellis_req = trellis_dir.join("requirements.txt");
    if trellis_req.exists() {
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &["install", "-r", &trellis_req.to_string_lossy(), "--no-deps"],
            Some(root),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &["install", "--upgrade", "open3d"],
            Some(root),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
    }

    let geometry_dir = custom_nodes_dir.join("ComfyUI-GeometryPack");
    clone_or_update_repo(
        root,
        &geometry_dir,
        "https://github.com/PozzettiAndrea/ComfyUI-GeometryPack",
    )?;
    let geometry_req = geometry_dir.join("requirements.txt");
    if geometry_req.exists() {
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &[
                "install",
                "-r",
                &geometry_req.to_string_lossy(),
                "--no-deps",
            ],
            Some(root),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
    }
    run_uv_pip_strict(
        uv_bin,
        py_path,
        &["install", "--upgrade", "tomli"],
        Some(root),
        &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
    )?;

    let ultrashape_dir = custom_nodes_dir.join("ComfyUI-UltraShape1");
    clone_or_update_repo(
        root,
        &ultrashape_dir,
        "https://github.com/jtydhr88/ComfyUI-UltraShape1",
    )?;
    let ultrashape_req = ultrashape_dir.join("requirements.txt");
    if ultrashape_req.exists() {
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &[
                "install",
                "-r",
                &ultrashape_req.to_string_lossy(),
                "--no-deps",
            ],
            Some(&ultrashape_dir),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
        run_uv_pip_strict(
            uv_bin,
            py_path,
            &["install", "-U", "accelerate"],
            Some(&ultrashape_dir),
            &[("UV_PYTHON_INSTALL_DIR", uv_python_install_dir)],
        )?;
    }

    let ultrashape_models_dir = root.join("models").join("UltraShape");
    std::fs::create_dir_all(&ultrashape_models_dir).map_err(|err| err.to_string())?;
    let ultrashape_model_file = ultrashape_models_dir.join("ultrashape_v1.pt");
    if !ultrashape_model_file.exists() {
        download_http_file(
            "https://huggingface.co/infinith/UltraShape/resolve/main/ultrashape_v1.pt",
            &ultrashape_model_file,
        )?;
    }

    // Re-assert stack after Trellis requirements/custom nodes.
    enforce_torch_profile_linux(
        uv_bin,
        py_path,
        root,
        "torch280_cu128",
        uv_python_install_dir,
    )?;

    Ok(())
}

fn uninstall_trellis2(
    root: &Path,
    _uv_bin: &str,
    py_path: &str,
    _uv_python_install_dir: &str,
) -> Result<(), String> {
    remove_custom_node_dirs(
        root,
        &[
            "ComfyUI-TRELLIS2",
            "ComfyUI-GeometryPack",
            "ComfyUI-UltraShape1",
        ],
    );
    pip_uninstall_best_effort(root, py_path, &["accelerate", "open3d"]);
    Ok(())
}

fn install_named_custom_node(
    app: &AppHandle,
    root: &Path,
    py_exe: &Path,
    repo_url: &str,
    folder_name: &str,
) -> Result<(), String> {
    let custom_nodes = root.join("custom_nodes");
    std::fs::create_dir_all(&custom_nodes).map_err(|err| err.to_string())?;
    install_custom_node(app, root, &custom_nodes, py_exe, repo_url, folder_name)
}

#[tauri::command]
async fn apply_comfyui_component_toggle(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ComfyComponentToggleRequest,
) -> Result<String, String> {
    let was_running = stop_comfyui_for_mutation(&app, &state)?;
    let component = request.component.trim().to_ascii_lowercase();

    let result = if matches!(
        component.as_str(),
        "addon_pinned_memory"
            | "pinned_memory"
            | "launch_listen"
            | "addon_launch_listen"
            | "launch_lowvram"
            | "addon_launch_lowvram"
            | "launch_bf16_unet"
            | "addon_launch_bf16_unet"
            | "launch_async_offload"
            | "addon_launch_async_offload"
            | "launch_disable_smart_memory"
            | "addon_launch_disable_smart_memory"
    ) {
        match component.as_str() {
            "addon_pinned_memory" | "pinned_memory" => {
                let enabled = request.enabled;
                state
                    .context
                    .config
                    .update_settings(|settings| settings.comfyui_pinned_memory_enabled = enabled)
                    .map_err(|err| err.to_string())?;
                if enabled {
                    Ok("Pinned memory enabled.".to_string())
                } else {
                    Ok("Pinned memory disabled.".to_string())
                }
            }
            "launch_listen" | "addon_launch_listen" => {
                let enabled = request.enabled;
                state
                    .context
                    .config
                    .update_settings(|settings| settings.comfyui_listen_enabled = enabled)
                    .map_err(|err| err.to_string())?;
                if enabled {
                    Ok("ComfyUI will start with --listen enabled.".to_string())
                } else {
                    Ok("ComfyUI will start without --listen.".to_string())
                }
            }
            "launch_lowvram" | "addon_launch_lowvram" => {
                let enabled = request.enabled;
                state
                    .context
                    .config
                    .update_settings(|settings| settings.comfyui_lowvram_enabled = enabled)
                    .map_err(|err| err.to_string())?;
                if enabled {
                    Ok("ComfyUI will start with --lowvram enabled.".to_string())
                } else {
                    Ok("ComfyUI will start without --lowvram.".to_string())
                }
            }
            "launch_bf16_unet" | "addon_launch_bf16_unet" => {
                let enabled = request.enabled;
                state
                    .context
                    .config
                    .update_settings(|settings| settings.comfyui_bf16_unet_enabled = enabled)
                    .map_err(|err| err.to_string())?;
                if enabled {
                    Ok("ComfyUI will start with --bf16-unet enabled.".to_string())
                } else {
                    Ok("ComfyUI will start without --bf16-unet.".to_string())
                }
            }
            "launch_async_offload" | "addon_launch_async_offload" => {
                let enabled = request.enabled;
                state
                    .context
                    .config
                    .update_settings(|settings| settings.comfyui_async_offload_enabled = enabled)
                    .map_err(|err| err.to_string())?;
                if enabled {
                    Ok("ComfyUI will start with --async-offload enabled.".to_string())
                } else {
                    Ok("ComfyUI will start without --async-offload.".to_string())
                }
            }
            "launch_disable_smart_memory" | "addon_launch_disable_smart_memory" => {
                let enabled = request.enabled;
                state
                    .context
                    .config
                    .update_settings(|settings| {
                        settings.comfyui_disable_smart_memory_enabled = enabled
                    })
                    .map_err(|err| err.to_string())?;
                if enabled {
                    Ok("ComfyUI will start with --disable-smart-memory enabled.".to_string())
                } else {
                    Ok("ComfyUI will start without --disable-smart-memory.".to_string())
                }
            }
            _ => Err("Unknown component toggle target.".to_string()),
        }
    } else {
        let root = resolve_root_path(&state.context, request.comfyui_root)?;
        let py_path = {
            let probe = python_for_root(&root);
            probe.get_program().to_string_lossy().to_string()
        };
        let py_exe = PathBuf::from(&py_path);
        let _ = kill_python_processes_for_root(&root, &py_exe);

        let shared_runtime_root = state.context.config.cache_path().join("comfyui-runtime");
        let uv_bin = resolve_uv_binary(&shared_runtime_root, &app)?;
        let uv_python_install_dir = shared_runtime_root
            .join(".python")
            .to_string_lossy()
            .to_string();
        let app_clone = app.clone();
        let root_clone = root.clone();
        let py_path_clone = py_path.clone();
        let py_exe_clone = py_exe.clone();
        let component_clone = component.clone();
        let uv_bin_clone = uv_bin.clone();
        let uv_python_install_dir_clone = uv_python_install_dir.clone();
        let enabled = request.enabled;
        tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
            match component_clone.as_str() {
                "addon_insightface" | "insightface" => {
                    if enabled {
                        install_insightface(
                            &root_clone,
                            &uv_bin_clone,
                            &py_path_clone,
                            &uv_python_install_dir_clone,
                        )?;
                        Ok("Installed InsightFace.".to_string())
                    } else {
                        if detect_launch_attention_backend_for_root(&root_clone).as_deref()
                            == Some("nunchaku")
                        {
                            return Err(
                                "Cannot remove InsightFace while Nunchaku is selected. Switch attention backend first."
                                    .to_string(),
                            );
                        }
                        uninstall_insightface(
                            &root_clone,
                            &uv_bin_clone,
                            &py_path_clone,
                            &uv_python_install_dir_clone,
                        )?;
                        Ok("Removed InsightFace.".to_string())
                    }
                }
                "addon_trellis2" | "trellis2" => {
                    if enabled {
                        ensure_git_available(&app_clone)?;
                        install_trellis2(
                            &root_clone,
                            &uv_bin_clone,
                            &py_path_clone,
                            &uv_python_install_dir_clone,
                        )?;
                        Ok("Installed Trellis2.".to_string())
                    } else {
                        uninstall_trellis2(
                            &root_clone,
                            &uv_bin_clone,
                            &py_path_clone,
                            &uv_python_install_dir_clone,
                        )?;
                        Ok("Removed Trellis2.".to_string())
                    }
                }
                key @ ("node_comfyui_manager" | "node_comfyui_easy_use" | "node_rgthree_comfy"
                    | "node_comfyui_gguf" | "node_comfyui_kjnodes" | "node_comfyui_crystools") => {
                    let spec = custom_node_spec(key)
                        .expect("key matched one of CUSTOM_NODES' flag_key values above");
                    if enabled {
                        ensure_git_available(&app_clone)?;
                        install_named_custom_node(
                            &app_clone,
                            &root_clone,
                            &py_exe_clone,
                            spec.repo_url,
                            spec.install_folder_name,
                        )?;
                        Ok(format!("Installed {}.", spec.display_name))
                    } else {
                        remove_custom_node_dirs(&root_clone, spec.known_folder_names);
                        Ok(format!("Removed {}.", spec.display_name))
                    }
                }
                _ => Err("Unknown component toggle target.".to_string()),
            }
        })
        .await
        .map_err(|err| format!("Component operation task failed: {err}"))?
    }?;

    restart_comfyui_after_mutation(&app, &state, was_running)?;
    Ok(result)
}

#[tauri::command]
async fn update_selected_comfyui(
    app: AppHandle,
    state: State<'_, AppState>,
    comfyui_root: Option<String>,
) -> Result<String, String> {
    let was_running = stop_comfyui_for_mutation(&app, &state)?;
    let root = resolve_root_path(&state.context, comfyui_root)?;
    if !root.join("main.py").is_file() {
        return Err("Selected folder is not a valid ComfyUI root.".to_string());
    }
    if !root.join(".git").exists() {
        return Err("Selected ComfyUI install is not git-based.".to_string());
    }

    let Some((latest_tag, latest_version)) = git_latest_release_tag(&root) else {
        return Err("Could not resolve latest ComfyUI release tag from remote.".to_string());
    };
    let installed_version_norm =
        read_comfyui_installed_version(&root).and_then(|v| normalize_release_version(&v));
    if let Some(current) = installed_version_norm {
        let current_triplet = parse_semver_triplet(&current);
        let latest_triplet = parse_semver_triplet(&latest_version);
        if matches!(
            (current_triplet, latest_triplet),
            (Some(local), Some(latest)) if local >= latest
        ) {
            return Ok(format!(
                "ComfyUI is already on latest release tag (v{latest_version})."
            ));
        }
    }

    let shared_runtime_root = state.context.config.cache_path().join("comfyui-runtime");
    let uv_bin = resolve_uv_binary(&shared_runtime_root, &app)?;
    let uv_python_install_dir = shared_runtime_root
        .join(".python")
        .to_string_lossy()
        .to_string();
    let selected_profile = resolve_desired_torch_profile(&state.context.config.settings(), &root);
    let latest_tag_for_task = latest_tag.clone();
    let latest_version_for_task = latest_version.clone();
    let branch_for_task_raw = git_current_branch(&root).unwrap_or_else(|| "master".to_string());
    let branch_for_task = if branch_for_task_raw.eq_ignore_ascii_case("head") {
        "master".to_string()
    } else {
        branch_for_task_raw
    };
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        run_command_with_retry("git", &["fetch", "--tags", "origin"], Some(&root), 2)?;
        if let Err(err) =
            run_command_with_retry("git", &["merge", "--ff-only", &latest_tag_for_task], Some(&root), 2)
        {
            let lower = err.to_ascii_lowercase();
            let can_repoint_branch = lower.contains("unrelated histories")
                || lower.contains("not possible to fast-forward")
                || lower.contains("not possible to fast forward")
                || lower.contains("cannot fast-forward")
                || lower.contains("diverging");
            if can_repoint_branch {
                // Preserve recoverability before repointing branch tip to release tag.
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let backup_branch = format!("arctic-backup-before-tag-update-{ts}");
                run_command_with_retry("git", &["branch", &backup_branch], Some(&root), 1)
                    .map_err(|backup_err| {
                        format!(
                            "Failed to create backup branch before tag migration ({backup_branch}). Details: {backup_err}"
                        )
                    })?;
                run_command_with_retry(
                    "git",
                    &["checkout", "-B", &branch_for_task, &latest_tag_for_task],
                    Some(&root),
                    1,
                )
                .map_err(|checkout_err| {
                    format!(
                        "Failed to switch branch '{}' to release tag {} after merge fast-forward failed. Backup branch: {}. Details: {}",
                        branch_for_task, latest_tag_for_task, backup_branch, checkout_err
                    )
                })?;
            } else {
                return Err(format!(
                    "Failed to fast-forward ComfyUI to release tag {latest_tag_for_task}. Resolve local git divergence first. Details: {err}"
                ));
            }
        }

        let py = python_exe_for_root(&root)?;
        let req = root.join("requirements.txt");
        if req.exists() {
            run_uv_pip_strict(
                &uv_bin,
                py.to_string_lossy().as_ref(),
                &["install", "-r", "requirements.txt", "--no-cache"],
                Some(&root),
                &[("UV_PYTHON_INSTALL_DIR", &uv_python_install_dir)],
            )
            .map_err(|err| format!("Failed to install ComfyUI requirements: {err}"))?;
            enforce_torch_profile_linux(
                &uv_bin,
                py.to_string_lossy().as_ref(),
                &root,
                &selected_profile,
                &uv_python_install_dir,
            )
            .map_err(|err| format!("Failed to re-apply selected torch profile: {err}"))?;
        }
        Ok(format!(
            "ComfyUI updated successfully to release tag {latest_tag_for_task} (v{latest_version_for_task})."
        ))
    })
    .await
    .map_err(|err| format!("ComfyUI update task failed: {err}"))??;

    restart_comfyui_after_mutation(&app, &state, was_running)?;
    Ok(format!(
        "ComfyUI updated successfully to release tag {latest_tag} (v{latest_version})."
    ))
}

pub(crate) fn stop_comfyui_root_impl(state: &AppState) -> Result<bool, String> {
    let configured_root = state.context.config.settings().comfyui_root;
    let mut stopped_any = kill_managed_comfyui_child(state)?;

    // After app restart, or when Flatpak launches ComfyUI on the host, we may no
    // longer have a child handle that can stop the real server process. In that case,
    // stop the host listener process by its selected ComfyUI root.
    if let Some(root) = configured_root {
        let root = normalize_canonical_path(&std::fs::canonicalize(&root).unwrap_or(root));
        if kill_host_comfyui_for_root(&root)? {
            stopped_any = true;
        }
    }

    Ok(stopped_any)
}

fn main_window_icon() -> Option<Image<'static>> {
    static MAIN_ICON: OnceLock<Option<Image<'static>>> = OnceLock::new();
    MAIN_ICON
        .get_or_init(|| {
            Image::from_bytes(include_bytes!("../icons/icon.png"))
                .ok()
                .or_else(|| Image::from_bytes(include_bytes!("../icons/favicon.ico")).ok())
                .or_else(|| Image::from_bytes(include_bytes!("../icons/icon.ico")).ok())
        })
        .clone()
}

#[cfg(feature = "desktop-tray")]
fn stopped_tray_icon() -> Option<Image<'static>> {
    static STOPPED_ICON: OnceLock<Option<Image<'static>>> = OnceLock::new();
    STOPPED_ICON
        .get_or_init(|| {
            #[cfg(target_os = "linux")]
            {
                Image::from_bytes(include_bytes!("../icons/icon-32.png")).ok()
            }

            #[cfg(not(target_os = "linux"))]
            {
                Image::from_bytes(include_bytes!("../icons/favicon.ico"))
                    .ok()
                    .or_else(|| Image::from_bytes(include_bytes!("../icons/icon.ico")).ok())
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
                Image::from_bytes(include_bytes!("../icons/started-32.png"))
                    .ok()
                    .or_else(|| Image::from_bytes(include_bytes!("../icons/icon-32.png")).ok())
            }

            #[cfg(not(target_os = "linux"))]
            {
                Image::from_bytes(include_bytes!("../icons/started.ico"))
                    .ok()
                    .or_else(|| Image::from_bytes(include_bytes!("../icons/icon.ico")).ok())
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
fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
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
fn setup_tray(_app: &AppHandle) -> tauri::Result<()> {
    Ok(())
}

#[tauri::command]
fn cancel_active_download(state: State<'_, AppState>) -> Result<bool, String> {
    let mut active = recover_lock(state.active_cancel.lock());
    let mut abort = recover_lock(state.active_abort.lock());
    if let Some(token) = active.as_ref() {
        token.cancel();
        // Cancelling the token only stops the download at its next
        // cooperative checkpoint; force-abort the task too so a transfer
        // blocked in a non-cooperative call (e.g. a stalled read) is
        // actually torn down instead of continuing in the background after
        // the UI reports "cancelled". Matches app_windows.rs.
        if let Some(handle) = abort.take() {
            handle.abort();
        }
        *active = None;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn run() {
    #[cfg(target_os = "linux")]
    {
        // Work around blank window / GBM allocation failures on some Wayland+NVIDIA setups.
        // Allow users to override externally if they need different behavior.
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        // Additional fallback for blank/transparent WebKit views on Linux GPU drivers.
        if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
        install_linux_gdk_log_filter();
    }

    let args: Vec<String> = std::env::args().collect();
    let nerdstats = args
        .iter()
        .any(|arg| arg.eq_ignore_ascii_case("--nerdstats"));
    let fakeamd = args.iter().any(|arg| arg.eq_ignore_ascii_case("--fakeamd"));
    let fakeintel = args
        .iter()
        .any(|arg| arg.eq_ignore_ascii_case("--fakeintel"));
    let fakeamd_allow_rocm_setup = args
        .iter()
        .any(|arg| arg.eq_ignore_ascii_case("--fakeamd-allow-rocm-setup"));
    let fakeintel_allow_xpu_setup = args
        .iter()
        .any(|arg| arg.eq_ignore_ascii_case("--fakeintel-allow-xpu-setup"));
    if nerdstats {
        std::env::set_var("ARCTIC_NERDSTATS", "1");
    }
    if fakeamd {
        std::env::set_var("ARCTIC_FAKE_AMD", "1");
    }
    if fakeintel {
        std::env::set_var("ARCTIC_FAKE_INTEL", "1");
    }
    if fakeamd_allow_rocm_setup {
        std::env::set_var("ARCTIC_FAKE_AMD_ALLOW_ROCM_SETUP", "1");
    }
    if fakeintel_allow_xpu_setup {
        std::env::set_var("ARCTIC_FAKE_INTEL_ALLOW_XPU_SETUP", "1");
    }
    if nerdstats {
        try_attach_parent_console();
    }
    env_logger::Builder::from_default_env()
        .filter_level(if nerdstats {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .target(env_logger::Target::Stdout)
        .init();

    if nerdstats {
        log::info!("Nerdstats mode enabled (verbose runtime logging).");
    }
    if fakeamd {
        if fakeamd_allow_rocm_setup {
            log::info!("Fake AMD mode enabled with real guided ROCm setup allowed for testing.");
        } else {
            log::info!("Fake AMD mode enabled (UI simulation only; guided ROCm setup disabled).");
        }
    }
    if fakeintel {
        if fakeintel_allow_xpu_setup {
            log::info!("Fake Intel mode enabled with real guided Intel setup allowed for testing.");
        } else {
            log::info!(
                "Fake Intel mode enabled (UI simulation only; guided Intel setup disabled)."
            );
        }
    }

    let context = match build_context() {
        Ok(context) => context,
        Err(err) => {
            eprintln!("Failed to initialize app context: {err:#}");
            std::process::exit(1);
        }
    };
    let mut tauri_context = tauri::generate_context!();
    tauri_context.set_default_window_icon(main_window_icon());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let _ = show_main_window(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            if tray_enabled_for_platform() {
                setup_tray(app.handle())?;
            } else {
                log::info!("System tray disabled for this platform/runtime.");
            }
            warm_linux_prereq_cache_background();
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Only hide-to-tray when tray support is enabled on this platform.
                // On Linux we disable tray by default, so close should quit the app.
                if !tray_enabled_for_platform() {
                    return;
                }
                let state = window.app_handle().state::<AppState>();
                let quitting = state.quitting.lock().map(|flag| *flag).unwrap_or(false);
                if !quitting {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .manage(AppState {
            context,
            active_cancel: Mutex::new(None),
            active_abort: Mutex::new(None),
            install_cancel: Mutex::new(None),
            comfyui_process: Mutex::new(None),
            quitting: Mutex::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            crate::platform::get_platform_capabilities,
            get_app_snapshot,
            crate::shared::get_catalog,
            crate::shared::refresh_catalog,
            crate::shared::get_settings,
            inspect_comfyui_path,
            list_comfyui_installations,
            get_comfyui_install_recommendation,
            crate::shared::set_comfyui_gpu_selection,
            get_rocm_guided_status,
            install_rocm_guided,
            get_xpu_guided_status,
            install_xpu_guided,
            crate::shared::get_comfyui_resume_state,
            get_comfyui_addon_state,
            apply_attention_backend_change,
            set_comfyui_launch_attention_backend,
            apply_comfyui_component_toggle,
            crate::shared::get_comfyui_update_status,
            update_selected_comfyui,
            run_comfyui_preflight,
            get_hf_xet_preflight,
            set_hf_xet_enabled,
            set_comfyui_root,
            set_comfyui_install_base,
            crate::shared::get_comfyui_extra_model_config,
            crate::shared::get_effective_download_destination,
            crate::shared::set_comfyui_extra_model_config,
            crate::shared::set_comfyui_custom_launch_args,
            crate::shared::set_comfyui_show_runtime_logs,
            crate::shared::save_civitai_token,
            check_updates_now,
            auto_update_startup,
            crate::shared::download_model_assets,
            crate::shared::download_model_assets_batch,
            download_lora_asset,
            crate::shared::download_workflow_asset,
            crate::shared::get_lora_metadata,
            start_comfyui_install,
            crate::shared::cancel_comfyui_install,
            crate::shared::start_comfyui_root,
            crate::shared::stop_comfyui_root,
            crate::shared::get_comfyui_runtime_status,
            open_folder,
            open_external_url,
            crate::shared::pick_folder,
            cancel_active_download
        ])
        .run(tauri_context)
        .expect("failed to run tauri application");
}

fn tray_enabled_for_platform() -> bool {
    #[cfg(not(feature = "desktop-tray"))]
    {
        return false;
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

#[cfg(target_os = "linux")]
fn install_linux_gdk_log_filter() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    if INSTALLED.get().is_some() {
        return;
    }

    glib::log_set_writer_func(|level, fields| {
        let mut domain: Option<&str> = None;
        let mut message: Option<&str> = None;
        for field in fields {
            match field.key() {
                "GLIB_DOMAIN" => domain = field.value_str(),
                "MESSAGE" => message = field.value_str(),
                _ => {}
            }
        }

        if matches!(level, glib::LogLevel::Critical)
            && domain == Some("Gdk")
            && message
                .map(|m| m.contains("gdk_window_thaw_toplevel_updates"))
                .unwrap_or(false)
        {
            return glib::LogWriterOutput::Handled;
        }

        if matches!(level, glib::LogLevel::Warning)
            && domain == Some("libayatana-appindicator")
            && message
                .map(|m| {
                    m.contains("libayatana-appindicator is deprecated")
                        && m.contains("libayatana-appindicator-glib")
                })
                .unwrap_or(false)
        {
            return glib::LogWriterOutput::Handled;
        }

        glib::log_writer_default(level, fields)
    });

    let _ = INSTALLED.set(());
}

#[cfg(test)]
mod gpu_detection_tests {
    use super::*;

    #[test]
    fn finds_headless_nvidia_controller_in_mixed_lspci_output() {
        let output = concat!(
            "03:00.0 VGA compatible controller [0300]: Advanced Micro Devices, Inc. ",
            "[AMD/ATI] Navi 22 [Radeon RX 6700 XT] [1002:73df]\n",
            "09:00.0 3D controller [0302]: NVIDIA Corporation GB206 ",
            "[GeForce RTX 5060 Ti] [10de:2d04]",
        );

        assert_eq!(
            find_nvidia_gpu_name_in_lspci(output).as_deref(),
            Some("NVIDIA Corporation GB206 [GeForce RTX 5060 Ti] [10de:2d04]")
        );
    }

    #[test]
    fn mixed_gpu_recommendation_prefers_nvidia_cuda() {
        let recommendation = comfy_install_recommendation_for(
            NvidiaGpuDetails {
                name: Some("NVIDIA GeForce RTX 5060 Ti".to_string()),
                vram_mb: Some(16_384),
                driver_version: Some("580.95.05".to_string()),
                compute_capability: Some("12.0".to_string()),
            },
            Some("AMD Radeon RX 6700 XT".to_string()),
            None,
            None,
            false,
        );

        assert_eq!(
            recommendation.gpu_name.as_deref(),
            Some("NVIDIA GeForce RTX 5060 Ti")
        );
        assert_eq!(recommendation.torch_profile, "torch291_cu130");
        assert!(!recommendation.detection_pending);
    }

    #[test]
    fn explicit_amd_selection_uses_rocm_on_mixed_gpu_system() {
        let recommendation = comfy_install_recommendation_for(
            NvidiaGpuDetails {
                name: Some("NVIDIA GeForce RTX 5060 Ti".to_string()),
                vram_mb: Some(16_384),
                driver_version: Some("580.95.05".to_string()),
                compute_capability: Some("12.0".to_string()),
            },
            Some("AMD Radeon RX 6700 XT".to_string()),
            None,
            Some("amd"),
            false,
        );

        assert_eq!(
            recommendation.gpu_name.as_deref(),
            Some("AMD Radeon RX 6700 XT")
        );
        assert_eq!(recommendation.torch_profile, "torch211_rocm72");
    }

    #[test]
    fn rocm72_profile_uses_matching_pytorch_packages_and_is_detectable() {
        assert_eq!(
            torch_profile_to_packages_linux("torch211_rocm72"),
            (
                "2.11.0",
                "0.26.0",
                "2.11.0",
                "https://download.pytorch.org/whl/rocm7.2",
            )
        );
        assert_eq!(
            torch_profile_from_versions("2.11.0+rocm7.2", "7.2.41134").as_deref(),
            Some("torch211_rocm72")
        );
    }
}
