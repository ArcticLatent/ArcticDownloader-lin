import {
  ALWAYS_ONLY_VARIANT_ID,
  comfyTorchProfiles,
  el,
  invoke,
  listen,
  state,
} from "../lib/app-context.js";
import { normalizeSlashes, parentDir } from "../lib/path.js";
import { debounce } from "../lib/timing.js";
import { isSafeHttpUrl } from "../lib/url.js";

export function registerEventHandlers({
  addCompleted,
  applyAttentionBackendFromToggle,
  applyCatalogSnapshot,
  applyComfyAddonRules,
  applyComfyTorchProfileOptions,
  applyComponentToggleFromCheckbox,
  applyLaunchAttentionFlagFromToggle,
  applySelectedExistingInstallation,
  beginBusyDownload,
  clearComfyUpdateStatusCache,
  comfyInstallNameFromRoot,
  currentGuidedAccelTarget,
  endBusyDownload,
  ensureProgressSmoother,
  filteredModelsForCurrentSelection,
  hideStartupOverlay,
  isRocmTorchProfile,
  loadInstalledAddonState,
  loadLoraMetadata,
  loadWorkflowPreview,
  logComfyLine,
  logComfyRuntimeLine,
  logLine,
  modelHasAlwaysArtifacts,
  newestComfyInstall,
  openComfyWhenReady,
  persistComfyExtraModelConfigForRoot,
  recommendedVariantIdForModel,
  refreshComfyResumeState,
  refreshComfyRuntimeStatus,
  refreshComfyUiUpdateStatus,
  refreshEffectiveDownloadDestination,
  refreshExistingInstallations,
  refreshLoraSelectors,
  refreshRocmGuidedStatus,
  refreshWorkflowSelectors,
  renderActiveTransfers,
  renderComfyRuntimeLogs,
  renderCompletedTransfers,
  renderModelSelectionList,
  renderOverallProgress,
  renderTransfers,
  requestCancelDownload,
  resetComfySelectionsToDefaults,
  runComfyPreflight,
  runWithManagedInstallOverlay,
  selectedArtifactKeysForDownload,
  selectedGpuVendor,
  selectedModelItems,
  selectedRamTierValue,
  selectedVramTierValue,
  selectedWorkflow,
  setCatalogLoading,
  setComfyQuickActions,
  setProgress,
  setSelectedModelVariant,
  setStartupStatus,
  showBlockingOverlay,
  startComfyInstall,
  switchTab,
  syncComfyInstallSelection,
  updateComfyInstallButton,
  updateComfyModeUi,
  updateComfyRuntimeButton,
  updateComfyUpdateButton,
  updateRocmGuidedUi,
  updateUpdateButton,
  workflowExternalUrl,
}) {
el.tabComfyui.addEventListener("click", () => switchTab("comfyui"));
el.tabModels.addEventListener("click", () => switchTab("models"));
el.tabLoras.addEventListener("click", () => switchTab("loras"));
el.tabWorkflows.addEventListener("click", () => switchTab("workflows"));

el.modelFamily.addEventListener("change", renderModelSelectionList);
el.vramTier.addEventListener("change", renderModelSelectionList);
el.ramTier.addEventListener("change", renderModelSelectionList);
// Debounced: renderModelSelectionList() filters the whole catalog and
// rebuilds the selection list + queue DOM on every call, which is wasteful
// (and visibly janky on a large catalog) if run on every single keystroke.
el.modelSearch?.addEventListener("input", debounce(renderModelSelectionList, 200));
el.selectVisibleModels?.addEventListener("click", () => {
  const vramTier = selectedVramTierValue();
  filteredModelsForCurrentSelection().forEach((model) => {
    const recommendedVariantId = recommendedVariantIdForModel(model, vramTier);
    if (recommendedVariantId) {
      setSelectedModelVariant(model.id, recommendedVariantId);
    } else if (modelHasAlwaysArtifacts(model)) {
      setSelectedModelVariant(model.id, ALWAYS_ONLY_VARIANT_ID);
    }
  });
  renderModelSelectionList();
});
el.clearModelSelection?.addEventListener("click", () => {
  state.selectedModelVariants.clear();
  state.manuallySelectedModelVariants.clear();
  state.selectedModelArtifactChoices.clear();
  renderModelSelectionList();
});
el.refreshCatalog?.addEventListener("click", async () => {
  if (!invoke) return;
  const originalLabel = el.refreshCatalog.textContent;
  try {
    setCatalogLoading(true);
    el.refreshCatalog.textContent = "Refreshing...";
    el.refreshCatalog.disabled = true;
    showBlockingOverlay("Refreshing catalog...");
    const catalog = await invoke("refresh_catalog");
    applyCatalogSnapshot(catalog);
    logLine("Catalog refreshed from Supabase.");
  } catch (err) {
    setCatalogLoading(false, "Catalog refresh failed. Check your connection and Supabase configuration.");
    renderModelSelectionList();
    logLine(`Catalog refresh failed: ${err}`);
  } finally {
    el.refreshCatalog.textContent = originalLabel;
    el.refreshCatalog.disabled = false;
    hideStartupOverlay();
  }
});

el.loraFamily.addEventListener("change", () => {
  refreshLoraSelectors();
  loadLoraMetadata().catch((err) => logLine(String(err)));
});
el.loraId.addEventListener("change", () => {
  loadLoraMetadata().catch((err) => logLine(String(err)));
});
el.workflowFamily.addEventListener("change", refreshWorkflowSelectors);
el.workflowId.addEventListener("change", loadWorkflowPreview);

el.saveRoot.addEventListener("click", async () => {
  try {
    await invoke("set_comfyui_root", { comfyuiRoot: el.comfyRoot.value });
    el.comfyRootLora.value = el.comfyRoot.value;
    if (el.comfyRootWorkflow) {
      el.comfyRootWorkflow.value = el.comfyRoot.value;
    }
    await loadInstalledAddonState(el.comfyRoot.value);
    await refreshEffectiveDownloadDestination();
    const original = el.saveRoot.textContent;
    el.saveRoot.textContent = "Saved";
    el.saveRoot.disabled = true;
    window.setTimeout(() => {
      el.saveRoot.textContent = original || "Save Folder";
      el.saveRoot.disabled = false;
    }, 900);
  } catch (err) {
    logLine(`Save folder failed: ${err}`);
  }
});

el.chooseRoot.addEventListener("click", async () => {
  try {
    const selected = await invoke("pick_folder", { title: "Choose ComfyUI folder" });
    if (!selected) return;
    el.comfyRoot.value = selected;
    await invoke("set_comfyui_root", { comfyuiRoot: selected });
    el.comfyRootLora.value = selected;
    if (el.comfyRootWorkflow) {
      el.comfyRootWorkflow.value = selected;
    }
    logLine("ComfyUI folder selected.");
    await loadInstalledAddonState(selected);
    await refreshEffectiveDownloadDestination();
  } catch (err) {
    logLine(`Choose folder failed: ${err}`);
  }
});

el.saveRootLora.addEventListener("click", async () => {
  try {
    await invoke("set_comfyui_root", { comfyuiRoot: el.comfyRootLora.value });
    el.comfyRoot.value = el.comfyRootLora.value;
    if (el.comfyRootWorkflow) {
      el.comfyRootWorkflow.value = el.comfyRootLora.value;
    }
    await loadInstalledAddonState(el.comfyRoot.value);
    await refreshEffectiveDownloadDestination();
    const original = el.saveRootLora.textContent;
    el.saveRootLora.textContent = "Saved";
    el.saveRootLora.disabled = true;
    window.setTimeout(() => {
      el.saveRootLora.textContent = original || "Save Folder";
      el.saveRootLora.disabled = false;
    }, 900);
  } catch (err) {
    logLine(`Save folder failed: ${err}`);
  }
});

el.chooseRootLora.addEventListener("click", async () => {
  try {
    const selected = await invoke("pick_folder", { title: "Choose ComfyUI folder" });
    if (!selected) return;
    el.comfyRootLora.value = selected;
    await invoke("set_comfyui_root", { comfyuiRoot: selected });
    el.comfyRoot.value = selected;
    if (el.comfyRootWorkflow) {
      el.comfyRootWorkflow.value = selected;
    }
    logLine("ComfyUI folder selected.");
    await loadInstalledAddonState(selected);
    await refreshEffectiveDownloadDestination();
  } catch (err) {
    logLine(`Choose folder failed: ${err}`);
  }
});

el.saveRootWorkflow?.addEventListener("click", async () => {
  try {
    await invoke("set_comfyui_root", { comfyuiRoot: el.comfyRootWorkflow.value });
    el.comfyRoot.value = el.comfyRootWorkflow.value;
    el.comfyRootLora.value = el.comfyRootWorkflow.value;
    await loadInstalledAddonState(el.comfyRoot.value);
    await refreshEffectiveDownloadDestination();
    const original = el.saveRootWorkflow.textContent;
    el.saveRootWorkflow.textContent = "Saved";
    el.saveRootWorkflow.disabled = true;
    window.setTimeout(() => {
      el.saveRootWorkflow.textContent = original || "Save Folder";
      el.saveRootWorkflow.disabled = false;
    }, 900);
  } catch (err) {
    logLine(`Save folder failed: ${err}`);
  }
});

el.chooseRootWorkflow?.addEventListener("click", async () => {
  try {
    const selected = await invoke("pick_folder", { title: "Choose ComfyUI folder" });
    if (!selected) return;
    el.comfyRootWorkflow.value = selected;
    await invoke("set_comfyui_root", { comfyuiRoot: selected });
    el.comfyRoot.value = selected;
    el.comfyRootLora.value = selected;
    logLine("ComfyUI folder selected.");
    await loadInstalledAddonState(selected);
    await refreshEffectiveDownloadDestination();
  } catch (err) {
    logLine(`Choose folder failed: ${err}`);
  }
});

el.saveInstallRoot.addEventListener("click", async () => {
  try {
    await syncComfyInstallSelection(el.comfyInstallRoot.value, true);
    const original = el.saveInstallRoot.textContent;
    el.saveInstallRoot.textContent = "Saved";
    el.saveInstallRoot.disabled = true;
    window.setTimeout(() => {
      el.saveInstallRoot.textContent = original || "Save Base";
      el.saveInstallRoot.disabled = false;
    }, 900);
    await refreshComfyResumeState();
  } catch (err) {
    logComfyLine(`Save folder failed: ${err}`);
  }
});

el.chooseInstallRoot.addEventListener("click", async () => {
  try {
    const selected = await invoke("pick_folder", { title: "Choose ComfyUI base folder" });
    if (!selected) return;
    await syncComfyInstallSelection(selected, true);
    logComfyLine("ComfyUI install folder selected.");
    await refreshComfyResumeState();
  } catch (err) {
    logComfyLine(`Choose install folder failed: ${err}`);
  }
});

el.chooseExtraModelRoot?.addEventListener("click", async () => {
  try {
    const selected = await invoke("pick_folder", { title: "Choose shared models folder" });
    if (!selected) return;
    if (el.comfyExtraModelRoot) {
      el.comfyExtraModelRoot.value = selected;
    }
    if (el.comfyExtraModelDefault) {
      el.comfyExtraModelDefault.checked = true;
    }
    state.sharedModelsRootDefault = String(selected || "").trim();
    state.sharedModelsUseDefault = true;
    if (state.comfyMode === "manage") {
      await persistComfyExtraModelConfigForRoot(el.comfyExistingInstall?.value || el.comfyRoot.value);
    }
    await refreshEffectiveDownloadDestination();
    logComfyLine("Optional extra models folder selected.");
  } catch (err) {
    logComfyLine(`Choose extra models folder failed: ${err}`);
  }
});

el.clearExtraModelRoot?.addEventListener("click", async () => {
  if (el.comfyExtraModelRoot) {
    el.comfyExtraModelRoot.value = "";
  }
  if (el.comfyExtraModelDefault) {
    el.comfyExtraModelDefault.checked = false;
  }
  state.sharedModelsRootDefault = "";
  state.sharedModelsUseDefault = false;
  if (state.comfyMode === "manage") {
    await persistComfyExtraModelConfigForRoot(el.comfyExistingInstall?.value || el.comfyRoot.value);
  }
  await refreshEffectiveDownloadDestination();
  logComfyLine("Optional extra models folder cleared.");
});

el.comfyExtraModelDefault?.addEventListener("change", async () => {
  const hasRoot = Boolean(String(el.comfyExtraModelRoot?.value || "").trim());
  if (!hasRoot && el.comfyExtraModelDefault?.checked) {
    el.comfyExtraModelDefault.checked = false;
    return;
  }
  state.sharedModelsRootDefault = String(el.comfyExtraModelRoot?.value || "").trim();
  state.sharedModelsUseDefault = Boolean(el.comfyExtraModelDefault?.checked && hasRoot);
  if (state.comfyMode === "manage") {
    await persistComfyExtraModelConfigForRoot(el.comfyExistingInstall?.value || el.comfyRoot.value);
  }
  await refreshEffectiveDownloadDestination();
});

el.comfyExtraModelRoot?.addEventListener("change", async () => {
  const rootValue = String(el.comfyExtraModelRoot?.value || "").trim();
  if (!rootValue && el.comfyExtraModelDefault?.checked) {
    el.comfyExtraModelDefault.checked = false;
  }
  state.sharedModelsRootDefault = rootValue;
  state.sharedModelsUseDefault = Boolean(el.comfyExtraModelDefault?.checked && rootValue);
  if (state.comfyMode === "manage") {
    await persistComfyExtraModelConfigForRoot(el.comfyExistingInstall?.value || el.comfyRoot.value);
  }
  await refreshEffectiveDownloadDestination();
});

el.comfyTorchProfile?.addEventListener("change", async () => {
  state.comfyTorchProfileLocked = true;
  applyComfyAddonRules();
  updateRocmGuidedUi();
  if (isRocmTorchProfile(el.comfyTorchProfile.value) || String(el.comfyTorchProfile.value || "").trim() === "torch291_xpu") {
    await refreshRocmGuidedStatus(false);
  }
});

el.comfyGpuSelection?.addEventListener("change", async () => {
  const previous = state.comfyGpuSelection;
  const selected = String(el.comfyGpuSelection.value || "auto").trim().toLowerCase();
  state.comfyGpuSelection = selected;
  state.comfyDetectedGpuVendor = selectedGpuVendor();
  state.comfyTorchProfileLocked = false;
  try {
    state.settings = await invoke("set_comfyui_gpu_selection", { gpuSelection: selected });
    applyComfyTorchProfileOptions();
    state.comfyRefreshRecommendation?.(0);
    applyComfyAddonRules();
    updateRocmGuidedUi();
  } catch (err) {
    state.comfyGpuSelection = previous;
    el.comfyGpuSelection.value = previous;
    state.comfyDetectedGpuVendor = selectedGpuVendor();
    logComfyLine(`GPU selection failed: ${err}`);
  }
});

el.comfyMode?.addEventListener("change", async () => {
  state.comfyMode = el.comfyMode.value === "manage" ? "manage" : "install";
  if (state.comfyMode !== "manage") {
    resetComfySelectionsToDefaults();
    const savedTorchProfile = String(state.settings?.comfyui_torch_profile || "").trim();
    if (state.comfyDetectedGpuVendor === "amd") {
      state.comfyTorchProfileLocked = false;
      applyComfyTorchProfileOptions("torch211_rocm72");
      el.comfyTorchProfile.value = "torch211_rocm72";
    } else if (state.comfyDetectedGpuVendor === "intel") {
      state.comfyTorchProfileLocked = false;
      applyComfyTorchProfileOptions("torch291_xpu");
      el.comfyTorchProfile.value = "torch291_xpu";
    } else {
      applyComfyTorchProfileOptions(savedTorchProfile || "torch280_cu128");
      if (savedTorchProfile && comfyTorchProfiles.some((x) => x.value === savedTorchProfile)) {
        el.comfyTorchProfile.value = savedTorchProfile;
        state.comfyTorchProfileLocked = true;
      } else {
        state.comfyTorchProfileLocked = false;
      }
    }
    applyComfyAddonRules();
    updateRocmGuidedUi();
    if (el.comfyExtraModelRoot) {
      el.comfyExtraModelRoot.value = state.sharedModelsRootDefault || "";
    }
    if (el.comfyExtraModelDefault) {
      el.comfyExtraModelDefault.checked = state.sharedModelsRootDefault
        ? Boolean(state.sharedModelsUseDefault)
        : false;
    }
  } else {
    try {
      const installs = await refreshExistingInstallations(el.comfyInstallRoot?.value || "", null);
      const latest = newestComfyInstall(installs);
      const selectedRoot = String(latest?.root || el.comfyExistingInstall?.value || el.comfyRoot.value || "").trim();
      if (selectedRoot) {
        if (el.comfyExistingInstall) {
          el.comfyExistingInstall.value = selectedRoot;
        }
        await runWithManagedInstallOverlay(`Loading ${comfyInstallNameFromRoot(selectedRoot)}...`, async () => {
          await applySelectedExistingInstallation(selectedRoot);
        });
      } else {
        await loadInstalledAddonState(el.comfyRoot.value || "");
      }
    } catch (_) {
      loadInstalledAddonState(el.comfyRoot.value || "").catch(() => {});
    }
  }
  updateComfyModeUi();
});

el.useExistingInstall?.addEventListener("click", async () => {
  const selectedRoot = String(el.comfyExistingInstall?.value || "").trim();
  if (!selectedRoot) {
    logComfyLine("No existing ComfyUI installation selected.");
    return;
  }
  try {
    await runWithManagedInstallOverlay(`Loading ${comfyInstallNameFromRoot(selectedRoot)}...`, async () => {
      await applySelectedExistingInstallation(selectedRoot);
    });
    state.comfyMode = "manage";
    if (el.comfyMode) el.comfyMode.value = "manage";
    updateComfyModeUi();
    logComfyLine(`Now managing: ${selectedRoot}`);
  } catch (err) {
    logComfyLine(`Failed to use selected installation: ${err}`);
  }
});

el.comfyExistingInstall?.addEventListener("change", async () => {
  updateComfyModeUi();
  if (state.comfyInstallSwitchBusy) return;
  const selectedRoot = String(el.comfyExistingInstall?.value || "").trim();
  if (!selectedRoot) {
    refreshComfyUiUpdateStatus("").catch(() => {});
    return;
  }
  const previousRoot = String(el.comfyRoot?.value || "").trim();
  const switchingInstall = previousRoot
    && normalizeSlashes(previousRoot) !== normalizeSlashes(selectedRoot);
  try {
    if (state.comfyMode === "manage" && switchingInstall) {
      await runWithManagedInstallOverlay(`Switching to ${comfyInstallNameFromRoot(selectedRoot)}...`, async () => {
        await refreshComfyRuntimeStatus().catch(() => {});
        if (state.comfyRuntimeRunning) {
          setStartupStatus(`Stopping ComfyUI before loading ${comfyInstallNameFromRoot(selectedRoot)}...`);
          logComfyLine("ComfyUI server is running. Stopping it before switching managed install...");
          await invoke("stop_comfyui_root");
          await refreshComfyRuntimeStatus().catch(() => {});
          if (state.comfyRuntimeRunning) {
            logComfyLine("ComfyUI is still running. Stop it first, then switch install.");
            throw new Error("ComfyUI is still running. Stop it first, then switch install.");
          }
          logComfyLine("ComfyUI server stopped.");
        }
        setStartupStatus(`Loading ${comfyInstallNameFromRoot(selectedRoot)}...`);
        await applySelectedExistingInstallation(selectedRoot);
      });
    } else if (state.comfyMode !== "manage" && switchingInstall) {
      await runWithManagedInstallOverlay(`Loading ${comfyInstallNameFromRoot(selectedRoot)}...`, async () => {
        await applySelectedExistingInstallation(selectedRoot);
      });
    } else {
      await applySelectedExistingInstallation(selectedRoot);
    }
    if (state.comfyMode === "manage") {
      logComfyLine(`Now managing: ${selectedRoot}`);
    }
  } catch (err) {
    logComfyLine(`Failed to load selected installation: ${err}`);
  }
});

el.updateSelectedInstall?.addEventListener("click", async () => {
  const selectedRoot = String(el.comfyExistingInstall?.value || "").trim();
  if (!selectedRoot) {
    logComfyLine("No existing ComfyUI installation selected.");
    return;
  }
  if (state.comfyUpdateBusy) return;
  if (!state.comfyUpdateChecked) {
    await refreshComfyUiUpdateStatus(selectedRoot);
    return;
  }
  if (!state.comfyUpdateAvailable) {
    return;
  }
  try {
    state.comfyUpdateBusy = true;
    updateComfyUpdateButton();
    logComfyLine("Updating ComfyUI...");
    const result = await invoke("update_selected_comfyui", { comfyuiRoot: selectedRoot });
    if (result) {
      logComfyLine(String(result));
    }
    clearComfyUpdateStatusCache(selectedRoot);
    await refreshComfyUiUpdateStatus(selectedRoot);
    await loadInstalledAddonState(selectedRoot);
  } catch (err) {
    logComfyLine(`ComfyUI update failed: ${err}`);
  } finally {
    state.comfyUpdateBusy = false;
    updateComfyUpdateButton();
  }
});

el.installComfyui.addEventListener("click", async () => {
  await startComfyInstall(false);
});

el.addonSageAttention?.addEventListener("change", () => {
  applyAttentionBackendFromToggle(el.addonSageAttention).catch((err) => logComfyLine(String(err)));
});
el.addonSageAttention3?.addEventListener("change", () => {
  applyAttentionBackendFromToggle(el.addonSageAttention3).catch((err) => logComfyLine(String(err)));
});
el.addonFlashAttention?.addEventListener("change", () => {
  applyAttentionBackendFromToggle(el.addonFlashAttention).catch((err) => logComfyLine(String(err)));
});
el.flagSageAttention?.addEventListener("change", () => {
  applyLaunchAttentionFlagFromToggle(el.flagSageAttention).catch((err) => logComfyLine(String(err)));
});
el.flagFlashAttention?.addEventListener("change", () => {
  applyLaunchAttentionFlagFromToggle(el.flagFlashAttention).catch((err) => logComfyLine(String(err)));
});
el.addonNunchaku?.addEventListener("change", () => {
  applyComfyAddonRules();
  applyAttentionBackendFromToggle(el.addonNunchaku).catch((err) => logComfyLine(String(err)));
});
el.addonInsightFace?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.addonInsightFace, "addon_insightface", "InsightFace")
    .catch((err) => logComfyLine(String(err)));
});
el.addonTrellis2?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.addonTrellis2, "addon_trellis2", "Trellis2")
    .catch((err) => logComfyLine(String(err)));
});
el.addonPinnedMemory?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.addonPinnedMemory, "addon_pinned_memory", "Pinned Memory")
    .catch((err) => logComfyLine(String(err)));
});
el.launchListen?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.launchListen, "launch_listen", "--listen")
    .catch((err) => logComfyLine(String(err)));
});
el.flagLowvram?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.flagLowvram, "launch_lowvram", "--lowvram")
    .catch((err) => logComfyLine(String(err)));
});
el.flagBf16Unet?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.flagBf16Unet, "launch_bf16_unet", "--bf16-unet")
    .catch((err) => logComfyLine(String(err)));
});
el.flagAsyncOffload?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.flagAsyncOffload, "launch_async_offload", "--async-offload")
    .catch((err) => logComfyLine(String(err)));
});
el.flagDisableSmartMemory?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.flagDisableSmartMemory, "launch_disable_smart_memory", "--disable-smart-memory")
    .catch((err) => logComfyLine(String(err)));
});
el.rocmGuidedCheck?.addEventListener("click", async () => {
  await refreshRocmGuidedStatus(true);
});
el.rocmGuidedInstall?.addEventListener("click", async () => {
  const target = currentGuidedAccelTarget();
  if (!target) return;
  state.rocmGuidedBusy = true;
  updateRocmGuidedUi();
  logComfyLine(`Starting guided ${target.logPrefix} setup...`);
  await new Promise((resolve) => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(resolve);
    });
  });
  try {
    const status = await invoke(target.installCommand);
    state.rocmGuidedStatus = status ? { ...status, target: target.key } : null;
    if (status?.detail) {
      logComfyLine(`${target.logPrefix} guided setup: ${status.detail}`);
    }
    await refreshRocmGuidedStatus(false);
  } catch (err) {
    logComfyLine(`${target.logPrefix} guided setup failed: ${err}`);
    await refreshRocmGuidedStatus(false);
  } finally {
    state.rocmGuidedBusy = false;
    updateRocmGuidedUi();
  }
});
el.nodeComfyuiManager?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.nodeComfyuiManager, "node_comfyui_manager", "comfyui-manager")
    .catch((err) => logComfyLine(String(err)));
});
el.nodeComfyuiEasyUse?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.nodeComfyuiEasyUse, "node_comfyui_easy_use", "ComfyUI-Easy-Use")
    .catch((err) => logComfyLine(String(err)));
});
el.nodeRgthreeComfy?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.nodeRgthreeComfy, "node_rgthree_comfy", "rgthree-comfy")
    .catch((err) => logComfyLine(String(err)));
});
el.nodeComfyuiGguf?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.nodeComfyuiGguf, "node_comfyui_gguf", "ComfyUI-GGUF")
    .catch((err) => logComfyLine(String(err)));
});
el.nodeComfyuiKjnodes?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.nodeComfyuiKjnodes, "node_comfyui_kjnodes", "comfyui-kjnodes")
    .catch((err) => logComfyLine(String(err)));
});
el.nodeComfyuiCrystools?.addEventListener("change", () => {
  applyComponentToggleFromCheckbox(el.nodeComfyuiCrystools, "node_comfyui_crystools", "comfyui-crystools")
    .catch((err) => logComfyLine(String(err)));
});
el.runPreflight?.addEventListener("click", () => {
  runComfyPreflight().then((result) => {
    if (!result) return;
    logComfyLine(result.summary || "Preflight completed.");
  });
});
el.comfyResumeBtn?.addEventListener("click", async () => {
  await startComfyInstall(false);
});
el.comfyFreshBtn?.addEventListener("click", async () => {
  await startComfyInstall(true);
});

