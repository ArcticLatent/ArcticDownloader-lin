/** @type {TauriInvoke} */
export const invoke = (command, args) => {
  const handler = window.__TAURI__?.core?.invoke;
  if (!handler) {
    return Promise.reject(new Error("Tauri IPC is unavailable."));
  }
  return handler(command, args);
};

/** @type {TauriListen} */
export const listen = (event, callback) => {
  const handler = window.__TAURI__?.event?.listen || window.__TAURI__?.core?.listen;
  if (!handler) {
    return Promise.reject(new Error("Tauri event API is unavailable."));
  }
  return handler(event, callback);
};

export const DOT_SEP = " \u2022 ";
export const ALWAYS_ONLY_VARIANT_ID = "__always_only__";

/** @type {ArcticAppState} */
export const state = {
  catalog: null,
  catalogLoading: true,
  catalogError: "",
  settings: null,
  platformCapabilities: null,
  activeTab: "comfyui",
  transfers: new Map(),
  completed: [],
  completedSeq: 0,
  loraMetaRequestSeq: 0,
  currentLoraMetaId: null,
  loraMetaCache: new Map(),
  busyDownloads: 0,
  activeDownloadKind: null,
  comfyInstallBusy: false,
  comfySage3Eligible: false,
  comfyPreflightOk: null,
  comfyResumeState: null,
  comfyRuntimeRunning: false,
  comfyRuntimeStarting: false,
  comfyRuntimeTarget: "",
  comfyAttentionBusy: false,
  comfyComponentBusy: false,
  comfyMode: "install",
  comfyInstallSwitchBusy: false,
  updateAvailable: false,
  updateVersion: null,
  updateManagedExternally: false,
  updateNotes: "",
  appVersion: "",
  updateChecking: false,
  updateInstalling: false,
  selectedComfyVersion: null,
  titleSystemText: "Loading system info...",
  comfyUpdateAvailable: false,
  comfyUpdateChecked: false,
  comfyUpdateBusy: false,
  comfyUpdateChecking: false,
  comfyLatestVersion: null,
  comfyLastUpdateDetailLogKey: "",
  comfyTorchProfileLocked: false,
  comfyAddonLoadSeq: 0,
  comfySnapshotSeq: 0,
  comfyRecommendationSeq: 0,
  comfyDetectedGpuVendor: "",
  comfyGpuSelection: "auto",
  detectedGpus: [],
  comfyRefreshRecommendation: null,
  detectedRamGb: null,
  detectedRamTier: "",
  detectedVramTier: "",
  comfyTorchRecommendedBase: "Platform recommendation: detecting the selected GPU...",
  rocmGuidedBusy: false,
  rocmGuidedStatus: null,
  sharedModelsRootDefault: "",
  sharedModelsUseDefault: false,
  selectedModelVariants: new Map(),
  manuallySelectedModelVariants: new Set(),
  selectedModelArtifactChoices: new Map(),
  comfyRuntimeLogs: [],
};

export const ramOptions = [
  { id: "ram_8", label: "8 GB RAM", gb: 8 },
  { id: "ram_16", label: "16 GB RAM", gb: 16 },
  { id: "ram_32", label: "32 GB RAM", gb: 32 },
  { id: "ram_64", label: "64 GB RAM", gb: 64 },
  { id: "ram_96", label: "96 GB RAM", gb: 96 },
  { id: "ram_128", label: "128 GB RAM", gb: 128 },
];

export const vramOptions = [
  { id: "vram_8", label: "8 GB VRAM", tier: "tier_c" },
  { id: "vram_12", label: "12 GB VRAM", tier: "tier_b" },
  { id: "vram_16", label: "16 GB VRAM", tier: "tier_a" },
  { id: "vram_24", label: "24 GB VRAM", tier: "tier_a" },
  { id: "vram_32", label: "32 GB VRAM", tier: "tier_s" },
  { id: "vram_48", label: "48 GB VRAM", tier: "tier_s" },
  { id: "vram_80", label: "80 GB VRAM", tier: "tier_s" },
  { id: "vram_96", label: "96 GB VRAM", tier: "tier_s" },
];

