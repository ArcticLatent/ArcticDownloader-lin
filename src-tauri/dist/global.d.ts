// Minimal application-facing types for the Tauri bridge and shared frontend
// state. These intentionally model only the surface this app consumes.
export {};

declare global {
  type ArcticRecord = Record<string, any>;

  interface ArcticSelectOption {
    value: string;
    label: string;
    disabled?: boolean;
  }

  interface ArcticTorchProfile {
    value: string;
    label: string;
    backend: string;
  }

  type TauriInvoke = <T = any>(
    command: string,
    args?: Record<string, any>,
  ) => Promise<T>;

  type TauriEvent<T = any> = {
    event: string;
    id: number;
    payload: T;
  };

  type TauriListen = <T = any>(
    event: string,
    handler: (event: TauriEvent<T>) => void,
  ) => Promise<() => void>;

  interface ArcticCatalogArtifact extends ArcticRecord {}

  interface ArcticCatalogArtifactGroup extends ArcticRecord {
    artifacts?: ArcticCatalogArtifact[];
  }

  interface ArcticCatalogVariant extends ArcticRecord {
    id: string;
    artifacts?: ArcticCatalogArtifact[];
  }

  interface ArcticCatalogModel extends ArcticRecord {
    id: string;
    family: string;
    display_name: string;
    variants?: ArcticCatalogVariant[];
    always?: ArcticCatalogArtifactGroup[];
  }

  interface ArcticCatalogLora extends ArcticRecord {
    id: string;
    family: string;
    display_name: string;
  }

  interface ArcticCatalogWorkflow extends ArcticRecord {
    id: string;
    family: string;
  }

  interface ArcticCatalog {
    models: ArcticCatalogModel[];
    loras: ArcticCatalogLora[];
    workflows: ArcticCatalogWorkflow[];
  }

  interface ArcticRamThresholds {
    tier_a: number;
    tier_b: number;
    tier_c: number;
  }

  interface ArcticSelectedModelItem {
    modelId: string;
    variantId: string;
    model: ArcticCatalogModel;
    variant: ArcticCatalogVariant | null;
    alwaysOnly: boolean;
    label: string;
  }

  interface ArcticQueueArtifactGroup {
    index: number;
    rank: number;
    label: string;
    artifacts: ArcticCatalogArtifact[];
  }

  interface ArcticCatalogDependencies {
    logLine: (message: unknown) => void;
    updateDownloadButtons: () => void;
  }

  interface ArcticGuidedAccelTarget {
    key: string;
    statusLabel: string;
    checkLabel: string;
    installLabel: string;
    checkCommand: string;
    installCommand: string;
    logPrefix: string;
    detectedField: string;
  }

  interface ArcticEventHandlerDependencies {
    addCompleted: (item: ArcticCompletedDownloadInput) => void;
    applyAttentionBackendFromToggle: (box: HTMLInputElement) => Promise<void>;
    applyCatalogSnapshot: (
      catalog: ArcticCatalog,
      options?: { resetSelectors?: boolean },
    ) => void;
    applyComfyAddonRules: () => void;
    applyComfyTorchProfileOptions: (selectedValue?: string | null) => void;
    applyComponentToggleFromCheckbox: (
      box: HTMLInputElement,
      component: string,
      label: string,
    ) => Promise<void>;
    applyLaunchAttentionFlagFromToggle: (box: HTMLInputElement) => Promise<void>;
    applySelectedExistingInstallation: (rootPath: string) => Promise<void>;
    beginBusyDownload: (label: string) => void;
    clearComfyUpdateStatusCache: (rootPath: string) => void;
    comfyInstallNameFromRoot: (rootPath: unknown) => string;
    currentGuidedAccelTarget: () => ArcticGuidedAccelTarget | null;
    endBusyDownload: () => void;
    ensureProgressSmoother: () => void;
    filteredModelsForCurrentSelection: () => ArcticCatalogModel[];
    hideStartupOverlay: () => void;
    isRocmTorchProfile: (profile: unknown) => boolean;
    loadInstalledAddonState: (rootPath: unknown) => Promise<void>;
    loadLoraMetadata: () => Promise<void>;
    loadWorkflowPreview: () => void;
    logComfyLine: (message: unknown) => void;
    logComfyRuntimeLine: (message: unknown, stream?: string) => void;
    logLine: (message: unknown) => void;
    modelHasAlwaysArtifacts: (model: ArcticCatalogModel) => boolean;
    newestComfyInstall: (installs: ArcticComfyInstall[]) => ArcticComfyInstall | null;
    openComfyWhenReady: (timeoutMs?: number) => Promise<boolean>;
    persistComfyExtraModelConfigForRoot: (rootPath: unknown) => Promise<void>;
    recommendedVariantIdForModel: (model: ArcticCatalogModel, selectedTier?: string) => string;
    refreshComfyResumeState: () => Promise<void>;
    refreshComfyRuntimeStatus: () => Promise<void>;
    refreshComfyUiUpdateStatus: (rootPath?: string | null) => Promise<void>;
    refreshEffectiveDownloadDestination: () => Promise<void>;
    refreshExistingInstallations: (
      basePath: string,
      preferredRoot?: string | null,
    ) => Promise<ArcticComfyInstall[]>;
    refreshLoraSelectors: () => void;
    refreshRocmGuidedStatus: (logResult?: boolean) => Promise<ArcticRecord | null>;
    refreshWorkflowSelectors: () => void;
    renderActiveTransfers: () => void;
    renderComfyRuntimeLogs: () => void;
    renderCompletedTransfers: () => void;
    renderModelSelectionList: () => void;
    renderOverallProgress: () => void;
    renderTransfers: () => void;
    requestCancelDownload: () => Promise<void>;
    resetComfySelectionsToDefaults: () => void;
    runComfyPreflight: () => Promise<ArcticRecord | null>;
    runWithManagedInstallOverlay: <T>(
      message: string,
      work: () => Promise<T>,
    ) => Promise<T>;
    selectedArtifactKeysForDownload: (item: ArcticSelectedModelItem) => string[];
    selectedGpuVendor: (selection?: string) => string;
    selectedModelItems: () => ArcticSelectedModelItem[];
    selectedRamTierValue: () => string;
    selectedVramTierValue: () => string;
    selectedWorkflow: () => ArcticCatalogWorkflow | null;
    setCatalogLoading: (loading: boolean, message?: string) => void;
    setComfyQuickActions: (installDir: unknown, comfyRoot: unknown) => void;
    setProgress: (text: string) => void;
    setSelectedModelVariant: (
      modelId: string,
      variantId: string,
      manuallySelected?: boolean,
    ) => void;
    setStartupStatus: (message: unknown) => void;
    showBlockingOverlay: (message: unknown) => void;
    startComfyInstall: (forceFresh: boolean) => Promise<void>;
    switchTab: (tab: string) => void;
    syncComfyInstallSelection: (
      selectedPath: string,
      persistInstallBase?: boolean,
      keepCurrentMode?: boolean,
      emitDetectionLog?: boolean,
    ) => Promise<void>;
    updateComfyInstallButton: () => void;
    updateComfyModeUi: () => void;
    updateComfyRuntimeButton: () => void;
    updateComfyUpdateButton: () => void;
    updateRocmGuidedUi: () => void;
    updateUpdateButton: () => void;
    workflowExternalUrl: (workflow: ArcticCatalogWorkflow | null) => string;
  }

  interface ArcticPlatformCapabilities extends ArcticRecord {
    platform: string;
    supports_rocm_guided_setup?: boolean;
    supports_xpu_guided_setup?: boolean;
    torch_profiles?: ArcticTorchProfile[];
  }

  interface ArcticGpuOption extends ArcticRecord {
    value: string;
    label: string;
    vendor: string;
  }

  interface ArcticRuntimeLog {
    text: string;
    stream: string;
    stamp: string;
    level: string;
  }

  interface ArcticTransfer extends ArcticRecord {
    id: string;
    kind: string;
    artifact: string;
    phase: string;
    received: number;
    size: number;
    folder: string;
    displayReceived?: number;
    displayTs?: number;
    lastUpdateTs?: number;
  }

  interface ArcticCompletedDownload {
    id: string;
    name: string;
    folder: string;
    status: string;
  }

  interface ArcticCompletedDownloadInput {
    name: string;
    folder?: string;
    status: string;
  }

  interface ArcticAppState {
    catalog: ArcticCatalog | null;
    catalogLoading: boolean;
    catalogError: string;
    settings: ArcticRecord | null;
    platformCapabilities: ArcticPlatformCapabilities | null;
    activeTab: string;
    transfers: Map<string, ArcticTransfer>;
    completed: ArcticCompletedDownload[];
    completedSeq: number;
    loraMetaRequestSeq: number;
    currentLoraMetaId: string | null;
    loraMetaCache: Map<string, ArcticRecord>;
    busyDownloads: number;
    activeDownloadKind: string | null;
    comfyInstallBusy: boolean;
    comfySage3Eligible: boolean;
    comfyPreflightOk: boolean | null;
    comfyResumeState: ArcticRecord | null;
    comfyRuntimeRunning: boolean;
    comfyRuntimeStarting: boolean;
    comfyRuntimeTarget: string;
    comfyAttentionBusy: boolean;
    comfyComponentBusy: boolean;
    comfyMode: string;
    comfyInstallSwitchBusy: boolean;
    updateAvailable: boolean;
    updateVersion: string | null;
    appVersion: string;
    updateChecking: boolean;
    updateInstalling: boolean;
    selectedComfyVersion: string | null;
    titleSystemText: string;
    comfyUpdateAvailable: boolean;
    comfyUpdateChecked: boolean;
    comfyUpdateBusy: boolean;
    comfyUpdateChecking: boolean;
    comfyLatestVersion: string | null;
    comfyLastUpdateDetailLogKey: string;
    comfyTorchProfileLocked: boolean;
    comfyAddonLoadSeq: number;
    comfySnapshotSeq: number;
    comfyRecommendationSeq: number;
    comfyDetectedGpuVendor: string;
    comfyGpuSelection: string;
    detectedGpus: ArcticGpuOption[];
    comfyRefreshRecommendation: ((attempt?: number, generation?: number) => void) | null;
    detectedRamGb: number | null;
    detectedRamTier: string;
    detectedVramTier: string;
    comfyTorchRecommendedBase: string;
    rocmGuidedBusy: boolean;
    rocmGuidedStatus: ArcticRecord | null;
    sharedModelsRootDefault: string;
    sharedModelsUseDefault: boolean;
    selectedModelVariants: Map<string, string>;
    manuallySelectedModelVariants: Set<string>;
    selectedModelArtifactChoices: Map<string, boolean>;
    comfyRuntimeLogs: ArcticRuntimeLog[];
  }

  interface ArcticBootstrapDependencies {
    applyComfyAddonRules: () => void;
    applyComfyTorchProfileOptions: (selectedValue?: string | null) => void;
    catalogHasContent: (catalog?: ArcticCatalog | null) => boolean;
    familyOptions: (models: ArcticCatalogModel[]) => ArcticSelectOption[];
    isRocmTorchProfile: (profile: unknown) => boolean;
    loadInstalledAddonState: (root: string) => Promise<unknown>;
    loadLoraMetadata: () => Promise<unknown>;
    logComfyLine: (message: unknown) => void;
    logLine: (message: unknown) => void;
    loraFamilyOptions: (loras: ArcticCatalogLora[]) => ArcticSelectOption[];
    normalizeRamTier: (value: unknown) => string;
    platformLabel: () => string;
    preferredTorchProfile: (backend: string, requestedValue?: string | null) => string;
    refreshComfyResumeState: () => Promise<unknown>;
    refreshComfyRuntimeStatus: () => Promise<unknown>;
    refreshEffectiveDownloadDestination: () => Promise<unknown>;
    refreshExistingInstallations: (
      basePath: string,
      preferredRoot?: string | null,
    ) => Promise<ArcticRecord[]>;
    refreshGpuSelectionOptions: (snapshot: ArcticRecord) => void;
    refreshLoraSelectors: () => void;
    refreshRocmGuidedStatus: (logResult?: boolean) => Promise<ArcticRecord | null>;
    refreshWorkflowSelectors: () => void;
    renderAppVersionTag: () => void;
    renderCatalogStatus: () => void;
    renderModelSelectionList: () => void;
    renderPreflight: (result: ArcticRecord | null) => void;
    renderTitleMeta: () => void;
    runComfyPreflight: () => Promise<unknown>;
    setCatalogLoading: (loading: boolean, message?: string) => void;
    setComfyQuickActions: (installDir: string, comfyRoot: string) => void;
    setOptions: (
      select: HTMLSelectElement,
      options: ArcticSelectOption[],
      selectedValue?: string | null,
    ) => void;
    setStartupStatus: (text: unknown) => void;
    setTorchRecommendedDetecting: (detecting: boolean) => void;
    syncComfyInstallSelection: (
      selectedPath: string,
      persistInstallBase?: boolean,
      keepCurrentMode?: boolean,
      emitDetectionLog?: boolean,
    ) => Promise<void>;
    updateComfyModeUi: () => void;
    updateRamTierOptions: () => void;
    updateRocmGuidedUi: () => void;
    vramOptionsWithPlaceholder: () => ArcticSelectOption[];
    vramTierForMb: (value: unknown) => string;
    workflowFamilyOptions: (workflows: ArcticCatalogWorkflow[]) => ArcticSelectOption[];
  }

  interface ArcticGpuTorchDependencies {
    logComfyLine: (message: unknown) => void;
    setOptions: (
      select: HTMLSelectElement,
      options: ArcticSelectOption[],
      selectedValue?: string | null,
    ) => void;
  }

  interface ArcticDownloadProgressElements {
    progressLine: HTMLElement;
    overallProgress: HTMLElement;
    overallProgressFill: HTMLElement;
    overallProgressMeta: HTMLElement;
    transferList: HTMLElement;
    completedList: HTMLElement;
    downloadModel: HTMLElement;
    downloadLora: HTMLElement;
    downloadWorkflow: HTMLElement;
  }

  interface ArcticDownloadProgressDependencies {
    state: ArcticAppState;
    elements: ArcticDownloadProgressElements;
    invoke: TauriInvoke;
    logLine: (message: unknown) => void;
    selectedWorkflow: () => ArcticCatalogWorkflow | null;
    workflowExternalUrl: (workflow: ArcticCatalogWorkflow | null) => string;
  }

  interface ArcticComfyInstall extends ArcticRecord {
    name: string;
    root: string;
  }

  interface ArcticComfyInstallRequest {
    installRoot: string;
    extraModelRoot: string | null;
    extraModelUseDefault: boolean;
    torchProfile: string | null;
    includeSageAttention: boolean;
    includeSageAttention3: boolean;
    includeFlashAttention: boolean;
    includeInsightFace: boolean;
    includeNunchaku: boolean;
    includeTrellis2: boolean;
    includePinnedMemory: boolean;
    nodeComfyuiManager: boolean;
    nodeComfyuiEasyUse: boolean;
    nodeRgthreeComfy: boolean;
    nodeComfyuiGguf: boolean;
    nodeComfyuiKjnodes: boolean;
    nodeComfyuiCrystools: boolean;
    forceFresh?: boolean;
  }

  interface ArcticComfyUiDependencies {
    hideStartupOverlay: () => void;
    logComfyLine: (message: unknown) => void;
    refreshEffectiveDownloadDestination: () => Promise<unknown>;
    renderTitleMeta: () => void;
    setToggleBusy: (
      box: HTMLInputElement | null | undefined,
      busy: boolean,
    ) => void;
    setTorchRecommendedDetecting: (detecting: boolean) => void;
    showBlockingOverlay: (message: unknown) => void;
    showConfirmDialog: (message: string) => Promise<boolean>;
    torchProfileBackend: (profile: unknown) => string;
    torchProfileLabel: (profile: unknown) => string;
    waitForNextPaint: () => Promise<void>;
  }

  interface Window {
    __TAURI__?: {
      core?: {
        invoke?: TauriInvoke;
        listen?: TauriListen;
      };
      event?: {
        listen?: TauriListen;
      };
      [key: string]: any;
    };
  }
}