el.comfyClearInstallLog?.addEventListener("click", () => {
  if (el.comfyInstallLog) el.comfyInstallLog.textContent = "Ready";
});

el.comfyClearRuntimeLog?.addEventListener("click", () => {
  state.comfyRuntimeLogs = [];
  renderComfyRuntimeLogs();
});

el.comfyRuntimeLogFilter?.addEventListener("change", () => {
  renderComfyRuntimeLogs();
});

el.clearStatusLog?.addEventListener("click", () => {
  if (el.statusLog) el.statusLog.textContent = "Ready";
});

el.comfyOpenInstallFolder?.addEventListener("click", async () => {
  const path = String(el.comfyOpenInstallFolder.dataset.path || "").trim();
  if (!path) return;
  try {
    await invoke("open_folder", { path });
  } catch (err) {
    logComfyLine(`Open install folder failed: ${err}`);
  }
});

el.comfyStartInstalled?.addEventListener("click", async () => {
  const preferredManageRoot = state.comfyMode === "manage"
    ? String(el.comfyExistingInstall?.value || "").trim()
    : "";
  const path = String(preferredManageRoot || el.comfyStartInstalled.dataset.path || "").trim();
  if (!path) return;
  try {
    if (state.comfyRuntimeRunning) {
      state.comfyRuntimeStarting = false;
      state.comfyRuntimeTarget = "";
      updateComfyRuntimeButton();
      const stopped = await invoke("stop_comfyui_root");
      logComfyLine(stopped ? "ComfyUI stop requested." : "ComfyUI was not running.");
      await refreshComfyRuntimeStatus();
    } else {
      state.comfyRuntimeTarget = comfyInstallNameFromRoot(path);
      state.comfyRuntimeStarting = true;
      state.comfyRuntimeRunning = false;
      updateComfyRuntimeButton();
      await invoke("start_comfyui_root", { comfyuiRoot: path });
      logComfyLine("ComfyUI launch requested.");
    }
  } catch (err) {
    state.comfyRuntimeStarting = false;
    state.comfyRuntimeRunning = false;
    state.comfyRuntimeTarget = "";
    updateComfyRuntimeButton();
    logComfyLine(`ComfyUI runtime action failed: ${err}`);
  }
});