/** @type {Record<string, string>} */
export const vramTierLabels = {
  tier_s: "32+ GB VRAM",
  tier_a: "16/24 GB VRAM",
  tier_b: "12 GB VRAM",
  tier_c: "8 GB VRAM",
};

/** @type {Record<string, number>} */
export const tierStrength = {
  tier_c: 0,
  tier_b: 1,
  tier_a: 2,
  tier_s: 3,
};

/** @type {ArcticTorchProfile[]} */
export let comfyTorchProfiles = [
  { value: "torch271_cu128", label: "Torch 2.7.1 + cu128", backend: "nvidia" },
  { value: "torch280_cu128", label: "Torch 2.8.0 + cu128", backend: "nvidia" },
  { value: "torch211_rocm72", label: "Torch 2.11.0 + ROCm 7.2 (Linux)", backend: "amd" },
  { value: "torch291_rocm64", label: "Torch 2.9.1 + ROCm 6.4 (Linux compatibility)", backend: "amd" },
  { value: "torch291_xpu", label: "Torch 2.9.1 + XPU", backend: "intel" },
  { value: "torch291_cu130", label: "Torch 2.9.1 + cu130", backend: "nvidia" },
];

/** @param {ArcticTorchProfile[]} profiles */
export function setComfyTorchProfiles(profiles) {
  comfyTorchProfiles = profiles;
}

/**
 * Resolve markup that is required by the application shell. Failing during
 * startup gives a useful error and keeps every consumer non-nullable.
 *
 * @template {HTMLElement} T
 * @param {string} id
 * @returns {T}
 */
const byId = (id) => {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`Required application element #${id} was not found.`);
  }
  return /** @type {T} */ (element);
};

