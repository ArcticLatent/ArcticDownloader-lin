import { createDownloadProgress } from "./features/download-progress.js";
import { createBootstrapController } from "./features/bootstrap.js";
import { createCatalogFeature } from "./features/catalog.js";
import { createComfyUiFeature } from "./features/comfyui.js";
import { createGpuTorchFeature } from "./features/gpu-torch.js";
import { registerEventHandlers } from "./features/event-handlers.js";
import { createUiShell } from "./features/ui-shell.js";
import { el, invoke, state } from "./lib/app-context.js";

const {
  hideStartupOverlay,
  logComfyLine,
  logComfyRuntimeLine,
  logLine,
  renderAppVersionTag,
  renderComfyRuntimeLogs,
  renderTitleMeta,
  setStartupStatus,
  setToggleBusy,
  showBlockingOverlay,
  showConfirmDialog,
  updateUpdateButton,
  waitForNextPaint,
} = createUiShell();

const catalogFeature = createCatalogFeature({
  logLine,
  updateDownloadButtons: (...args) => updateDownloadButtons(...args),
});
const {
  catalogHasContent,
  familyOptions,
  filteredModelsForCurrentSelection,
  loadLoraMetadata,
  loadWorkflowPreview,
  loraFamilyOptions,
  modelHasAlwaysArtifacts,
  normalizeRamTier,
  recommendedVariantIdForModel,
  refreshEffectiveDownloadDestination,
  refreshLoraSelectors,
  refreshWorkflowSelectors,
  renderCatalogStatus,
  renderModelSelectionList,
  selectedArtifactKeysForDownload,
  selectedModelItems,
  selectedRamTierValue,
  selectedVramTierValue,
  selectedWorkflow,
  setCatalogLoading,
  setOptions,
  setSelectedModelVariant,
  switchTab,
  updateRamTierOptions,
  vramOptionsWithPlaceholder,
  vramTierForMb,
  workflowExternalUrl,
  workflowFamilyOptions,
} = catalogFeature;

const {
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
} = createGpuTorchFeature({ logComfyLine, setOptions });

const {
  addCompleted,
  beginBusyDownload,
  endBusyDownload,
  ensureProgressSmoother,
  renderActiveTransfers,
  renderCompletedTransfers,
  renderOverallProgress,
  renderTransfers,
  requestCancelDownload,
  setProgress,
  updateDownloadButtons,
} = createDownloadProgress({
  state,
  elements: el,
  invoke,
  logLine,
  selectedWorkflow,
  workflowExternalUrl,
});


const {
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
} = createComfyUiFeature({
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
});


const { applyCatalogSnapshot, bootstrap } = createBootstrapController({
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
});

const { initEventListeners } = registerEventHandlers({
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
});

switchTab("comfyui");
updateDownloadButtons();
updateComfyInstallButton();
updateComfyRuntimeButton();
updateComfyUpdateButton();
updateUpdateButton();
renderTransfers();

(async () => {
  setStartupStatus("Connecting event listeners...");
  await initEventListeners();
  try {
    setStartupStatus("Preparing workspace...");
    await bootstrap();
    hideStartupOverlay();
    setTimeout(() => {
      invoke("check_updates_now")
        .then((startup) => {
          if (startup?.available === true) {
            state.updateAvailable = true;
            state.updateVersion = startup.version || null;
            el.updateStatus.textContent = "New update available";
            updateUpdateButton();
            logLine(`Update available: v${startup.version}`);
          } else {
            state.updateAvailable = false;
            state.updateVersion = null;
            if (startup?.notes) {
              el.updateStatus.textContent = "Managed externally";
              logLine(startup.notes);
            }
            updateUpdateButton();
          }
        })
        .catch((err) => {
          console.debug("Startup update check skipped:", err);
        });
    }, 0);
  } catch (err) {
    logLine(`Initialization failed: ${err}`);
    setStartupStatus(`Startup error: ${err}`);
    window.setTimeout(() => {
      hideStartupOverlay();
    }, 900);
  }
})();

// Runtime status polling (low-overhead, non-overlapping) to avoid UI hitching.
scheduleRuntimeStatusPoll(1800);