el.saveToken.addEventListener("click", async () => {
  try {
    await invoke("save_civitai_token", { token: el.civitaiToken.value });
    const original = el.saveToken.textContent;
    el.saveToken.textContent = "Saved";
    el.saveToken.disabled = true;
    window.setTimeout(() => {
      el.saveToken.textContent = original || "Save Token";
      el.saveToken.disabled = false;
    }, 900);
    await loadLoraMetadata();
  } catch (err) {
    logLine(`Save token failed: ${err}`);
  }
});

el.checkUpdates.addEventListener("click", async () => {
  if (state.updateInstalling) return;
  if (state.updateAvailable) {
    try {
      state.updateInstalling = true;
      updateUpdateButton();
      el.updateStatus.textContent = state.updateVersion
        ? `Installing v${state.updateVersion}...`
        : "Installing update...";
      showBlockingOverlay("App updating... It will close now. Please launch it again after update.");
      await invoke("auto_update_startup");
    } catch (err) {
      state.updateInstalling = false;
      updateUpdateButton();
      el.updateStatus.textContent = "Error";
      logLine(String(err));
      hideStartupOverlay();
    }
    return;
  }
  try {
    state.updateChecking = true;
    updateUpdateButton();
    el.updateStatus.textContent = "Checking...";
    const result = await invoke("check_updates_now");
    if (result.available) {
      state.updateAvailable = true;
      state.updateVersion = result.version || null;
      el.updateStatus.textContent = "New update available";
      updateUpdateButton();
      logLine(`Update available: v${result.version}`);
    } else {
      state.updateAvailable = false;
      state.updateVersion = null;
      el.updateStatus.textContent = result.notes ? "Managed externally" : "Up to date";
      updateUpdateButton();
      logLine(result.notes || "No updates available.");
    }
  } catch (err) {
    state.updateAvailable = false;
    state.updateVersion = null;
    state.updateInstalling = false;
    updateUpdateButton();
    el.updateStatus.textContent = "Error";
    logLine(String(err));
  } finally {
    state.updateChecking = false;
    updateUpdateButton();
  }
});