export const el = {
  version: byId("version"),
  updateStatus: byId("update-status"),
  statusLog: byId("status-log"),
  clearStatusLog: byId("clear-status-log"),
  progressLine: byId("download-progress"),
  overallProgress: byId("overall-progress"),
  overallProgressFill: byId("overall-progress-fill"),
  overallProgressMeta: byId("overall-progress-meta"),
  transferList: byId("transfer-list"),
  completedList: byId("completed-list"),
  checkUpdates: /** @type {HTMLButtonElement} */ (byId("check-updates")),
  refreshCatalog: /** @type {HTMLButtonElement} */ (byId("refresh-catalog")),
  appVersionTag: byId("app-version-tag"),

  tabComfyui: byId("tab-comfyui"),
  tabModels: byId("tab-models"),
  tabLoras: byId("tab-loras"),
  tabWorkflows: byId("tab-workflows"),
  contentComfyui: byId("tab-content-comfyui"),
  contentModels: byId("tab-content-models"),
  contentLoras: byId("tab-content-loras"),
  contentWorkflows: byId("tab-content-workflows"),
  downloadsStatusPanel: byId("downloads-status-panel"),

  comfyTorchProfile: /** @type {HTMLSelectElement} */ (byId("comfy-torch-profile")),
  comfyTorchRecommended: byId("comfy-torch-recommended"),
  comfyGpuSelection: /** @type {HTMLSelectElement} */ (byId("comfy-gpu-selection")),
  comfyGpuSelectionHelp: byId("comfy-gpu-selection-help"),
  rocmGuidedRow: byId("rocm-guided-row"),
  rocmGuidedActions: byId("rocm-guided-actions"),
  rocmGuidedStatus: byId("rocm-guided-status"),
  rocmGuidedCheck: /** @type {HTMLButtonElement} */ (byId("rocm-guided-check")),
  rocmGuidedInstall: /** @type {HTMLButtonElement} */ (byId("rocm-guided-install")),
  comfyMode: /** @type {HTMLSelectElement} */ (byId("comfy-mode")),
  comfyModeHelp: byId("comfy-mode-help"),
  comfyExistingRow: byId("comfy-existing-row"),
  comfyExistingInstall: /** @type {HTMLSelectElement} */ (byId("comfy-existing-install")),
  updateSelectedInstall: /** @type {HTMLButtonElement} */ (byId("update-selected-install")),
  useExistingInstall: /** @type {HTMLButtonElement} */ (byId("use-existing-install")),
  comfyInstallRoot: /** @type {HTMLInputElement} */ (byId("comfy-install-root")),
  chooseInstallRoot: byId("choose-install-root"),
  saveInstallRoot: /** @type {HTMLButtonElement} */ (byId("save-install-root")),
  comfyExtraModelRow: byId("comfy-extra-model-row"),
  comfyExtraModelRoot: /** @type {HTMLInputElement} */ (byId("comfy-extra-model-root")),
  chooseExtraModelRoot: byId("choose-extra-model-root"),
  comfyExtraModelDefault: /** @type {HTMLInputElement} */ (byId("comfy-extra-model-default")),
  clearExtraModelRoot: byId("clear-extra-model-root"),
  comfyResumeBanner: byId("comfy-resume-banner"),
  comfyResumeText: byId("comfy-resume-text"),
  comfyResumeBtn: byId("comfy-resume-btn"),
  comfyFreshBtn: byId("comfy-fresh-btn"),
  installComfyui: byId("install-comfyui"),
  comfyInstallSpinner: byId("comfy-install-spinner"),
  comfyQuickActions: byId("comfy-quick-actions"),
  comfyLastInstallPath: byId("comfy-last-install-path"),
  comfyOpenInstallFolder: byId("comfy-open-install-folder"),
  comfyStartInstalled: /** @type {HTMLButtonElement} */ (byId("comfy-start-installed")),
  comfyInstallLog: byId("comfy-install-log"),
  comfyClearInstallLog: byId("comfy-clear-install-log"),
  comfyRuntimeLog: byId("comfy-runtime-log"),
  comfyClearRuntimeLog: byId("comfy-clear-runtime-log"),
  comfyRuntimeLogFilter: /** @type {HTMLSelectElement} */ (byId("comfy-runtime-log-filter")),
  runPreflight: byId("run-preflight"),
  preflightSummary: byId("preflight-summary"),
  preflightList: byId("preflight-list"),
  addonSageAttention: /** @type {HTMLInputElement} */ (byId("addon-sageattention")),
  addonSageAttention3: /** @type {HTMLInputElement} */ (byId("addon-sageattention3")),
  addonFlashAttention: /** @type {HTMLInputElement} */ (byId("addon-flashattention")),
  addonInsightFace: /** @type {HTMLInputElement} */ (byId("addon-insightface")),
  addonNunchaku: /** @type {HTMLInputElement} */ (byId("addon-nunchaku")),
  addonTrellis2: /** @type {HTMLInputElement} */ (byId("addon-trellis2")),
  addonPinnedMemory: /** @type {HTMLInputElement} */ (byId("addon-pinned-memory")),
  flagSageAttention: /** @type {HTMLInputElement} */ (byId("flag-sageattention")),
  flagFlashAttention: /** @type {HTMLInputElement} */ (byId("flag-flashattention")),
  launchListen: /** @type {HTMLInputElement} */ (byId("launch-listen")),
  flagLowvram: /** @type {HTMLInputElement} */ (byId("flag-lowvram")),
  flagBf16Unet: /** @type {HTMLInputElement} */ (byId("flag-bf16-unet")),
  flagAsyncOffload: /** @type {HTMLInputElement} */ (byId("flag-async-offload")),
  flagDisableSmartMemory: /** @type {HTMLInputElement} */ (byId("flag-disable-smart-memory")),
  comfyCustomLaunchArgs: /** @type {HTMLInputElement} */ (byId("comfy-custom-launch-args")),
  saveComfyCustomLaunchArgs: byId("save-comfy-custom-launch-args"),
  clearComfyCustomLaunchArgs: byId("clear-comfy-custom-launch-args"),
  comfyShowRuntimeLogs: /** @type {HTMLInputElement} */ (byId("comfy-show-runtime-logs")),
  nodeComfyuiManager: /** @type {HTMLInputElement} */ (byId("node-comfyui-manager")),
  nodeComfyuiEasyUse: /** @type {HTMLInputElement} */ (byId("node-comfyui-easy-use")),
  nodeRgthreeComfy: /** @type {HTMLInputElement} */ (byId("node-rgthree-comfy")),
  nodeComfyuiGguf: /** @type {HTMLInputElement} */ (byId("node-comfyui-gguf")),
  nodeComfyuiKjnodes: /** @type {HTMLInputElement} */ (byId("node-comfyui-kjnodes")),
  nodeComfyuiCrystools: /** @type {HTMLInputElement} */ (byId("node-comfyui-crystools")),

  comfyRoot: /** @type {HTMLInputElement} */ (byId("comfy-root")),
  chooseRoot: byId("choose-root"),
  saveRoot: /** @type {HTMLButtonElement} */ (byId("save-root")),
  comfyRootLora: /** @type {HTMLInputElement} */ (byId("comfy-root-lora")),
  chooseRootLora: byId("choose-root-lora"),
  saveRootLora: /** @type {HTMLButtonElement} */ (byId("save-root-lora")),
  comfyRootWorkflow: /** @type {HTMLInputElement} */ (byId("comfy-root-workflow")),
  chooseRootWorkflow: byId("choose-root-workflow"),
  saveRootWorkflow: /** @type {HTMLButtonElement} */ (byId("save-root-workflow")),

  modelFamily: /** @type {HTMLSelectElement} */ (byId("model-family")),
  vramTier: /** @type {HTMLSelectElement} */ (byId("vram-tier")),
  ramTier: /** @type {HTMLSelectElement} */ (byId("ram-tier")),
  modelSelectionList: byId("model-selection-list"),
  modelSelectionSummary: byId("model-selection-summary"),
  selectedModelQueue: byId("selected-model-queue"),
  modelCatalogStatus: byId("model-catalog-status"),
  effectiveDownloadDestination: byId("effective-download-destination"),
  modelSearch: /** @type {HTMLInputElement} */ (byId("model-search")),
  selectVisibleModels: byId("select-visible-models"),
  clearModelSelection: byId("clear-model-selection"),
  downloadModel: byId("download-model"),
  enableHfXet: /** @type {HTMLInputElement} */ (byId("enable-hf-xet")),

  loraFamily: /** @type {HTMLSelectElement} */ (byId("lora-family")),
  loraId: /** @type {HTMLSelectElement} */ (byId("lora-id")),
  loraCatalogStatus: byId("lora-catalog-status"),
  civitaiToken: /** @type {HTMLInputElement} */ (byId("civitai-token")),
  saveToken: /** @type {HTMLButtonElement} */ (byId("save-token")),
  downloadLora: byId("download-lora"),
  workflowFamily: /** @type {HTMLSelectElement} */ (byId("workflow-family")),
  workflowId: /** @type {HTMLSelectElement} */ (byId("workflow-id")),
  workflowCatalogStatus: byId("workflow-catalog-status"),
  downloadWorkflow: byId("download-workflow"),

  metaCreator: byId("meta-creator"),
  metaCreatorLink: /** @type {HTMLAnchorElement} */ (byId("meta-creator-link")),
  metaStrength: byId("meta-strength"),
  metaTriggers: byId("meta-triggers"),
  metaDescription: byId("meta-description"),

  previewImage: /** @type {HTMLImageElement} */ (byId("preview-image")),
  previewVideo: /** @type {HTMLVideoElement} */ (byId("preview-video")),
  previewCaption: byId("preview-caption"),
  workflowPreviewImage: /** @type {HTMLImageElement} */ (byId("workflow-preview-image")),
  workflowPreviewCaption: byId("workflow-preview-caption"),
  workflowYoutubeLink: /** @type {HTMLAnchorElement} */ (byId("workflow-youtube-link")),
  workflowYoutubeText: byId("workflow-youtube-text"),
  confirmOverlay: byId("confirm-overlay"),
  confirmMessage: byId("confirm-message"),
  confirmYes: byId("confirm-yes"),
  confirmNo: byId("confirm-no"),
  startupOverlay: byId("startup-overlay"),
  startupStatus: byId("startup-status"),
};
