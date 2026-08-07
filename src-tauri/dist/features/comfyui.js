import { comfyTorchProfiles, el, invoke, state } from "../lib/app-context.js";
import { normalizeSlashes, parentDir, PATH_SEP } from "../lib/path.js";

/** @param {ArcticComfyUiDependencies} dependencies */
export function createComfyUiFeature({
  hideStartupOverlay,
  logComfyLine,
  refreshEffectiveDownloadDestination,
  renderTitleMeta,
  setToggleBusy,
  setTorchRecommendedDetecting,
  showBlockingOverlay,
  showConfirmDialog,
  torchProfileBackend,
  torchProfileLabel,
  waitForNextPaint,
}) {
  /** @type {Map<string, {status: ArcticRecord, at: number}>} */
  const comfyUpdateStatusCache = new Map();
  /** @type {Map<string, Promise<ArcticRecord>>} */
  const comfyUpdateStatusInflight = new Map();
  const COMFY_UPDATE_STATUS_CACHE_MS = 4000;

  /** @param {string} rootPath */
  function clearComfyUpdateStatusCache(rootPath) {
    comfyUpdateStatusCache.delete(normalizeSlashes(rootPath));
  }
function updateComfyInstallButton() {
  if (!el.installComfyui) return;
  el.installComfyui.textContent = state.comfyInstallBusy ? "Cancel Install" : "Install ComfyUI";
  el.comfyInstallSpinner?.classList.toggle("hidden", !state.comfyInstallBusy);
}

function updateComfyUpdateButton() {
  const btn = el.updateSelectedInstall;
  if (!btn) return;
  const hasSelection = Boolean(String(el.comfyExistingInstall?.value || "").trim());
  btn.classList.toggle("hidden", !hasSelection);
  btn.classList.remove("update-available");
  if (!hasSelection) return;

  if (state.comfyInstallSwitchBusy) {
    btn.textContent = "Switching...";
    btn.disabled = true;
    return;
  }
  if (state.comfyUpdateChecking) {
    btn.textContent = "Checking...";
    btn.disabled = true;
    return;
  }
  if (state.comfyUpdateBusy) {
    btn.textContent = "Updating...";
    btn.disabled = true;
    return;
  }
  if (!state.comfyUpdateChecked) {
    btn.textContent = "Check ComfyUI";
    btn.disabled = false;
    return;
  }
  if (state.comfyUpdateAvailable) {
    btn.textContent = "Update ComfyUI";
    btn.disabled = false;
    btn.classList.add("update-available");
    return;
  }
  btn.textContent = "No Update";
  btn.disabled = true;
}

/** @param {unknown} name */
function comfyInstallOrder(name) {
  const lower = String(name || "").trim().toLowerCase();
  if (lower === "comfyui") return 0;
  const match = /^comfyui-(\d+)$/.exec(lower);
  if (!match) return -1;
  const ordinal = match[1];
  if (!ordinal) return -1;
  const parsed = Number.parseInt(ordinal, 10);
  return Number.isFinite(parsed) ? parsed : -1;
}

/**
 * @param {ArcticComfyInstall[]} installs
 * @returns {ArcticComfyInstall | null}
 */
function newestComfyInstall(installs) {
  if (!Array.isArray(installs) || installs.length === 0) return null;
  const first = installs[0];
  if (!first) return null;
  let best = first;
  let bestOrder = comfyInstallOrder(best.name);
  for (const item of installs.slice(1)) {
    const order = comfyInstallOrder(item?.name);
    if (order > bestOrder) {
      best = item;
      bestOrder = order;
    }
  }
  return best;
}

/** @param {unknown} rootPath */
function comfyInstallNameFromRoot(rootPath) {
  const normalized = normalizeSlashes(String(rootPath || "").trim());
  if (!normalized) return "ComfyUI";
  const parts = normalized.split(PATH_SEP).filter(Boolean);
  return parts.at(-1) || "ComfyUI";
}

/**
 * @param {unknown} installDir
 * @param {unknown} comfyRoot
 */
function setComfyQuickActions(installDir, comfyRoot) {
  const install = String(installDir || "").trim();
  const root = String(comfyRoot || "").trim();
  if (!install && !root) {
    el.comfyQuickActions?.classList.add("hidden");
    return;
  }
  const finalInstall = install || parentDir(root);
  const finalRoot = root || finalInstall;
  el.comfyQuickActions?.classList.remove("hidden");
  if (el.comfyLastInstallPath) {
    el.comfyLastInstallPath.textContent = `Last install: ${finalInstall}`;
  }
  if (el.comfyOpenInstallFolder) {
    el.comfyOpenInstallFolder.dataset.path = finalRoot;
  }
  if (el.comfyStartInstalled) {
    el.comfyStartInstalled.dataset.path = finalRoot;
  }
}

/** @returns {ArcticComfyInstallRequest} */
function buildComfyInstallRequest() {
  const extraModelRoot = String(el.comfyExtraModelRoot?.value || "").trim();
  return {
    installRoot: String(el.comfyInstallRoot.value || "").trim(),
    extraModelRoot: extraModelRoot || null,
    extraModelUseDefault: Boolean(el.comfyExtraModelDefault?.checked && extraModelRoot),
    torchProfile: el.comfyTorchProfile.value || null,
    includeSageAttention: Boolean(el.addonSageAttention.checked),
    includeSageAttention3: Boolean(el.addonSageAttention3.checked),
    includeFlashAttention: Boolean(el.addonFlashAttention.checked),
    includeInsightFace: Boolean(el.addonInsightFace.checked),
    includeNunchaku: Boolean(el.addonNunchaku.checked),
    includeTrellis2: Boolean(el.addonTrellis2?.checked),
    includePinnedMemory: Boolean(el.addonPinnedMemory?.checked ?? true),
    nodeComfyuiManager: Boolean(el.nodeComfyuiManager.checked),
    nodeComfyuiEasyUse: Boolean(el.nodeComfyuiEasyUse.checked),
    nodeRgthreeComfy: Boolean(el.nodeRgthreeComfy.checked),
    nodeComfyuiGguf: Boolean(el.nodeComfyuiGguf.checked),
    nodeComfyuiKjnodes: Boolean(el.nodeComfyuiKjnodes.checked),
    nodeComfyuiCrystools: Boolean(el.nodeComfyuiCrystools?.checked),
  };
}

function resetComfySelectionsToDefaults() {
  if (el.addonSageAttention) el.addonSageAttention.checked = false;
  if (el.addonSageAttention3) el.addonSageAttention3.checked = false;
  if (el.addonFlashAttention) el.addonFlashAttention.checked = false;
  if (el.addonNunchaku) el.addonNunchaku.checked = false;
  if (el.addonInsightFace) el.addonInsightFace.checked = false;
  if (el.addonTrellis2) el.addonTrellis2.checked = false;
  if (el.addonPinnedMemory) el.addonPinnedMemory.checked = true;
  if (el.flagSageAttention) el.flagSageAttention.checked = false;
  if (el.flagFlashAttention) el.flagFlashAttention.checked = false;
  if (el.launchListen) el.launchListen.checked = false;
  if (el.flagLowvram) el.flagLowvram.checked = false;
  if (el.flagBf16Unet) el.flagBf16Unet.checked = false;
  if (el.flagAsyncOffload) el.flagAsyncOffload.checked = false;
  if (el.flagDisableSmartMemory) el.flagDisableSmartMemory.checked = false;

  if (el.nodeComfyuiManager) el.nodeComfyuiManager.checked = false;
  if (el.nodeComfyuiEasyUse) el.nodeComfyuiEasyUse.checked = false;
  if (el.nodeRgthreeComfy) el.nodeRgthreeComfy.checked = false;
  if (el.nodeComfyuiGguf) el.nodeComfyuiGguf.checked = false;
  if (el.nodeComfyuiKjnodes) el.nodeComfyuiKjnodes.checked = false;
  if (el.nodeComfyuiCrystools) el.nodeComfyuiCrystools.checked = false;
  applyComfyAddonRules();
}

/** @param {unknown} comfyuiRoot */
async function loadInstalledAddonState(comfyuiRoot) {
  const root = String(comfyuiRoot || el.comfyRoot.value || "").trim();
  if (!root) return;
  const loadSeq = ++state.comfyAddonLoadSeq;
  try {
    const installed = await invoke("get_comfyui_addon_state", { comfyuiRoot: root });
    if (loadSeq !== state.comfyAddonLoadSeq) return;
    const selectedRoot = normalizeSlashes(String(
      el.comfyExistingInstall?.value || el.comfyRoot.value || "",
    ).trim());
    if (selectedRoot && normalizeSlashes(root) !== selectedRoot) return;
    const installedTorchProfile = String(installed?.torch_profile || "").trim();
    if (installedTorchProfile && comfyTorchProfiles.some((x) => x.value === installedTorchProfile)) {
      el.comfyTorchProfile.value = installedTorchProfile;
      state.comfyTorchProfileLocked = true;
      if (state.comfyMode === "manage") {
        logComfyLine(`Selected install is using ${torchProfileLabel(installedTorchProfile)}.`);
      }
    }
    if (el.addonSageAttention) el.addonSageAttention.checked = Boolean(installed?.sage_attention);
    if (el.addonSageAttention3) el.addonSageAttention3.checked = Boolean(installed?.sage_attention3);
    if (el.addonFlashAttention) el.addonFlashAttention.checked = Boolean(installed?.flash_attention);
    if (el.addonNunchaku) el.addonNunchaku.checked = Boolean(installed?.nunchaku);
    if (el.addonInsightFace) el.addonInsightFace.checked = Boolean(installed?.insight_face);
    if (el.addonTrellis2) el.addonTrellis2.checked = Boolean(installed?.trellis2);
    if (el.flagSageAttention) {
      el.flagSageAttention.checked = Boolean(
        installed?.launch_sage_attention || installed?.launch_sage_attention3
      );
    }
    if (el.flagFlashAttention) el.flagFlashAttention.checked = Boolean(installed?.launch_flash_attention);
    if (el.launchListen) el.launchListen.checked = Boolean(installed?.listen_enabled);
    if (el.flagLowvram) el.flagLowvram.checked = Boolean(installed?.lowvram_enabled);
    if (el.flagBf16Unet) el.flagBf16Unet.checked = Boolean(installed?.bf16_unet_enabled);
    if (el.flagAsyncOffload) el.flagAsyncOffload.checked = Boolean(installed?.async_offload_enabled);
    if (el.flagDisableSmartMemory) el.flagDisableSmartMemory.checked = Boolean(installed?.disable_smart_memory_enabled);

    if (el.nodeComfyuiManager) el.nodeComfyuiManager.checked = Boolean(installed?.node_comfyui_manager);
    if (el.nodeComfyuiEasyUse) el.nodeComfyuiEasyUse.checked = Boolean(installed?.node_comfyui_easy_use);
    if (el.nodeRgthreeComfy) el.nodeRgthreeComfy.checked = Boolean(installed?.node_rgthree_comfy);
    if (el.nodeComfyuiGguf) el.nodeComfyuiGguf.checked = Boolean(installed?.node_comfyui_gguf);
    if (el.nodeComfyuiKjnodes) el.nodeComfyuiKjnodes.checked = Boolean(installed?.node_comfyui_kjnodes);
    if (el.nodeComfyuiCrystools) el.nodeComfyuiCrystools.checked = Boolean(installed?.node_comfyui_crystools);
    applyComfyAddonRules();
  } catch (_) {
    // Ignore when root is unset or not fully installed yet.
  }
}

function updateComfyRuntimeButton() {
  if (!el.comfyStartInstalled) return;
  const running = Boolean(state.comfyRuntimeRunning);
  const starting = Boolean(state.comfyRuntimeStarting);
  const busy = Boolean(
    state.comfyAttentionBusy
    || state.comfyComponentBusy,
  );
  const target = String(state.comfyRuntimeTarget || "").trim();
  if (starting) {
    el.comfyStartInstalled.textContent = target ? `Starting ${target}...` : "Starting ComfyUI...";
    el.comfyStartInstalled.disabled = true;
    el.comfyStartInstalled.classList.remove("stop-state");
    el.comfyStartInstalled.classList.add("starting-state");
    return;
  }
  if (busy) {
    el.comfyStartInstalled.textContent = running ? "Stop ComfyUI" : "Applying changes...";
    el.comfyStartInstalled.disabled = true;
    el.comfyStartInstalled.classList.toggle("stop-state", running);
    el.comfyStartInstalled.classList.remove("starting-state");
    return;
  }
  el.comfyStartInstalled.textContent = running ? "Stop ComfyUI" : "Start ComfyUI";
  el.comfyStartInstalled.disabled = false;
  el.comfyStartInstalled.classList.toggle("stop-state", running);
  el.comfyStartInstalled.classList.remove("starting-state");
}

function attentionAddonEntries() {
  return [
    { box: el.addonSageAttention, backend: "sage", label: "SageAttention" },
    { box: el.addonSageAttention3, backend: "sage3", label: "SageAttention3" },
    { box: el.addonFlashAttention, backend: "flash", label: "FlashAttention" },
    { box: el.addonNunchaku, backend: "nunchaku", label: "Nunchaku" },
  ].filter((entry) => Boolean(entry.box));
}

/** @param {HTMLInputElement} box */
function attentionEntryForBox(box) {
  return attentionAddonEntries().find((entry) => entry.box === box) || null;
}

/** @param {HTMLInputElement | null} [exceptBox] */
function checkedAttentionEntries(exceptBox = null) {
  return attentionAddonEntries().filter((entry) => entry.box !== exceptBox && entry.box.checked);
}

/** @param {HTMLInputElement} changedBox */
function enforceExclusiveAttentionSelectionLocal(changedBox) {
  if (!changedBox?.checked) return;
  checkedAttentionEntries(changedBox).forEach((entry) => {
    entry.box.checked = false;
  });
}

function attentionFlagEntries() {
  return [
    {
      box: el.flagSageAttention,
      backend: () => (el.addonSageAttention3?.checked ? "sage3" : "sage"),
      label: "SageAttention",
    },
    { box: el.flagFlashAttention, backend: "flash", label: "FlashAttention" },
  ].filter((entry) => Boolean(entry.box));
}

/** @param {HTMLInputElement} box */
function attentionFlagEntryForBox(box) {
  return attentionFlagEntries().find((entry) => entry.box === box) || null;
}

/** @param {HTMLInputElement | null} [exceptBox] */
function checkedAttentionFlagEntries(exceptBox = null) {
  return attentionFlagEntries().filter((entry) => entry.box !== exceptBox && entry.box.checked);
}

/** @param {HTMLInputElement} changedBox */
function enforceExclusiveAttentionFlagSelectionLocal(changedBox) {
  if (!changedBox?.checked) return;
  checkedAttentionFlagEntries(changedBox).forEach((entry) => {
    entry.box.checked = false;
  });
}

/** @param {HTMLInputElement} changedBox */
async function applyAttentionBackendFromToggle(changedBox) {
  if (!changedBox) return;
  if (state.comfyMode !== "manage") {
    enforceExclusiveAttentionSelectionLocal(changedBox);
    return;
  }
  if (state.comfyAttentionBusy) return;

  const root = String(el.comfyRoot.value || "").trim();
  if (!root) {
    logComfyLine("Set ComfyUI folder first.");
    changedBox.checked = !changedBox.checked;
    return;
  }

  const changed = attentionEntryForBox(changedBox);
  if (!changed) return;
  const others = checkedAttentionEntries(changedBox);
  let targetBackend = "none";
  let confirmMessage = "";

  if (changedBox.checked) {
    targetBackend = changed.backend;
    if (others.length > 0) {
      const installed = others[0];
      if (installed) {
        confirmMessage = `Are you sure you want to install '${changed.label}'?\nInstalling '${changed.label}' will automatically remove '${installed.label}'.`;
      }
    }
  } else {
    confirmMessage = `Are you sure you want to remove '${changed.label}'?`;
  }

  if (confirmMessage && !(await showConfirmDialog(confirmMessage))) {
    changedBox.checked = !changedBox.checked;
    return;
  }

  await waitForNextPaint();
  state.comfyAttentionBusy = true;
  updateComfyRuntimeButton();
  setToggleBusy(changedBox, true);
  try {
    const result = await invoke("apply_attention_backend_change", {
      request: {
        comfyuiRoot: root,
        targetBackend,
        torchProfile: el.comfyTorchProfile?.value || null,
      },
    });
    if (result) {
      logComfyLine(String(result));
    }
    await loadInstalledAddonState(root);
  } catch (err) {
    logComfyLine(`Attention backend change failed: ${err}`);
    await loadInstalledAddonState(root);
  } finally {
    state.comfyAttentionBusy = false;
    updateComfyRuntimeButton();
    setToggleBusy(changedBox, false);
  }
}

/** @param {HTMLInputElement} changedBox */
async function applyLaunchAttentionFlagFromToggle(changedBox) {
  if (!changedBox) return;
  enforceExclusiveAttentionFlagSelectionLocal(changedBox);
  if (state.comfyMode !== "manage") {
    return;
  }
  if (state.comfyAttentionBusy) return;

  const root = String(el.comfyRoot.value || "").trim();
  if (!root) {
    logComfyLine("Set ComfyUI folder first.");
    changedBox.checked = !changedBox.checked;
    return;
  }

  const changed = attentionFlagEntryForBox(changedBox);
  if (!changed) return;
  const targetBackend = changedBox.checked
    ? (typeof changed.backend === "function" ? changed.backend() : changed.backend)
    : "none";

  await waitForNextPaint();
  state.comfyAttentionBusy = true;
  updateComfyRuntimeButton();
  setToggleBusy(changedBox, true);
  try {
    const result = await invoke("set_comfyui_launch_attention_backend", {
      request: {
        comfyuiRoot: root,
        targetBackend,
      },
    });
    if (result) {
      logComfyLine(String(result));
    }
    await loadInstalledAddonState(root);
  } catch (err) {
    logComfyLine(`Launch flag change failed: ${err}`);
    await loadInstalledAddonState(root);
  } finally {
    state.comfyAttentionBusy = false;
    updateComfyRuntimeButton();
    setToggleBusy(changedBox, false);
  }
}

/**
 * @param {HTMLInputElement} changedBox
 * @param {string} component
 * @param {string} label
 */
async function applyComponentToggleFromCheckbox(changedBox, component, label) {
  if (!changedBox || state.comfyComponentBusy) return;
  const launchSettingOnly = [
    "addon_pinned_memory",
    "pinned_memory",
    "launch_listen",
    "addon_launch_listen",
    "launch_lowvram",
    "addon_launch_lowvram",
    "launch_bf16_unet",
    "addon_launch_bf16_unet",
    "launch_async_offload",
    "addon_launch_async_offload",
    "launch_disable_smart_memory",
    "addon_launch_disable_smart_memory",
  ].includes(String(component || "").trim());
  if (state.comfyMode !== "manage" && !launchSettingOnly) {
    return;
  }
  const root = String(el.comfyRoot.value || "").trim();
  if (!root && !launchSettingOnly) {
    logComfyLine("Set ComfyUI folder first.");
    changedBox.checked = !changedBox.checked;
    return;
  }

  const enabling = Boolean(changedBox.checked);
  const action = ([
    "launch_listen",
    "addon_launch_listen",
    "launch_lowvram",
    "addon_launch_lowvram",
    "launch_bf16_unet",
    "addon_launch_bf16_unet",
    "launch_async_offload",
    "addon_launch_async_offload",
    "launch_disable_smart_memory",
    "addon_launch_disable_smart_memory",
  ].includes(component)
      ? (enabling ? "enable" : "disable")
      : (enabling ? "install" : "remove"));
  const ok = await showConfirmDialog(`Are you sure you want to ${action} '${label}'?`);
  if (!ok) {
    changedBox.checked = !changedBox.checked;
    return;
  }

  await waitForNextPaint();
  state.comfyComponentBusy = true;
  updateComfyRuntimeButton();
  setToggleBusy(changedBox, true);
  try {
    const result = await invoke("apply_comfyui_component_toggle", {
      request: {
        comfyuiRoot: root || null,
        component,
        enabled: enabling,
        torchProfile: el.comfyTorchProfile?.value || null,
      },
    });
    if (result) {
      logComfyLine(String(result));
    }
  } catch (err) {
    logComfyLine(`Component change failed: ${err}`);
  } finally {
    await loadInstalledAddonState(root);
    if (component === "addon_pinned_memory" && el.addonPinnedMemory) {
      try {
        const settings = await invoke("get_settings");
        el.addonPinnedMemory.checked = settings?.comfyui_pinned_memory_enabled !== false;
      } catch (_) {}
    }
    if ((component === "launch_listen" || component === "addon_launch_listen") && el.launchListen) {
      try {
        const settings = await invoke("get_settings");
        el.launchListen.checked = settings?.comfyui_listen_enabled === true;
      } catch (_) {}
    }
    if ((component === "launch_lowvram" || component === "addon_launch_lowvram") && el.flagLowvram) {
      try {
        const settings = await invoke("get_settings");
        el.flagLowvram.checked = settings?.comfyui_lowvram_enabled === true;
      } catch (_) {}
    }
    if ((component === "launch_bf16_unet" || component === "addon_launch_bf16_unet") && el.flagBf16Unet) {
      try {
        const settings = await invoke("get_settings");
        el.flagBf16Unet.checked = settings?.comfyui_bf16_unet_enabled === true;
      } catch (_) {}
    }
    if ((component === "launch_async_offload" || component === "addon_launch_async_offload") && el.flagAsyncOffload) {
      try {
        const settings = await invoke("get_settings");
        el.flagAsyncOffload.checked = settings?.comfyui_async_offload_enabled === true;
      } catch (_) {}
    }
    if ((component === "launch_disable_smart_memory" || component === "addon_launch_disable_smart_memory") && el.flagDisableSmartMemory) {
      try {
        const settings = await invoke("get_settings");
        el.flagDisableSmartMemory.checked = settings?.comfyui_disable_smart_memory_enabled === true;
      } catch (_) {}
    }
    state.comfyComponentBusy = false;
    updateComfyRuntimeButton();
    setToggleBusy(changedBox, false);
  }
}

/** @type {number | null} */
let runtimeStatusPollTimer = null;
let runtimeStatusPollInFlight = false;

async function refreshComfyRuntimeStatus() {
  if (runtimeStatusPollInFlight || !invoke) return;

  // Poll less aggressively unless we are in a start transition.
  if (!state.comfyRuntimeStarting && document.visibilityState !== "visible") {
    return;
  }

  runtimeStatusPollInFlight = true;
  const wasStarting = Boolean(state.comfyRuntimeStarting);
  try {
    const result = await invoke("get_comfyui_runtime_status");
    state.comfyRuntimeRunning = Boolean(result?.running);
  } catch (_) {
    state.comfyRuntimeRunning = false;
  } finally {
    runtimeStatusPollInFlight = false;
  }

  // Keep "Starting..." visible until ComfyUI is actually running or explicit runtime events clear it.
  if (state.comfyRuntimeRunning) {
    state.comfyRuntimeStarting = false;
    state.comfyRuntimeTarget = "";
  } else if (!wasStarting) {
    state.comfyRuntimeStarting = false;
  }
  updateComfyRuntimeButton();
}

/** @param {number | null} [delayMs] */
function scheduleRuntimeStatusPoll(delayMs = null) {
  const delay = delayMs ?? (state.comfyRuntimeStarting ? 1400 : 6500);
  if (runtimeStatusPollTimer) {
    window.clearTimeout(runtimeStatusPollTimer);
  }
  runtimeStatusPollTimer = window.setTimeout(async () => {
    await refreshComfyRuntimeStatus().catch(() => {});
    scheduleRuntimeStatusPoll();
  }, delay);
}

async function openComfyWhenReady(timeoutMs = 45000) {
  const startedAt = Date.now();
  while ((Date.now() - startedAt) < timeoutMs) {
    try {
      const status = await invoke("get_comfyui_runtime_status");
      if (status?.running) {
        await invoke("open_external_url", { url: "http://127.0.0.1:8188" });
        return true;
      }
    } catch (_) {}
    await new Promise((resolve) => window.setTimeout(resolve, 450));
  }
  return false;
}

function updateComfyModeUi() {
  const installMode = state.comfyMode !== "manage";
  const hasSelectedInstall = Boolean(String(el.comfyExistingInstall?.value || "").trim());
  const canShowManageActions = !installMode && hasSelectedInstall;
  const switchingManagedInstall = Boolean(state.comfyInstallSwitchBusy);
  const shouldShowQuickActions = !installMode && canShowManageActions;
  el.comfyExistingRow?.classList.toggle("hidden", installMode);
  el.installComfyui?.classList.toggle("hidden", !installMode);
  el.comfyResumeBanner?.classList.toggle("hidden", !installMode || !state.comfyResumeState?.found);
  el.comfyQuickActions?.classList.toggle("hidden", !shouldShowQuickActions);
  el.comfyOpenInstallFolder?.classList.toggle("hidden", !canShowManageActions);
  el.comfyStartInstalled?.classList.toggle("hidden", !canShowManageActions);
  if (el.comfyMode) el.comfyMode.disabled = switchingManagedInstall;
  if (el.comfyExistingInstall) el.comfyExistingInstall.disabled = switchingManagedInstall;
  if (el.useExistingInstall) {
    el.useExistingInstall.disabled = switchingManagedInstall || !hasSelectedInstall;
  }
  updateComfyUpdateButton();
  if (el.comfyModeHelp) {
    el.comfyModeHelp.textContent = installMode
      ? "Install a new ComfyUI into the selected base folder"
      : "Manage add-ons and runtime for a selected installation";
  }
  if (el.comfyInstallRoot) {
    el.comfyInstallRoot.placeholder = installMode
      ? "Select base folder (e.g. Documents). App will create /ComfyUI inside it."
      : "Base folder containing ComfyUI installations";
  }
  if (el.comfyTorchProfile) {
    el.comfyTorchProfile.disabled = !installMode;
    el.comfyTorchProfile.title = installMode
      ? ""
      : "Torch stack is locked in Manage Existing mode.";
  }
}

/**
 * @template T
 * @param {string} message
 * @param {() => Promise<T>} work
 * @returns {Promise<T>}
 */
async function runWithManagedInstallOverlay(message, work) {
  state.comfyInstallSwitchBusy = true;
  updateComfyModeUi();
  showBlockingOverlay(message || "Switching managed ComfyUI...");
  await waitForNextPaint();
  try {
    return await work();
  } finally {
    state.comfyInstallSwitchBusy = false;
    updateComfyModeUi();
    hideStartupOverlay();
  }
}

/** @param {unknown} rootPath */
async function loadComfyExtraModelConfigForRoot(rootPath) {
  const root = normalizeSlashes(String(rootPath || "").trim());
  if (!root) return;
  try {
    const cfg = await invoke("get_comfyui_extra_model_config", { comfyuiRoot: root });
    if (cfg?.configured) {
      if (el.comfyExtraModelRoot) {
        el.comfyExtraModelRoot.value = String(cfg.base_path || "").trim();
      }
      if (el.comfyExtraModelDefault) {
        el.comfyExtraModelDefault.checked = Boolean(cfg.use_as_default);
      }
      state.sharedModelsRootDefault = String(cfg.base_path || "").trim();
      state.sharedModelsUseDefault = Boolean(cfg.use_as_default);
    } else {
      if (el.comfyExtraModelRoot) el.comfyExtraModelRoot.value = "";
      if (el.comfyExtraModelDefault) el.comfyExtraModelDefault.checked = false;
    }
  } catch (err) {
    logComfyLine(`Failed loading extra model path config: ${err}`);
  }
}

/** @param {unknown} rootPath */
async function persistComfyExtraModelConfigForRoot(rootPath) {
  const root = normalizeSlashes(String(rootPath || "").trim());
  if (!root) return;
  const extraModelRoot = String(el.comfyExtraModelRoot?.value || "").trim();
  const useAsDefault = Boolean(el.comfyExtraModelDefault?.checked && extraModelRoot);
  try {
    await invoke("set_comfyui_extra_model_config", {
      comfyuiRoot: root,
      extraModelRoot: extraModelRoot || null,
      useAsDefault,
    });
  } catch (err) {
    logComfyLine(`Failed to save extra model path config: ${err}`);
  }
}

/**
 * @param {string} basePath
 * @param {string | null} [preferredRoot]
 */
async function refreshExistingInstallations(basePath, preferredRoot = null) {
  const base = normalizeSlashes(basePath);
  /** @type {ArcticComfyInstall[]} */
  let installs = [];
  try {
    installs = await invoke("list_comfyui_installations", { basePath: base || null });
  } catch (_) {
    installs = [];
  }

  if (!el.comfyExistingInstall) return installs;
  const explicitPreferred = normalizeSlashes(String(preferredRoot || "").trim());
  const currentPreferred = explicitPreferred || normalizeSlashes(String(
    el.comfyRoot.value || el.comfyExistingInstall.value || "",
  ).trim());
  el.comfyExistingInstall.innerHTML = "";

  if (!installs.length) {
    state.comfyMode = "install";
    if (el.comfyMode) el.comfyMode.value = "install";
    const empty = document.createElement("option");
    empty.value = "";
    empty.textContent = "No detected installations";
    el.comfyExistingInstall.appendChild(empty);
    el.comfyExistingInstall.value = "";
    if (el.comfyStartInstalled) {
      el.comfyStartInstalled.dataset.path = "";
    }
    if (el.comfyOpenInstallFolder) {
      el.comfyOpenInstallFolder.dataset.path = "";
    }
    if (el.comfyRoot) el.comfyRoot.value = "";
    if (el.comfyRootLora) el.comfyRootLora.value = "";
    invoke("set_comfyui_root", { comfyuiRoot: "" }).catch(() => {});
    state.selectedComfyVersion = null;
    state.comfyUpdateAvailable = false;
    state.comfyUpdateChecked = false;
    resetComfySelectionsToDefaults();
    updateComfyModeUi();
    renderTitleMeta();
    return installs;
  }

  installs.forEach((item) => {
    const opt = document.createElement("option");
    opt.value = item.root;
    opt.textContent = `${item.name} - ${item.root}`;
    el.comfyExistingInstall.appendChild(opt);
  });

  const preferred = explicitPreferred
    ? installs.find((x) => normalizeSlashes(x.root) === explicitPreferred)
    : null;
  if (preferred) {
    el.comfyExistingInstall.value = preferred.root;
  } else {
    const fallback = state.comfyMode === "manage"
      ? newestComfyInstall(installs)
      : (installs.find((x) => normalizeSlashes(x.root) === currentPreferred) || installs[0]);
    const selected = fallback || installs[0];
    if (selected) el.comfyExistingInstall.value = selected.root;
  }
  updateComfyModeUi();
  refreshComfyUiUpdateStatus(el.comfyExistingInstall.value).catch(() => {});
  return installs;
}

/** @param {string} rootPath */
async function applySelectedExistingInstallation(rootPath) {
  const root = normalizeSlashes(rootPath);
  if (!root) return;
  setTorchRecommendedDetecting(true);
  el.comfyRoot.value = root;
  el.comfyRootLora.value = root;
  try {
    await invoke("set_comfyui_root", { comfyuiRoot: root });
    await loadInstalledAddonState(root);
    await loadComfyExtraModelConfigForRoot(root);
    await refreshEffectiveDownloadDestination();
    setComfyQuickActions(el.comfyInstallRoot.value, root);
    refreshComfyUiUpdateStatus(root).catch(() => {});
  } finally {
    setTorchRecommendedDetecting(false);
  }
}

/** @param {string | null} [rootPath] */
async function refreshComfyUiUpdateStatus(rootPath = null) {
  const root = normalizeSlashes(rootPath || el.comfyExistingInstall?.value || el.comfyRoot.value || "");
  state.comfyUpdateChecking = true;
  state.comfyUpdateChecked = false;
  state.comfyUpdateAvailable = false;
  state.comfyLatestVersion = null;
  state.selectedComfyVersion = null;
  updateComfyUpdateButton();
  renderTitleMeta();
  if (!root) return;
  try {
    const cacheKey = normalizeSlashes(root);
    const cached = comfyUpdateStatusCache.get(cacheKey);
    let status;
    if (cached && (Date.now() - cached.at) < COMFY_UPDATE_STATUS_CACHE_MS) {
      status = cached.status;
    } else if (comfyUpdateStatusInflight.has(cacheKey)) {
      status = await comfyUpdateStatusInflight.get(cacheKey);
    } else {
      const request = invoke("get_comfyui_update_status", { comfyuiRoot: root })
        .then((result) => {
          comfyUpdateStatusCache.set(cacheKey, { status: result, at: Date.now() });
          return result;
        })
        .finally(() => comfyUpdateStatusInflight.delete(cacheKey));
      comfyUpdateStatusInflight.set(cacheKey, request);
      status = await request;
    }
    state.comfyUpdateChecked = Boolean(status?.checked);
    state.comfyUpdateAvailable = Boolean(status?.update_available);
    state.comfyLatestVersion = status?.latest_version || null;
    const detailTextRaw = String(status?.detail || "");
    const headMatchesTag = Boolean(status?.head_matches_latest_tag);
    state.selectedComfyVersion = headMatchesTag
      ? (status?.latest_version || status?.installed_version || null)
      : (status?.installed_version || null);
    updateComfyUpdateButton();
    renderTitleMeta();
    if (status?.detail) {
      const detailText = detailTextRaw;
      const detailKey = `${normalizeSlashes(root)}::${detailText}`;
      if (state.comfyLastUpdateDetailLogKey !== detailKey) {
        logComfyLine(detailText);
        state.comfyLastUpdateDetailLogKey = detailKey;
      }
    }
  } catch (err) {
    state.comfyUpdateChecked = false;
    state.comfyUpdateAvailable = false;
    state.comfyLatestVersion = null;
    state.selectedComfyVersion = null;
    renderTitleMeta();
    logComfyLine(`ComfyUI update check failed: ${err}`);
  } finally {
    state.comfyUpdateChecking = false;
    updateComfyUpdateButton();
  }
}

/**
 * @param {string} selectedPath
 * @param {boolean} [persistInstallBase]
 * @param {boolean} [keepCurrentMode]
 * @param {boolean} [emitDetectionLog]
 */
async function syncComfyInstallSelection(
  selectedPath,
  persistInstallBase = true,
  keepCurrentMode = false,
  emitDetectionLog = true,
) {
  const selected = normalizeSlashes(selectedPath);
  if (!selected) return;
  try {
    const inspection = await invoke("inspect_comfyui_path", { path: selected });
    const detectedRoot = normalizeSlashes(inspection?.detected_root || "");
    const normalizedSelected = normalizeSlashes(inspection?.selected || selected);

    if (detectedRoot) {
      // If user picked an existing ComfyUI root directly, keep install base as its parent.
      const pickedRootDirectly = normalizeSlashes(detectedRoot) === normalizeSlashes(normalizedSelected);
      const baseForInstall = pickedRootDirectly
        ? parentDir(detectedRoot)
        : normalizedSelected;
      el.comfyInstallRoot.value = baseForInstall;
      if (persistInstallBase) {
        await invoke("set_comfyui_install_base", { comfyuiInstallBase: baseForInstall });
      }
      const installs = await refreshExistingInstallations(
        baseForInstall,
        pickedRootDirectly ? detectedRoot : null,
      );
      if (!keepCurrentMode) {
        state.comfyMode = "manage";
        if (el.comfyMode) el.comfyMode.value = "manage";
      }
      updateComfyModeUi();
      if (pickedRootDirectly || installs.length === 1) {
        await applySelectedExistingInstallation(detectedRoot);
      } else if (installs.length > 1 && state.comfyMode === "manage") {
        await applySelectedExistingInstallation(el.comfyExistingInstall.value);
      }
      setComfyQuickActions(baseForInstall, detectedRoot);
      if (emitDetectionLog) {
        logComfyLine(`Detected ComfyUI install: ${detectedRoot}`);
      }
      await refreshComfyRuntimeStatus();
      if (emitDetectionLog && state.comfyRuntimeRunning) {
        logComfyLine("Detected running ComfyUI server. If you want to start a different one, stop this server first.");
      }
      return;
    }

    el.comfyInstallRoot.value = normalizedSelected;
    if (persistInstallBase) {
      await invoke("set_comfyui_install_base", { comfyuiInstallBase: normalizedSelected });
    }
    if (state.comfyMode !== "manage") {
      resetComfySelectionsToDefaults();
    }
    const installs = await refreshExistingInstallations(normalizedSelected);
    if (installs.length > 0) {
      if (!keepCurrentMode) {
        state.comfyMode = "manage";
        if (el.comfyMode) el.comfyMode.value = "manage";
      }
      updateComfyModeUi();
      const latest = newestComfyInstall(installs) || installs[0];
      if (latest?.root) {
        await applySelectedExistingInstallation(latest.root);
      }
    }
    await refreshComfyResumeState();
  } catch (_) {
    el.comfyInstallRoot.value = selected;
    if (persistInstallBase) {
      await invoke("set_comfyui_install_base", { comfyuiInstallBase: selected });
    }
    if (state.comfyMode !== "manage") {
      resetComfySelectionsToDefaults();
    }
    const installs = await refreshExistingInstallations(selected);
    if (installs.length > 0) {
      state.comfyMode = "manage";
      if (el.comfyMode) el.comfyMode.value = "manage";
      updateComfyModeUi();
      const latest = newestComfyInstall(installs) || installs[0];
      if (latest?.root) {
        await applySelectedExistingInstallation(latest.root);
      }
    }
    await refreshComfyResumeState();
  }
}

/** @param {ArcticRecord | null} result */
function renderPreflight(result) {
  if (!el.preflightList || !el.preflightSummary) return;
  const items = Array.isArray(result?.items) ? result.items : [];
  const profileSummary = result?.torchProfileLabel ? `Profile: ${result.torchProfileLabel}.` : "";
  el.preflightList.innerHTML = "";
  if (!items.length) {
    const msg = document.createElement("div");
    msg.className = "empty-msg";
    msg.textContent = "No checks executed yet.";
    el.preflightList.appendChild(msg);
    el.preflightSummary.textContent = profileSummary || "Not run yet.";
    state.comfyPreflightOk = null;
    return;
  }

  items.forEach((item) => {
    const row = document.createElement("div");
    row.className = `preflight-item ${String(item.status || "warn").toLowerCase()}`;
    const status = document.createElement("div");
    status.className = "status";
    status.textContent = String(item.status || "warn").toUpperCase();
    const text = document.createElement("div");
    text.textContent = `${item.title}: ${item.detail}`;
    row.appendChild(status);
    row.appendChild(text);
    el.preflightList.appendChild(row);
  });

  state.comfyPreflightOk = Boolean(result?.ok);
  const baseSummary = result?.summary || (state.comfyPreflightOk ? "Preflight passed." : "Preflight has issues.");
  el.preflightSummary.textContent = profileSummary ? `${baseSummary} ${profileSummary}` : baseSummary;
}

async function runComfyPreflight() {
  try {
    const request = buildComfyInstallRequest();
    const profileLabel = torchProfileLabel(request.torchProfile);
    logComfyLine(
      state.comfyMode === "manage"
        ? `Running preflight for the selected install using ${profileLabel}.`
        : `Running preflight using ${profileLabel}.`,
    );
    const result = await invoke("run_comfyui_preflight", { request });
    renderPreflight({ ...result, torchProfileLabel: profileLabel });
    return result;
  } catch (err) {
    renderPreflight({
      ok: false,
      summary: "Preflight failed to run.",
      torchProfileLabel: torchProfileLabel(el.comfyTorchProfile?.value || ""),
      items: [{ status: "fail", title: "Preflight runtime", detail: String(err) }],
    });
    return null;
  }
}

async function refreshComfyResumeState() {
  try {
    const installBase = String(el.comfyInstallRoot.value || "").trim() || null;
    const result = await invoke("get_comfyui_resume_state", { installBase });
    state.comfyResumeState = result || null;
    if (!result?.found) {
      el.comfyResumeBanner?.classList.add("hidden");
      updateComfyModeUi();
      return;
    }
    if (el.comfyResumeText) {
      el.comfyResumeText.textContent = result.summary || "Interrupted install found.";
    }
    el.comfyResumeBanner?.classList.remove("hidden");
    updateComfyModeUi();
  } catch (_) {
    state.comfyResumeState = null;
    el.comfyResumeBanner?.classList.add("hidden");
    updateComfyModeUi();
  }
}

/** @param {boolean} forceFresh */
async function startComfyInstall(forceFresh) {
  if (state.comfyInstallBusy) {
    const cancelled = await invoke("cancel_comfyui_install");
    if (cancelled) {
      logComfyLine("ComfyUI installation cancellation requested.");
    } else {
      logComfyLine("No active ComfyUI installation.");
    }
    return;
  }
  const root = String(el.comfyInstallRoot.value || "").trim();
  if (!root) {
    logComfyLine("Select install folder first.");
    return;
  }

  await refreshComfyRuntimeStatus();
  if (state.comfyRuntimeRunning) {
    logComfyLine("Detected running ComfyUI server. Stopping it before install...");
    try {
      await invoke("stop_comfyui_root");
    } catch (err) {
      logComfyLine(`Failed to stop running ComfyUI before install: ${err}`);
      return;
    }
    await refreshComfyRuntimeStatus();
    if (state.comfyRuntimeRunning) {
      logComfyLine("ComfyUI is still running. Stop it first, then retry install.");
      return;
    }
    logComfyLine("ComfyUI server stopped. Proceeding with install.");
  }

  const preflight = await runComfyPreflight();
  if (!preflight || !preflight.ok) {
    logComfyLine("Preflight has blocking issues. Resolve them before install.");
    return;
  }
  state.comfyInstallBusy = true;
  updateComfyInstallButton();
  logComfyLine(forceFresh ? "Starting fresh ComfyUI installation..." : "Starting ComfyUI installation...");
  try {
    const request = buildComfyInstallRequest();
    request.forceFresh = Boolean(forceFresh);
    await invoke("start_comfyui_install", { request });
    logComfyLine("ComfyUI installation started.");
  } catch (err) {
    state.comfyInstallBusy = false;
    updateComfyInstallButton();
    logComfyLine(`ComfyUI install failed to start: ${err}`);
  }
}

function applyComfyAddonRules() {
  const profile = String(el.comfyTorchProfile?.value || "").trim();
  const profileBackend = torchProfileBackend(profile);
  const nonCudaSelected = Boolean(profileBackend) && profileBackend !== "nvidia";

  if (el.addonSageAttention3) {
    const wasChecked = el.addonSageAttention3.checked;
    el.addonSageAttention3.disabled = !state.comfySage3Eligible;
    if (!state.comfySage3Eligible && wasChecked) {
      el.addonSageAttention3.checked = false;
    }
  }

  if (el.addonTrellis2) {
    const trellisAllowed = profile === "torch280_cu128";
    const wasChecked = el.addonTrellis2.checked;
    el.addonTrellis2.disabled = !trellisAllowed;
    if (!trellisAllowed && wasChecked) {
      el.addonTrellis2.checked = false;
    }
  }

  [
    el.addonSageAttention,
    el.addonSageAttention3,
    el.addonFlashAttention,
    el.addonNunchaku,
  ].forEach((box) => {
    if (!box) return;
    const wasChecked = box.checked;
    const eligibilityDisabled = box === el.addonSageAttention3 && !state.comfySage3Eligible;
    box.disabled = nonCudaSelected || eligibilityDisabled;
    if (nonCudaSelected && wasChecked) {
      box.checked = false;
    }
  });

  [el.flagSageAttention, el.flagFlashAttention].forEach((box) => {
    if (!box) return;
    const wasChecked = box.checked;
    box.disabled = nonCudaSelected;
    if (nonCudaSelected && wasChecked) {
      box.checked = false;
    }
  });

  if (el.addonNunchaku && el.addonInsightFace) {
    const nunchakuSelected = Boolean(el.addonNunchaku.checked);
    if (nunchakuSelected) {
      el.addonInsightFace.checked = true;
      el.addonInsightFace.disabled = true;
    } else {
      el.addonInsightFace.disabled = false;
    }
  }
}



  return {
    applyAttentionBackendFromToggle,
    applyComfyAddonRules,
    applyComponentToggleFromCheckbox,
    applyLaunchAttentionFlagFromToggle,
    applySelectedExistingInstallation,
    clearComfyUpdateStatusCache,
    comfyInstallNameFromRoot,
    loadInstalledAddonState,
    newestComfyInstall,
    openComfyWhenReady,
    persistComfyExtraModelConfigForRoot,
    refreshComfyResumeState,
    refreshComfyRuntimeStatus,
    refreshComfyUiUpdateStatus,
    refreshExistingInstallations,
    renderPreflight,
    resetComfySelectionsToDefaults,
    runComfyPreflight,
    runWithManagedInstallOverlay,
    scheduleRuntimeStatusPoll,
    setComfyQuickActions,
    startComfyInstall,
    syncComfyInstallSelection,
    updateComfyInstallButton,
    updateComfyModeUi,
    updateComfyRuntimeButton,
    updateComfyUpdateButton,
  };
}