el.metaCreatorLink.addEventListener("click", async (event) => {
  const href = el.metaCreatorLink.getAttribute("href") || "";
  event.preventDefault();
  if (!isSafeHttpUrl(href)) return;
  try {
    await invoke("open_external_url", { url: href });
  } catch (err) {
    logLine(`Open owner link failed: ${err}`);
  }
});

el.workflowYoutubeLink?.addEventListener("click", async (event) => {
  const href = el.workflowYoutubeLink.getAttribute("href") || "";
  event.preventDefault();
  if (!isSafeHttpUrl(href)) return;
  try {
    await invoke("open_external_url", { url: href });
  } catch (err) {
    logLine(`Open workflow tutorial link failed: ${err}`);
  }
});

document.querySelectorAll(".footer-link[data-url]").forEach((button) => {
  button.addEventListener("click", async () => {
    const url = button.getAttribute("data-url");
    if (!url) return;
    try {
      await invoke("open_external_url", { url });
    } catch (err) {
      logLine(`Open link failed: ${err}`);
    }
  });
});

async function initEventListeners() {
  if (!listen) {
    logLine("Tauri event bridge unavailable.");
    return;
  }
  try {
    await listen("download-progress", (event) => {
    const p = event.payload || {};
    if (p.phase === "cancelled") {
      logLine(`[${p.kind}] cancelled.`);
      setProgress(`[${p.kind}] cancelled`);
      state.transfers.clear();
      renderTransfers();
      endBusyDownload();
      return;
    }
    if (p.phase === "batch_finished") {
      if (p.kind !== "lora") {
        logLine(p.message || `[${p.kind}] download batch completed.`);
      }
      setProgress("Idle");
      for (const [key, transfer] of state.transfers) {
        if (transfer.kind === p.kind) state.transfers.delete(key);
      }
      renderTransfers();
      endBusyDownload();
      return;
    }
    if (p.phase === "batch_failed") {
      logLine(p.message || `[${p.kind}] download batch failed.`);
      setProgress(`[${p.kind}] failed`);
      for (const [key, transfer] of state.transfers) {
        if (transfer.kind === p.kind) state.transfers.delete(key);
      }
      renderTransfers();
      endBusyDownload();
      return;
    }

    const key = `${p.kind || "download"}:${p.index || "?"}:${p.artifact || "item"}`;
    const current = state.transfers.get(key) || {
      id: key,
      kind: p.kind || "download",
      artifact: p.artifact || "artifact",
      phase: "started",
      received: 0,
      size: Number(p.size || 0),
      folder: "",
    };
    current.phase = p.phase || current.phase;
    if (p.kind) current.kind = p.kind;
    current.lastUpdateTs = Date.now();
    if (p.artifact) current.artifact = p.artifact;
    if (p.received != null) {
      const nextReceived = Number(p.received || 0);
      const previousReceived = Number(current.received || 0);
      current.received = Number.isFinite(nextReceived)
        ? Math.max(previousReceived, nextReceived)
        : previousReceived;
    }
    if (p.phase === "started") {
      current.displayReceived = 0;
      current.displayTs = Date.now();
    }
    if (p.size != null) current.size = Number(p.size);
    if (typeof p.folder === "string" && p.folder.trim()) current.folder = p.folder.trim();
    state.transfers.set(key, current);

    if (p.phase === "started") {
      setProgress(`[${p.kind}] ${p.index || "?"}/${p.total || "?"} ${p.artifact || ""}`);
    } else if (p.phase === "progress") {
      ensureProgressSmoother();
    } else if (p.phase === "failed") {
      setProgress(`[${p.kind}] failed: ${p.message || "unknown error"}`);
      logLine(`[${p.kind}] ${p.artifact || "download"} failed: ${p.message || "unknown error"}`);
      current.phase = "failed";
      state.transfers.delete(key);
    } else if (p.phase === "finished") {
      setProgress(`[${p.kind}] finished: ${current.artifact || "file"}`);
      current.phase = "finished";
      addCompleted({
        name: current.artifact || "downloaded file",
        folder: current.folder || "",
        status: "downloaded",
      });
      state.transfers.delete(key);
      renderCompletedTransfers();
    }
    renderActiveTransfers();
    renderOverallProgress();
    });

    await listen("update-state", (event) => {
    const p = event.payload || {};
    if (p.message) {
      logLine(p.message);
      if (p.phase === "available") {
        state.updateAvailable = true;
        state.updateVersion = p.version || state.updateVersion || null;
        updateUpdateButton();
        el.updateStatus.textContent = "New update available";
      } else if (p.phase === "restarting") {
        state.updateInstalling = true;
        updateUpdateButton();
        el.updateStatus.textContent = "Installing update...";
        showBlockingOverlay("App updating... It will close now. Please launch it again after update.");
      } else {
        el.updateStatus.textContent = `${p.phase}`;
      }
    }
    });

    await listen("comfyui-install-progress", (event) => {
      const p = event.payload || {};
      const message = String(p.message || "").trim();
      if (message) {
        logComfyLine(message);
      }
      if (p.phase === "failed") {
        state.comfyInstallBusy = false;
        updateComfyInstallButton();
        return;
      }
      if (p.phase === "finished") {
        state.comfyInstallBusy = false;
        updateComfyInstallButton();
        el.comfyResumeBanner?.classList.add("hidden");
        if (typeof p.folder === "string" && p.folder.trim()) {
          const installedRoot = normalizeSlashes(p.folder.trim());
          const emittedBase = normalizeSlashes(String(p.artifact || "").trim());
          const installBase = emittedBase || parentDir(installedRoot) || installedRoot;

          const finalizeInstalledSelection = async () => {
            await syncComfyInstallSelection(installedRoot, true);
            state.comfyMode = "manage";
            if (el.comfyMode) el.comfyMode.value = "manage";
            await refreshExistingInstallations(installBase, installedRoot).catch(() => []);
            await applySelectedExistingInstallation(installedRoot).catch(() => {});
            await refreshComfyUiUpdateStatus(installedRoot).catch(() => {});
            setComfyQuickActions(installBase, installedRoot);
            await refreshComfyRuntimeStatus().catch(() => {});
            updateComfyModeUi();
          };

          finalizeInstalledSelection().catch((err) => {
            logComfyLine(`Failed to finalize installed ComfyUI selection: ${err}`);
            updateComfyModeUi();
          });
        }
        return;
      }
    });

    await listen("comfyui-runtime", (event) => {
      const p = event.payload || {};
      const phase = String(p.phase || "").trim();
      const msg = String(p.message || "").trim();
      if (msg) {
        logComfyLine(msg);
        logLine(msg);
      }
      if (phase === "starting") {
        state.comfyRuntimeStarting = true;
        state.comfyRuntimeRunning = false;
        updateComfyRuntimeButton();
        return;
      }
    if (phase === "started") {
      state.comfyRuntimeTarget = "";
      state.comfyRuntimeStarting = false;
      state.comfyRuntimeRunning = true;
      updateComfyRuntimeButton();
      refreshComfyRuntimeStatus().catch(() => {});
      return;
    }
    if (phase === "restarted_after_changes") {
      openComfyWhenReady().catch(() => {});
      return;
    }
    if (phase === "stopped" || phase === "start_failed" || phase === "stop_failed") {
      state.comfyRuntimeTarget = "";
      state.comfyRuntimeStarting = false;
      state.comfyRuntimeRunning = false;
        updateComfyRuntimeButton();
        refreshComfyRuntimeStatus().catch(() => {});
      }
    });

    await listen("comfyui-runtime-log", (event) => {
      const p = event.payload || {};
      const text = String(p.text || "").trimEnd();
      if (!text) return;
      logComfyRuntimeLine(text, String(p.stream || "stdout").trim() || "stdout");
    });
  } catch (err) {
    logLine(`Event listener setup failed: ${err}`);
  }
}

