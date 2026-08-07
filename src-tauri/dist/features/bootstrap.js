import {
  comfyTorchProfiles,
  DOT_SEP,
  el,
  invoke,
  setComfyTorchProfiles,
  state,
} from "../lib/app-context.js";
import { formatVramMbToGb } from "../lib/display-format.js";
import { normalizeSlashes, parentDir } from "../lib/path.js";

/** @param {ArcticBootstrapDependencies} dependencies */
export function createBootstrapController({
  applyComfyAddonRules,
  applyComfyTorchProfileOptions,
  catalogHasContent,
  familyOptions,
  isRocmTorchProfile,
  loadInstalledAddonState,
  loadLoraMetadata,
  logComfyLine,
  logLine,
  loraFamilyOptions,
  normalizeRamTier,
  platformLabel,
  preferredTorchProfile,
  refreshComfyResumeState,
  refreshComfyRuntimeStatus,
  refreshEffectiveDownloadDestination,
  refreshExistingInstallations,
  refreshGpuSelectionOptions,
  refreshLoraSelectors,
  refreshRocmGuidedStatus,
  refreshWorkflowSelectors,
  renderAppVersionTag,
  renderCatalogStatus,
  renderModelSelectionList,
  renderPreflight,
  renderTitleMeta,
  runComfyPreflight,
  setCatalogLoading,
  setComfyQuickActions,
  setOptions,
  setStartupStatus,
  setTorchRecommendedDetecting,
  syncComfyInstallSelection,
  updateComfyModeUi,
  updateRamTierOptions,
  updateRocmGuidedUi,
  vramOptionsWithPlaceholder,
  vramTierForMb,
  workflowFamilyOptions,
}) {
async function bootstrap() {
  if (!invoke) {
    logLine("Tauri invoke bridge unavailable.");
    return;
  }
  setStartupStatus("Loading settings and catalog...");
  setCatalogLoading(true);
  const [settings, catalog, platformCapabilities] = /** @type {[
    ArcticRecord,
    ArcticCatalog,
    ArcticPlatformCapabilities,
  ]} */ (await Promise.all([
    invoke("get_settings"),
    invoke("get_catalog"),
    invoke("get_platform_capabilities"),
  ]));

  state.settings = settings;
  state.catalog = catalog;
  state.platformCapabilities = platformCapabilities;
  if (Array.isArray(platformCapabilities?.torch_profiles) && platformCapabilities.torch_profiles.length) {
    setComfyTorchProfiles(platformCapabilities.torch_profiles.map(({ value, label, backend }) => ({
      value: String(value),
      label: String(label),
      backend: String(backend),
    })));
  }
  state.comfyGpuSelection = String(settings.comfyui_gpu_selection || "auto").trim().toLowerCase();

  state.appVersion = settings?.last_installed_version || state.appVersion || "";
  state.titleSystemText = "Loading system info...";
  renderAppVersionTag();
  renderTitleMeta();
  const refreshSnapshot = (attempt = 0, gen = ++state.comfySnapshotSeq) => {
    invoke("get_app_snapshot")
      .then((snapshot) => {
        if (gen !== state.comfySnapshotSeq) return; // superseded by a newer call
        const ramRaw = Number(snapshot.total_ram_gb);
        const ramGb = Number.isFinite(ramRaw) ? (ramRaw > 1000 ? ramRaw / 1000 : ramRaw) : null;
        const ramText = `${ramGb != null ? ramGb.toFixed(1) : "?"} GB RAM`;
        state.detectedRamGb = ramGb;
        state.detectedRamTier = normalizeRamTier(snapshot.ram_tier);
        state.detectedVramTier = vramTierForMb(snapshot.nvidia_gpu_vram_mb);
        renderModelSelectionList();
        const amdGpu = String(snapshot.amd_gpu_name || "").trim();
        const nvidiaGpu = String(snapshot.nvidia_gpu_name || "").trim();
        const intelGpu = String(snapshot.intel_gpu_name || "").trim();
        refreshGpuSelectionOptions(snapshot);
        const detectedGpus = [];
        if (nvidiaGpu) {
          const vram = formatVramMbToGb(snapshot.nvidia_gpu_vram_mb);
          detectedGpus.push(`NVIDIA: ${nvidiaGpu}${vram ? ` (${vram})` : ""}`);
        }
        if (amdGpu) detectedGpus.push(`AMD: ${amdGpu}`);
        if (intelGpu) detectedGpus.push(`Intel: ${intelGpu}`);
        const gpuText = detectedGpus.length ? detectedGpus.join(" + ") : "GPU: Not detected";
        state.appVersion = snapshot.version || state.appVersion;
        state.titleSystemText = `${ramText}${DOT_SEP}${gpuText}`;
        applyComfyTorchProfileOptions(el.comfyTorchProfile?.value || null);
        applyComfyAddonRules();
        updateRocmGuidedUi();
        if ((amdGpu || intelGpu
          || isRocmTorchProfile(el.comfyTorchProfile?.value)
          || String(el.comfyTorchProfile?.value || "").trim() === "torch291_xpu") && !state.rocmGuidedStatus) {
          refreshRocmGuidedStatus(false).catch(() => {});
        }
        renderAppVersionTag();
        renderTitleMeta();
        if ((snapshot.gpu_detection_pending || (!amdGpu && !nvidiaGpu && !intelGpu)) && attempt < 8) {
          setTimeout(() => refreshSnapshot(attempt + 1, gen), 600);
        }
      })
      .catch(() => {});
  };
  refreshSnapshot();

  el.comfyRoot.value = settings.comfyui_root || "";
  el.comfyRootLora.value = settings.comfyui_root || "";
  if (el.comfyRootWorkflow) {
    el.comfyRootWorkflow.value = settings.comfyui_root || "";
  }
  el.comfyInstallRoot.value = settings.comfyui_install_base || "";
  state.sharedModelsRootDefault = String(settings.shared_models_root || "").trim();
  state.sharedModelsUseDefault = Boolean(
    settings.shared_models_use_default
    || (state.sharedModelsRootDefault && settings.shared_models_use_default !== false),
  );
  if (el.comfyExtraModelRoot) {
    el.comfyExtraModelRoot.value = state.sharedModelsRootDefault;
  }
  if (el.comfyExtraModelDefault) {
    el.comfyExtraModelDefault.checked = state.sharedModelsRootDefault
      ? Boolean(state.sharedModelsUseDefault)
      : false;
  }
  if (el.comfyMode) {
    state.comfyMode = (settings.comfyui_root ? "manage" : "install");
    el.comfyMode.value = state.comfyMode;
  }
  el.civitaiToken.value = settings.civitai_token || "";
  if (el.addonPinnedMemory) {
    el.addonPinnedMemory.checked = settings.comfyui_pinned_memory_enabled !== false;
  }
  if (el.flagSageAttention) {
    el.flagSageAttention.checked = (
      settings.comfyui_attention_backend === "sage"
      || settings.comfyui_attention_backend === "sage3"
    );
  }
  if (el.flagFlashAttention) {
    el.flagFlashAttention.checked = settings.comfyui_attention_backend === "flash";
  }
  if (el.launchListen) {
    el.launchListen.checked = settings.comfyui_listen_enabled === true;
  }
  if (el.flagLowvram) {
    el.flagLowvram.checked = settings.comfyui_lowvram_enabled === true;
  }
  if (el.flagBf16Unet) {
    el.flagBf16Unet.checked = settings.comfyui_bf16_unet_enabled === true;
  }
  if (el.flagAsyncOffload) {
    el.flagAsyncOffload.checked = settings.comfyui_async_offload_enabled === true;
  }
  if (el.flagDisableSmartMemory) {
    el.flagDisableSmartMemory.checked = settings.comfyui_disable_smart_memory_enabled === true;
  }
  if (el.comfyCustomLaunchArgs) {
    el.comfyCustomLaunchArgs.value = settings.comfyui_custom_launch_args || "";
  }
  if (el.comfyShowRuntimeLogs) {
    el.comfyShowRuntimeLogs.checked = settings.comfyui_show_runtime_logs !== false;
  }
  if (el.enableHfXet) {
    el.enableHfXet.checked = settings.hf_xet_enabled === true;
  }
  setComfyQuickActions(settings.comfyui_last_install_dir || "", settings.comfyui_root || "");
  applyComfyTorchProfileOptions();
  const savedTorchProfile = String(settings.comfyui_torch_profile || "").trim();
  if (
    savedTorchProfile
    && comfyTorchProfiles.some((x) => x.value === savedTorchProfile)
    && state.comfyGpuSelection === "auto"
  ) {
    el.comfyTorchProfile.value = savedTorchProfile;
    state.comfyTorchProfileLocked = true;
  }
  updateRocmGuidedUi();

  const refreshRecommendation = (attempt = 0, gen = ++state.comfyRecommendationSeq) => {
    invoke("get_comfyui_install_recommendation", {
      gpuSelection: state.comfyGpuSelection === "auto" ? null : state.comfyGpuSelection,
    })
      .then((reco) => {
        if (gen !== state.comfyRecommendationSeq) return; // superseded by a newer call
        state.comfyTorchRecommendedBase = reco.gpu_name
          ? `${platformLabel()} recommendation: '${reco.torch_label}' for ${reco.gpu_name}`
          : `${platformLabel()} recommendation: '${reco.torch_label}' for the selected GPU`;
        setTorchRecommendedDetecting(false);
        state.comfySage3Eligible = String(reco.gpu_name || "").toLowerCase().includes("rtx 50");
        if (
          comfyTorchProfiles.some((x) => x.value === reco.torch_profile)
          && !state.comfyTorchProfileLocked
        ) {
          applyComfyTorchProfileOptions(reco.torch_profile);
          el.comfyTorchProfile.value = reco.torch_profile;
        }
        applyComfyAddonRules();
        if ((reco.detection_pending || !reco.gpu_name) && attempt < 8) {
          setTimeout(() => refreshRecommendation(attempt + 1, gen), 600);
        }
      })
      .catch((err) => {
        if (gen !== state.comfyRecommendationSeq) return; // superseded by a newer call
        state.comfyTorchRecommendedBase = `${platformLabel()} recommendation unavailable. Choose the Torch profile manually.`;
        setTorchRecommendedDetecting(false);
        if (["amd", "intel"].includes(state.comfyDetectedGpuVendor)) {
          const fallback = preferredTorchProfile(state.comfyDetectedGpuVendor);
          applyComfyTorchProfileOptions(fallback);
          el.comfyTorchProfile.value = fallback;
        } else if (!state.comfyTorchProfileLocked) {
          el.comfyTorchProfile.value = "torch280_cu128";
        }
        state.comfySage3Eligible = false;
        applyComfyAddonRules();
        logComfyLine(`Recommendation detection failed: ${err}`);
      });
  };
  state.comfyRefreshRecommendation = refreshRecommendation;
  refreshRecommendation();

  const initialInstallRoot = String(el.comfyInstallRoot?.value || "").trim();
  if (initialInstallRoot) {
    setStartupStatus("Running startup preflight checks...");
    setTimeout(() => {
      runComfyPreflight().catch(() => {});
    }, 0);
  } else {
    renderPreflight(null);
  }
  setStartupStatus("Scanning ComfyUI installations...");
  if (settings.comfyui_install_base) {
    let effectiveBase = normalizeSlashes(settings.comfyui_install_base);
    el.comfyInstallRoot.value = effectiveBase;
    await invoke("set_comfyui_install_base", { comfyuiInstallBase: effectiveBase }).catch(() => {});
    try {
      const inspection = await invoke("inspect_comfyui_path", { path: effectiveBase });
      const selectedNorm = normalizeSlashes(inspection?.selected || effectiveBase);
      const detectedNorm = normalizeSlashes(inspection?.detected_root || "");
      const leaf = selectedNorm.split("\\").pop() || "";
      const looksLikeComfyInstall = /^comfyui(?:-\d+)?$/i.test(leaf);
      if (
        looksLikeComfyInstall &&
        detectedNorm &&
        normalizeSlashes(detectedNorm) === selectedNorm
      ) {
        const parent = parentDir(selectedNorm);
        if (parent && parent !== selectedNorm) {
          effectiveBase = normalizeSlashes(parent);
          el.comfyInstallRoot.value = effectiveBase;
          await invoke("set_comfyui_install_base", { comfyuiInstallBase: effectiveBase }).catch(() => {});
          logComfyLine(`Adjusted install base to parent folder: ${effectiveBase}`);
        }
      }
    } catch (_) {}
    await syncComfyInstallSelection(effectiveBase, false, true, false);
  } else if (settings.comfyui_root) {
    const inferredBase = parentDir(settings.comfyui_root);
    el.comfyInstallRoot.value = inferredBase;
    await invoke("set_comfyui_install_base", { comfyuiInstallBase: inferredBase }).catch(() => {});
    await refreshExistingInstallations(inferredBase, null);
  } else {
    await refreshExistingInstallations("", null);
  }
  await refreshComfyResumeState();
  setStartupStatus("Checking ComfyUI runtime status...");
  await refreshComfyRuntimeStatus();
  updateComfyModeUi();
  setTimeout(() => {
    loadInstalledAddonState(el.comfyRoot.value || "").catch(() => {});
  }, 0);

  applyCatalogSnapshot(catalog, { resetSelectors: true });
  try {
    setStartupStatus("Checking downloader acceleration...");
    const xet = await invoke("get_hf_xet_preflight");
    if (xet?.detail) {
      logLine(xet.detail);
    }
  } catch (_) {}
  setStartupStatus("Starting UI...");
}

/**
 * @param {ArcticCatalog} catalog
 * @param {{ resetSelectors?: boolean }} [options]
 */
function applyCatalogSnapshot(catalog, { resetSelectors = false } = {}) {
  state.catalog = catalog;
  state.catalogLoading = false;
  state.catalogError = catalogHasContent(catalog)
    ? ""
    : "Catalog unavailable. Check your connection and Supabase configuration.";
  const currentModelFamily = resetSelectors ? "" : String(el.modelFamily?.value || "").trim();
  const currentVramTier = resetSelectors ? "" : String(el.vramTier?.value || "").trim();
  const currentLoraFamily = resetSelectors ? null : String(el.loraFamily?.value || "").trim();
  const currentWorkflowFamily = resetSelectors ? null : String(el.workflowFamily?.value || "").trim();

  setOptions(el.modelFamily, familyOptions(catalog.models), currentModelFamily);
  setOptions(el.vramTier, vramOptionsWithPlaceholder(), currentVramTier);
  updateRamTierOptions();
  renderModelSelectionList();
  refreshEffectiveDownloadDestination().catch(() => {});

  setOptions(el.loraFamily, loraFamilyOptions(catalog.loras), currentLoraFamily);
  refreshLoraSelectors();
  setTimeout(() => {
    loadLoraMetadata().catch(() => {});
  }, 0);

  setOptions(el.workflowFamily, workflowFamilyOptions(catalog.workflows || []), currentWorkflowFamily);
  refreshWorkflowSelectors();
  renderCatalogStatus();

  logLine(`Loaded ${catalog.models?.length || 0} models, ${catalog.loras?.length || 0} LoRAs, and ${catalog.workflows?.length || 0} workflows.`);
}

  return { applyCatalogSnapshot, bootstrap };
}
