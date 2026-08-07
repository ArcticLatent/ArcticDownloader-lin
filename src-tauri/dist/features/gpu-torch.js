import { comfyTorchProfiles, el, invoke, state } from "../lib/app-context.js";
import { formatVramMbToGb } from "../lib/display-format.js";

/** @param {ArcticGpuTorchDependencies} dependencies */
export function createGpuTorchFeature({ logComfyLine, setOptions }) {
/** @param {unknown} profile */
function torchProfileLabel(profile) {
  const value = String(profile || "").trim();
  return comfyTorchProfiles.find((item) => item.value === value)?.label || value || "Unknown profile";
}

function platformLabel() {
  return state.platformCapabilities?.platform === "windows" ? "Windows" : "Linux";
}

/** @param {unknown} profile */
function torchProfileBackend(profile) {
  const value = String(profile || "").trim();
  return String(comfyTorchProfiles.find((item) => item.value === value)?.backend || "");
}

/**
 * @param {string} backend
 * @param {string | null} [requestedValue]
 */
function preferredTorchProfile(backend, requestedValue = null) {
  const requested = comfyTorchProfiles.find((item) => item.value === requestedValue);
  if (requested && requested.backend === backend) return requested.value;
  if (backend === "nvidia") {
    const preferredCuda = comfyTorchProfiles.find((item) => item.value === "torch280_cu128");
    if (preferredCuda) return preferredCuda.value;
  }
  return comfyTorchProfiles.find((item) => item.backend === backend)?.value || requestedValue || "";
}

/** @param {unknown} profile */
function isRocmTorchProfile(profile) {
  return String(profile || "").includes("_rocm");
}

/** @param {ArcticRecord} snapshot */
function gpuOptionsFromSnapshot(snapshot) {
  const options = [];
  const nvidiaName = String(snapshot?.nvidia_gpu_name || "").trim();
  const amdName = String(snapshot?.amd_gpu_name || "").trim();
  const intelName = String(snapshot?.intel_gpu_name || "").trim();
  if (nvidiaName) {
    const vram = formatVramMbToGb(snapshot?.nvidia_gpu_vram_mb);
    options.push({ value: "nvidia", vendor: "nvidia", name: nvidiaName, label: `NVIDIA: ${nvidiaName}${vram ? ` (${vram})` : ""}` });
  }
  if (amdName) options.push({ value: "amd", vendor: "amd", name: amdName, label: `AMD: ${amdName}` });
  if (intelName) options.push({ value: "intel", vendor: "intel", name: intelName, label: `Intel: ${intelName}` });
  return options;
}

function selectedGpuVendor(selection = state.comfyGpuSelection) {
  if (selection !== "auto") return selection;
  return state.detectedGpus.find((gpu) => gpu.vendor === "nvidia")?.vendor
    || state.detectedGpus[0]?.vendor
    || "";
}

/** @param {ArcticRecord} snapshot */
function refreshGpuSelectionOptions(snapshot) {
  state.detectedGpus = gpuOptionsFromSnapshot(snapshot);
  const options = [{ value: "auto", label: "GPU: Automatic (recommended)" }]
    .concat(state.detectedGpus.map((gpu) => ({ value: gpu.value, label: gpu.label })));
  const requestedSelection = state.comfyGpuSelection;
  const selectionAvailable = requestedSelection === "auto"
    || state.detectedGpus.some((gpu) => gpu.value === requestedSelection);
  const effectiveSelection = selectionAvailable ? requestedSelection : "auto";
  if (!selectionAvailable && !snapshot?.gpu_detection_pending) {
    state.comfyGpuSelection = "auto";
    invoke("set_comfyui_gpu_selection", { gpuSelection: "auto" })
      .then((settings) => { state.settings = settings; })
      .catch((err) => logComfyLine(`Could not clear unavailable GPU selection: ${err}`));
  }
  setOptions(el.comfyGpuSelection, options, effectiveSelection);
  el.comfyGpuSelection.value = effectiveSelection;
  state.comfyDetectedGpuVendor = selectedGpuVendor(effectiveSelection);
  const selected = state.detectedGpus.find((gpu) => gpu.value === effectiveSelection);
  if (el.comfyGpuSelectionHelp) {
    const platform = platformLabel();
    el.comfyGpuSelectionHelp.textContent = effectiveSelection === "auto"
      ? `Platform: ${platform} • Automatically selects NVIDIA first, then another available GPU.`
      : `Platform: ${platform} • Torch will be configured for ${selected?.label || effectiveSelection}.`;
  }
}
/** @param {boolean} detecting */
function setTorchRecommendedDetecting(detecting) {
  if (!el.comfyTorchRecommended) return;
  if (detecting) {
    el.comfyTorchRecommended.textContent = "Detecting torch/add-ons for selected install...";
  } else {
    el.comfyTorchRecommended.textContent = state.comfyTorchRecommendedBase;
  }
}

function currentGuidedAccelTarget() {
  const profile = String(el.comfyTorchProfile?.value || "").trim();
  if (isRocmTorchProfile(profile) && state.platformCapabilities?.supports_rocm_guided_setup) {
    return {
      key: "rocm",
      statusLabel: "ROCm status",
      checkLabel: "Check ROCm",
      installLabel: "Guided ROCm Setup",
      checkCommand: "get_rocm_guided_status",
      installCommand: "install_rocm_guided",
      logPrefix: "ROCm",
      detectedField: "amd_detected",
    };
  }
  if (profile === "torch291_xpu" && state.platformCapabilities?.supports_xpu_guided_setup) {
    return {
      key: "xpu",
      statusLabel: "Intel XPU status",
      checkLabel: "Check Intel XPU",
      installLabel: "Guided Intel Setup",
      checkCommand: "get_xpu_guided_status",
      installCommand: "install_xpu_guided",
      logPrefix: "Intel XPU",
      detectedField: "intel_detected",
    };
  }
  return null;
}

function updateRocmGuidedUi() {
  const target = currentGuidedAccelTarget();
  const status = state.rocmGuidedStatus;
  const statusMatchesTarget = status?.target === target?.key;
  const show = Boolean(target) && !(statusMatchesTarget && status?.ready);
  if (el.rocmGuidedRow) {
    el.rocmGuidedRow.classList.toggle("hidden", !show);
  }
  if (el.rocmGuidedActions) {
    el.rocmGuidedActions.classList.toggle("hidden", !show);
  }
  if (!show) {
    return;
  }
  if (!target) return;
  if (el.rocmGuidedStatus) {
    el.rocmGuidedStatus.textContent = statusMatchesTarget && status?.detail
      ? `${target.statusLabel}: ${status.detail}`
      : `${target.statusLabel}: Not checked.`;
  }
  if (el.rocmGuidedCheck) {
    el.rocmGuidedCheck.disabled = state.rocmGuidedBusy;
    el.rocmGuidedCheck.textContent = state.rocmGuidedBusy ? "Checking..." : target.checkLabel;
  }
  if (el.rocmGuidedInstall) {
    const gpuDetected = !statusMatchesTarget ? true : status?.[target.detectedField] !== false;
    const installBlocked = state.rocmGuidedBusy || (statusMatchesTarget && status?.supported === false) || !gpuDetected;
    el.rocmGuidedInstall.disabled = installBlocked;
    el.rocmGuidedInstall.textContent = state.rocmGuidedBusy ? "Running Setup..." : target.installLabel;
  }
}

async function refreshRocmGuidedStatus(logResult = false) {
  if (!invoke) return null;
  const target = currentGuidedAccelTarget();
  if (!target) {
    state.rocmGuidedStatus = null;
    updateRocmGuidedUi();
    return null;
  }
  state.rocmGuidedBusy = true;
  updateRocmGuidedUi();
  try {
    const status = await invoke(target.checkCommand);
    state.rocmGuidedStatus = status ? { ...status, target: target.key } : null;
    if (logResult && status?.detail) {
      logComfyLine(`${target.logPrefix} check: ${status.detail}`);
    }
    return status;
  } catch (err) {
    state.rocmGuidedStatus = {
      detail: `Failed to check ${target.logPrefix} status: ${err}`,
      supported: false,
      [target.detectedField]: true,
      target: target.key,
    };
    if (logResult) {
      logComfyLine(String(state.rocmGuidedStatus.detail));
    }
    return null;
  } finally {
    state.rocmGuidedBusy = false;
    updateRocmGuidedUi();
  }
}
function comfyTorchProfileOptionsForDetectedGpu() {
  const vendor = String(state.comfyDetectedGpuVendor || "").toLowerCase();
  if (["amd", "intel", "nvidia"].includes(vendor)) {
    return comfyTorchProfiles.map((item) => ({
      ...item,
      disabled: item.backend !== vendor,
    }));
  }
  return comfyTorchProfiles.map((item) => ({ ...item, disabled: false }));
}

/** @param {string | null} [selectedValue] */
function applyComfyTorchProfileOptions(selectedValue = null) {
  if (!el.comfyTorchProfile) return;
  const options = comfyTorchProfileOptionsForDetectedGpu();
  const vendor = String(state.comfyDetectedGpuVendor || "").toLowerCase();
  const requestedValue = String(selectedValue || "").trim();
  const requestedOption = options.find((item) => item.value === requestedValue);
  const validRequestedValue = requestedOption && !requestedOption.disabled
    ? requestedValue
    : null;
  const forcedValue = ["amd", "intel", "nvidia"].includes(vendor)
    ? preferredTorchProfile(vendor, validRequestedValue)
    : selectedValue;
  setOptions(el.comfyTorchProfile, options, forcedValue);
  if (forcedValue) {
    el.comfyTorchProfile.value = forcedValue;
  }
  updateRocmGuidedUi();
}

  return {
    applyComfyTorchProfileOptions,
    currentGuidedAccelTarget,
    isRocmTorchProfile,
    platformLabel,
    preferredTorchProfile,
    refreshGpuSelectionOptions,
    refreshRocmGuidedStatus,
    selectedGpuVendor,
    setTorchRecommendedDetecting,
    torchProfileBackend,
    torchProfileLabel,
    updateRocmGuidedUi,
  };
}