el.downloadModel.addEventListener("click", async () => {
  if (state.busyDownloads > 0) {
    await requestCancelDownload();
    return;
  }
  const items = selectedModelItems().map((item) => ({
    modelId: item.modelId,
    variantId: item.variantId,
    selectedArtifactKeys: selectedArtifactKeysForDownload(item),
  }));
  if (!items.length) {
    logLine("Select at least one model first.");
    return;
  }
  const ramTier = selectedRamTierValue();
  const vramTier = selectedVramTierValue();
  beginBusyDownload("Starting model download...");
  try {
    if (items.length === 1) {
      await invoke("download_model_assets", {
        modelId: items[0].modelId,
        variantId: items[0].variantId,
        ramTier,
        vramTier,
        selectedArtifactKeys: items[0].selectedArtifactKeys,
        comfyuiRoot: el.comfyRoot.value,
      });
    } else {
      await invoke("download_model_assets_batch", {
        request: {
          items,
          ramTier,
          vramTier,
          comfyuiRoot: el.comfyRoot.value,
        },
      });
    }
    logLine("Model download started.");
  } catch (err) {
    logLine(String(err));
    endBusyDownload();
  }
});

el.saveComfyCustomLaunchArgs?.addEventListener("click", async () => {
  const raw = String(el.comfyCustomLaunchArgs?.value || "");
  try {
    state.settings = await invoke("set_comfyui_custom_launch_args", {
      customLaunchArgs: raw,
    });
    el.comfyCustomLaunchArgs.value = state.settings?.comfyui_custom_launch_args || raw.trim();
    logComfyLine("Custom launch args saved.");
  } catch (err) {
    logComfyLine(`Saving custom launch args failed: ${err}`);
  }
});

el.clearComfyCustomLaunchArgs?.addEventListener("click", async () => {
  try {
    state.settings = await invoke("set_comfyui_custom_launch_args", {
      customLaunchArgs: "",
    });
    if (el.comfyCustomLaunchArgs) {
      el.comfyCustomLaunchArgs.value = "";
    }
    logComfyLine("Custom launch args cleared.");
  } catch (err) {
    logComfyLine(`Clearing custom launch args failed: ${err}`);
  }
});

el.comfyShowRuntimeLogs?.addEventListener("change", async () => {
  const enabled = !!el.comfyShowRuntimeLogs.checked;
  try {
    state.settings = await invoke("set_comfyui_show_runtime_logs", { enabled });
    el.comfyShowRuntimeLogs.checked = state.settings?.comfyui_show_runtime_logs !== false;
    logComfyLine(
      enabled
        ? "Runtime logs will appear in the app on next launch."
        : "Runtime logs in the app are disabled for next launch.",
    );
  } catch (err) {
    el.comfyShowRuntimeLogs.checked = !enabled;
    logComfyLine(`Runtime log toggle failed: ${err}`);
  }
});

if (el.enableHfXet) {
  el.enableHfXet.addEventListener("change", async () => {
    const enabled = !!el.enableHfXet.checked;
    el.enableHfXet.disabled = true;
    try {
      const updated = await invoke("set_hf_xet_enabled", { enabled });
      state.settings = updated;
      el.enableHfXet.checked = updated?.hf_xet_enabled === true;
      if (enabled) {
        logLine("HF Xet experimental mode enabled.");
      } else {
        logLine("HF Xet experimental mode disabled. Using default downloader.");
      }
      try {
        const xet = await invoke("get_hf_xet_preflight");
        if (xet?.detail) logLine(xet.detail);
      } catch (_) {}
    } catch (err) {
      logLine(`HF Xet toggle failed: ${err}`);
      el.enableHfXet.checked = !enabled;
    } finally {
      el.enableHfXet.disabled = false;
    }
  });
}

el.downloadLora.addEventListener("click", async () => {
  if (state.busyDownloads > 0) {
    await requestCancelDownload();
    return;
  }
  if (!el.loraId.value) {
    logLine("Select a LoRA first.");
    return;
  }
  beginBusyDownload("Starting LoRA download...");
  try {
    await invoke("download_lora_asset", {
      loraId: el.loraId.value,
      token: el.civitaiToken.value?.trim() || null,
      comfyuiRoot: el.comfyRootLora.value,
    });
  } catch (err) {
    logLine(String(err));
    endBusyDownload();
  }
});

el.downloadWorkflow?.addEventListener("click", async () => {
  if (state.busyDownloads > 0) {
    await requestCancelDownload();
    return;
  }
  const workflow = selectedWorkflow();
  if (!workflow) {
    logLine("Select a workflow first.");
    return;
  }
  const externalUrl = workflowExternalUrl(workflow);
  if (externalUrl) {
    try {
      await invoke("open_external_url", { url: externalUrl });
      logLine("Opened workflow link in browser.");
    } catch (err) {
      logLine(`Open workflow link failed: ${err}`);
    }
    return;
  }
  beginBusyDownload("Starting workflow download...");
  try {
    await invoke("download_workflow_asset", {
      workflowId: workflow.id,
      comfyuiRoot: el.comfyRootWorkflow?.value || el.comfyRoot.value,
    });
    logLine("Workflow download started.");
  } catch (err) {
    logLine(String(err));
    endBusyDownload();
  }
});

  return { initEventListeners };
}
