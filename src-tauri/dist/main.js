const invoke = window.__TAURI__?.core?.invoke;
const listen = window.__TAURI__?.event?.listen || window.__TAURI__?.core?.listen;

const state = {
  catalog: null,
  settings: null,
  activeTab: "models",
  transfers: new Map(),
  completed: [],
  completedSeq: 0,
  loraMetaRequestSeq: 0,
  currentLoraMetaId: null,
  loraMetaCache: new Map(),
  busyDownloads: 0,
  activeDownloadKind: null,
};

const ramOptions = [
  { id: "tier_a", label: "Tier A (64 GB+)" },
  { id: "tier_b", label: "Tier B (32-63 GB)" },
  { id: "tier_c", label: "Tier C (<32 GB)" },
];

const vramOptions = [
  { id: "tier_s", label: "Tier S (32 GB+)" },
  { id: "tier_a", label: "Tier A (16-31 GB)" },
  { id: "tier_b", label: "Tier B (12-15 GB)" },
  { id: "tier_c", label: "Tier C (<12 GB)" },
];

const el = {
  version: document.getElementById("version"),
  updateStatus: document.getElementById("update-status"),
  statusLog: document.getElementById("status-log"),
  progressLine: document.getElementById("download-progress"),
  overallProgress: document.getElementById("overall-progress"),
  overallProgressFill: document.getElementById("overall-progress-fill"),
  overallProgressMeta: document.getElementById("overall-progress-meta"),
  transferList: document.getElementById("transfer-list"),
  completedList: document.getElementById("completed-list"),
  checkUpdates: document.getElementById("check-updates"),

  tabModels: document.getElementById("tab-models"),
  tabLoras: document.getElementById("tab-loras"),
  contentModels: document.getElementById("tab-content-models"),
  contentLoras: document.getElementById("tab-content-loras"),

  comfyRoot: document.getElementById("comfy-root"),
  chooseRoot: document.getElementById("choose-root"),
  saveRoot: document.getElementById("save-root"),
  comfyRootLora: document.getElementById("comfy-root-lora"),
  chooseRootLora: document.getElementById("choose-root-lora"),
  saveRootLora: document.getElementById("save-root-lora"),

  modelFamily: document.getElementById("model-family"),
  modelId: document.getElementById("model-id"),
  vramTier: document.getElementById("vram-tier"),
  ramTier: document.getElementById("ram-tier"),
  variantId: document.getElementById("variant-id"),
  downloadModel: document.getElementById("download-model"),

  loraFamily: document.getElementById("lora-family"),
  loraId: document.getElementById("lora-id"),
  civitaiToken: document.getElementById("civitai-token"),
  saveToken: document.getElementById("save-token"),
  downloadLora: document.getElementById("download-lora"),

  metaCreator: document.getElementById("meta-creator"),
  metaCreatorLink: document.getElementById("meta-creator-link"),
  metaStrength: document.getElementById("meta-strength"),
  metaTriggers: document.getElementById("meta-triggers"),
  metaDescription: document.getElementById("meta-description"),

  previewImage: document.getElementById("preview-image"),
  previewVideo: document.getElementById("preview-video"),
  previewCaption: document.getElementById("preview-caption"),
};

function logLine(text) {
  const stamp = new Date()
    .toLocaleTimeString([], { hour: "numeric", minute: "2-digit", hour12: true })
    .replace(/\s+/g, " ")
    .toUpperCase();
  el.statusLog.textContent = `[${stamp}] ${text}\n` + el.statusLog.textContent;
}

function setProgress(text) {
  el.progressLine.textContent = text || "Idle";
}

function trimDescription(text, max = 520) {
  const value = (text || "").trim();
  if (!value) return "-";
  if (value.length <= max) return value;
  return `${value.slice(0, max).trimEnd()}...`;
}

function isVideoPreviewUrl(url) {
  const value = String(url || "").toLowerCase();
  return value.endsWith(".mp4") || value.endsWith(".webm") || value.endsWith(".mov")
    || value.includes(".mp4?") || value.includes(".webm?") || value.includes(".mov?");
}

function applyLoraPreview(previewUrl, previewKind) {
  const url = String(previewUrl || "").trim();
  const kindRaw = String(previewKind || "").trim().toLowerCase();
  const kind = kindRaw === "video" || kindRaw === "image"
    ? kindRaw
    : (url ? (isVideoPreviewUrl(url) ? "video" : "image") : "none");

  if (!url || kind === "none") {
    el.previewImage.classList.add("hidden");
    el.previewVideo.classList.add("hidden");
    el.previewImage.src = "";
    el.previewVideo.src = "";
    el.previewCaption.textContent = "No preview available.";
    return;
  }

  if (kind === "video") {
    el.previewVideo.src = url;
    el.previewVideo.classList.remove("hidden");
    el.previewImage.classList.add("hidden");
    el.previewImage.src = "";
    el.previewCaption.textContent = "Video preview loaded.";
    return;
  }

  el.previewImage.src = url;
  el.previewImage.classList.remove("hidden");
  el.previewVideo.classList.add("hidden");
  el.previewVideo.src = "";
  el.previewCaption.textContent = "Image preview loaded.";
}

async function copyText(value) {
  const text = String(value || "").trim();
  if (!text) return false;
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch (_) {}

  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  const ok = document.execCommand("copy");
  document.body.removeChild(area);
  return ok;
}

function renderTriggerWords(words) {
  const list = Array.isArray(words) ? words.filter((x) => String(x || "").trim()) : [];
  el.metaTriggers.innerHTML = "";
  if (!list.length) {
    el.metaTriggers.textContent = "-";
    return;
  }
  const frag = document.createDocumentFragment();
  list.forEach((word, idx) => {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = word;
    button.style.width = "auto";
    button.style.minHeight = "28px";
    button.style.padding = "4px 8px";
    button.style.marginRight = "6px";
    button.style.marginBottom = "6px";
    button.addEventListener("click", async () => {
      const ok = await copyText(word);
      if (!ok) {
        logLine("Copy failed.");
        return;
      }
      const original = button.textContent;
      button.textContent = "Copied";
      button.disabled = true;
      window.setTimeout(() => {
        button.textContent = original;
        button.disabled = false;
      }, 900);
    });
    frag.appendChild(button);
    if (idx < list.length - 1) {
      const spacer = document.createTextNode(" ");
      frag.appendChild(spacer);
    }
  });
  el.metaTriggers.appendChild(frag);
}

function formatBytes(v) {
  const value = Number(v || 0);
  if (!value) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let n = value;
  let u = 0;
  while (n >= 1024 && u < units.length - 1) {
    n /= 1024;
    u += 1;
  }
  return `${n.toFixed(u === 0 ? 0 : 1)} ${units[u]}`;
}

function formatVramMbToGb(vramMb) {
  const value = Number(vramMb || 0);
  if (!value) return null;
  return `${(value / 1024).toFixed(1)} GB VRAM`;
}

function renderOverallProgress() {
  const active = [...state.transfers.values()].filter((x) => x.phase !== "finished" && x.phase !== "failed");
  const busyOnly = state.busyDownloads > 0 && active.length === 0;

  if (!active.length && !busyOnly) {
    el.overallProgress.classList.add("hidden");
    el.overallProgress.classList.remove("indeterminate");
    el.overallProgressMeta.classList.add("hidden");
    el.overallProgressFill.style.width = "0%";
    return;
  }

  el.overallProgress.classList.remove("hidden");
  el.overallProgressMeta.classList.remove("hidden");

  const known = active.filter((x) => Number(x.size || 0) > 0);
  if (!known.length) {
    el.overallProgress.classList.add("indeterminate");
    el.overallProgressFill.style.removeProperty("width");
    const activeCount = Math.max(active.length, state.busyDownloads > 0 ? 1 : 0);
    el.overallProgressMeta.textContent = `Downloading ${activeCount} file(s)...`;
    return;
  }

  const totalBytes = known.reduce((sum, x) => sum + Number(x.size || 0), 0);
  const receivedBytes = known.reduce((sum, x) => sum + Math.min(Number(x.received || 0), Number(x.size || 0)), 0);
  const pct = totalBytes > 0 ? Math.max(0, Math.min(100, Math.round((receivedBytes / totalBytes) * 100))) : 0;
  const unknownCount = Math.max(0, active.length - known.length);

  el.overallProgress.classList.remove("indeterminate");
  el.overallProgressFill.style.width = `${pct}%`;
  el.overallProgressMeta.textContent = unknownCount > 0
    ? `${pct}% • ${formatBytes(receivedBytes)} / ${formatBytes(totalBytes)} • ${known.length} known + ${unknownCount} unknown`
    : `${pct}% • ${formatBytes(receivedBytes)} / ${formatBytes(totalBytes)} • ${known.length} active`;
}

function beginBusyDownload(label) {
  state.busyDownloads += 1;
  if (!state.activeDownloadKind) {
    state.activeDownloadKind = state.activeTab === "loras" ? "lora" : "model";
  }
  setProgress(label || "Downloading...");
  updateDownloadButtons();
  renderOverallProgress();
}

function endBusyDownload() {
  state.busyDownloads = Math.max(0, state.busyDownloads - 1);
  if (state.busyDownloads === 0) {
    state.activeDownloadKind = null;
    setProgress("Idle");
  }
  updateDownloadButtons();
  renderOverallProgress();
}

function updateDownloadButtons() {
  const cancelling = state.busyDownloads > 0;
  if (cancelling) {
    el.downloadModel.textContent = "Cancel Download";
    el.downloadLora.textContent = "Cancel Download";
  } else {
    el.downloadModel.textContent = "Download Model Assets";
    el.downloadLora.textContent = "Download LoRA";
  }
}

async function requestCancelDownload() {
  try {
    setProgress("Cancelling download...");
    const cancelled = await invoke("cancel_active_download");
    if (cancelled) {
      logLine("Cancellation requested.");
      setProgress("Cancellation requested...");
    } else {
      logLine("No active download to cancel.");
      endBusyDownload();
    }
  } catch (err) {
    logLine(`Cancel failed: ${err}`);
    endBusyDownload();
  }
}

function renderActiveTransfers() {
  const active = [...state.transfers.values()].filter((x) => x.phase !== "finished" && x.phase !== "failed");
  el.transferList.innerHTML = "";
  if (!active.length) {
    const msg = document.createElement("div");
    msg.className = "empty-msg";
    msg.textContent = "No active transfers.";
    el.transferList.appendChild(msg);
  }
  for (const item of active) {
    const pct = item.size > 0 ? Math.max(0, Math.min(100, Math.round((item.received / item.size) * 100))) : 0;
    const row = document.createElement("div");
    row.className = "transfer-item";
    const title = document.createElement("div");
    title.className = "transfer-title";
    title.textContent = item.artifact || item.id;
    const bar = document.createElement("div");
    bar.className = "bar";
    const fill = document.createElement("span");
    fill.style.width = `${pct}%`;
    bar.appendChild(fill);
    const sub = document.createElement("div");
    sub.className = "transfer-sub";
    sub.textContent = item.size
      ? `${item.phase} • ${formatBytes(item.received)} / ${formatBytes(item.size)}`
      : item.phase;
    row.appendChild(title);
    row.appendChild(bar);
    row.appendChild(sub);
    el.transferList.appendChild(row);
  }
}

function renderCompletedTransfers() {
  el.completedList.innerHTML = "";
  if (!state.completed.length) {
    const msg = document.createElement("div");
    msg.className = "empty-msg";
    msg.textContent = "No completed downloads.";
    el.completedList.appendChild(msg);
  } else {
    const max = Math.min(30, state.completed.length);
    for (let i = 0; i < max; i += 1) {
      const item = state.completed[i];
      const hasFolder = Boolean(item.folder && item.folder.trim());
      const row = document.createElement("div");
      row.className = "transfer-item";
      const title = document.createElement("div");
      title.className = "transfer-title";
      title.textContent = item.name;
      const sub = document.createElement("div");
      sub.className = "transfer-sub";
      sub.textContent = item.status;
      const button = document.createElement("button");
      button.textContent = "Open Folder";
      button.setAttribute("type", "button");
      if (!hasFolder) {
        button.disabled = true;
      } else {
        button.addEventListener("click", async () => {
          try {
            await invoke("open_folder", { path: item.folder });
          } catch (err) {
            logLine(`Open folder failed: ${err}`);
          }
        });
      }
      row.appendChild(title);
      row.appendChild(sub);
      row.appendChild(button);
      el.completedList.appendChild(row);
    }
  }
}

function renderTransfers() {
  renderActiveTransfers();
  renderCompletedTransfers();
  renderOverallProgress();
}

function addCompleted(item) {
  const index = state.completed.findIndex(
    (x) => x.name === item.name && x.status === item.status && x.folder === (item.folder || ""),
  );
  if (index >= 0) {
    if (item.folder && item.folder.trim()) {
      state.completed[index].folder = item.folder;
    }
  } else {
    state.completed.unshift({
      id: `done-${Date.now()}-${state.completedSeq++}`,
      name: item.name,
      folder: item.folder || "",
      status: item.status,
    });
  }
}

function setOptions(select, options, selectedValue = null) {
  const current = selectedValue ?? select.value;
  select.innerHTML = "";
  options.forEach((item) => {
    const opt = document.createElement("option");
    opt.value = item.value;
    opt.textContent = item.label;
    select.appendChild(opt);
  });
  if (options.find((item) => item.value === current)) {
    select.value = current;
  }
}

function switchTab(tab) {
  state.activeTab = tab;
  const models = tab === "models";
  el.tabModels.classList.toggle("active", models);
  el.tabLoras.classList.toggle("active", !models);
  el.contentModels.classList.toggle("hidden", !models);
  el.contentLoras.classList.toggle("hidden", models);
}

function familyOptions(models) {
  const families = [...new Set(models.map((m) => m.family))].sort();
  return [{ value: "all", label: "All Model Families" }, ...families.map((f) => ({ value: f, label: f }))];
}

function loraFamilyOptions(loras) {
  const families = [...new Set(loras.map((l) => l.family).filter(Boolean))].sort();
  return [{ value: "all", label: "All LoRA Families" }, ...families.map((f) => ({ value: f, label: f }))];
}

function refreshModelSelectors() {
  if (!state.catalog) return;

  const family = el.modelFamily.value || "all";
  const filtered = state.catalog.models.filter((m) => family === "all" || m.family === family);
  const modelOptions = filtered.map((m) => ({ value: m.id, label: m.display_name }));
  setOptions(el.modelId, modelOptions);

  const selectedModel = state.catalog.models.find((m) => m.id === el.modelId.value);
  const tier = el.vramTier.value;
  const variants = (selectedModel?.variants || [])
    .filter((v) => v.tier === tier)
    .map((v) => ({
      value: v.id,
      label: [v.model_size, v.quantization, v.note, v.tier?.toUpperCase?.()].filter(Boolean).join(" • "),
    }));

  setOptions(el.variantId, variants.length ? variants : [{ value: "", label: "No variant for selected VRAM tier" }]);
}

function refreshLoraSelectors() {
  if (!state.catalog) return;
  const family = el.loraFamily.value || "all";
  const filtered = state.catalog.loras.filter((l) => family === "all" || l.family === family);
  const options = filtered.map((l) => ({ value: l.id, label: l.display_name }));
  setOptions(el.loraId, options);
}

async function loadLoraMetadata() {
  const loraId = el.loraId.value;
  if (!loraId) return;
  const requestSeq = ++state.loraMetaRequestSeq;
  const cachedMeta = state.loraMetaCache.get(loraId) || null;

  if (cachedMeta && cachedMeta.preview_url) {
    applyLoraPreview(cachedMeta.preview_url, cachedMeta.preview_kind);
  }

  try {
    const rawMeta = await invoke("get_lora_metadata", {
      loraId,
      token: el.civitaiToken.value?.trim() || null,
    });
    const meta = { ...rawMeta };
    if (requestSeq !== state.loraMetaRequestSeq || loraId !== el.loraId.value) {
      return;
    }
    if ((!meta.preview_url || !String(meta.preview_url).trim()) && cachedMeta?.preview_url) {
      meta.preview_url = cachedMeta.preview_url;
      meta.preview_kind = cachedMeta.preview_kind;
    }
    state.loraMetaCache.set(loraId, meta);

    el.metaCreator.textContent = meta.creator || "-";
    const creatorName = String(meta.creator || "").trim();
    const creatorUrl = String(meta.creator_url || "").trim();
    const fallbackCreatorUrl = creatorName && creatorName !== "-" && creatorName.toLowerCase() !== "unknown creator"
      ? `https://civitai.com/user/${encodeURIComponent(creatorName)}`
      : "";
    const finalCreatorUrl = creatorUrl || fallbackCreatorUrl;
    if (finalCreatorUrl) {
      el.metaCreatorLink.href = finalCreatorUrl;
      el.metaCreatorLink.style.pointerEvents = "auto";
    } else {
      el.metaCreatorLink.href = "#";
      el.metaCreatorLink.style.pointerEvents = "none";
    }
    el.metaStrength.textContent = meta.strength || "-";
    renderTriggerWords(meta.triggers || []);
    el.metaDescription.textContent = trimDescription(meta.description || "-");
    state.currentLoraMetaId = loraId;

    applyLoraPreview(meta.preview_url, meta.preview_kind);
  } catch (err) {
    if (cachedMeta) {
      return;
    }
    logLine(`Metadata error: ${err}`);
  }
}

async function bootstrap() {
  if (!invoke) {
    logLine("Tauri invoke bridge unavailable.");
    return;
  }

  const [snapshot, settings, catalog] = await Promise.all([
    invoke("get_app_snapshot"),
    invoke("get_settings"),
    invoke("get_catalog"),
  ]);

  state.settings = settings;
  state.catalog = catalog;

  const ramText = `${snapshot.total_ram_gb?.toFixed?.(1) ?? "?"} GB RAM`;
  const gpuText = snapshot.nvidia_gpu_name
    ? `${snapshot.nvidia_gpu_name}${formatVramMbToGb(snapshot.nvidia_gpu_vram_mb) ? ` (${formatVramMbToGb(snapshot.nvidia_gpu_vram_mb)})` : ""}`
    : "NVIDIA GPU: Not detected";
  el.version.textContent = `Version ${snapshot.version} • ${ramText} • ${gpuText}`;

  el.comfyRoot.value = settings.comfyui_root || "";
  el.comfyRootLora.value = settings.comfyui_root || "";
  el.civitaiToken.value = settings.civitai_token || "";

  setOptions(el.modelFamily, familyOptions(catalog.models));
  setOptions(el.vramTier, vramOptions.map((v) => ({ value: v.id, label: v.label })), "tier_s");
  setOptions(el.ramTier, ramOptions.map((r) => ({ value: r.id, label: r.label })), "tier_a");
  refreshModelSelectors();

  setOptions(el.loraFamily, loraFamilyOptions(catalog.loras));
  refreshLoraSelectors();
  await loadLoraMetadata();

  logLine(`Loaded ${snapshot.model_count} models and ${snapshot.lora_count} LoRAs.`);
}

el.tabModels.addEventListener("click", () => switchTab("models"));
el.tabLoras.addEventListener("click", () => switchTab("loras"));

el.modelFamily.addEventListener("change", refreshModelSelectors);
el.modelId.addEventListener("change", refreshModelSelectors);
el.vramTier.addEventListener("change", refreshModelSelectors);

el.loraFamily.addEventListener("change", () => {
  refreshLoraSelectors();
  loadLoraMetadata().catch((err) => logLine(String(err)));
});
el.loraId.addEventListener("change", () => {
  loadLoraMetadata().catch((err) => logLine(String(err)));
});

el.saveRoot.addEventListener("click", async () => {
  try {
    await invoke("set_comfyui_root", { comfyuiRoot: el.comfyRoot.value });
    el.comfyRootLora.value = el.comfyRoot.value;
    logLine("ComfyUI folder saved.");
  } catch (err) {
    logLine(`Save folder failed: ${err}`);
  }
});

el.chooseRoot.addEventListener("click", async () => {
  try {
    const selected = await invoke("pick_folder");
    if (!selected) return;
    el.comfyRoot.value = selected;
    await invoke("set_comfyui_root", { comfyuiRoot: selected });
    el.comfyRootLora.value = selected;
    logLine("ComfyUI folder selected.");
  } catch (err) {
    logLine(`Choose folder failed: ${err}`);
  }
});

el.saveRootLora.addEventListener("click", async () => {
  try {
    await invoke("set_comfyui_root", { comfyuiRoot: el.comfyRootLora.value });
    el.comfyRoot.value = el.comfyRootLora.value;
    logLine("ComfyUI folder saved.");
  } catch (err) {
    logLine(`Save folder failed: ${err}`);
  }
});

el.chooseRootLora.addEventListener("click", async () => {
  try {
    const selected = await invoke("pick_folder");
    if (!selected) return;
    el.comfyRootLora.value = selected;
    await invoke("set_comfyui_root", { comfyuiRoot: selected });
    el.comfyRoot.value = selected;
    logLine("ComfyUI folder selected.");
  } catch (err) {
    logLine(`Choose folder failed: ${err}`);
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
  try {
    const result = await invoke("check_updates_now");
    if (result.available) {
      el.updateStatus.textContent = `Update: v${result.version} available`;
      logLine(`Update available: v${result.version}`);
    } else {
      el.updateStatus.textContent = "Update: up to date";
      logLine("No updates available.");
    }
  } catch (err) {
    el.updateStatus.textContent = "Update: error";
    logLine(String(err));
  }
});

el.metaCreatorLink.addEventListener("click", async (event) => {
  const href = el.metaCreatorLink.getAttribute("href") || "";
  if (!href || href === "#") {
    event.preventDefault();
    return;
  }
  event.preventDefault();
  try {
    await invoke("open_external_url", { url: href });
  } catch (err) {
    logLine(`Open owner link failed: ${err}`);
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
      renderTransfers();
      endBusyDownload();
      return;
    }
    if (p.phase === "batch_failed") {
      logLine(p.message || `[${p.kind}] download batch failed.`);
      setProgress(`[${p.kind}] failed`);
      renderTransfers();
      endBusyDownload();
      return;
    }

    const key = `${p.kind || "download"}:${p.index || "?"}:${p.artifact || "item"}`;
    const current = state.transfers.get(key) || {
      id: key,
      artifact: p.artifact || "artifact",
      phase: "started",
      received: 0,
      size: Number(p.size || 0),
      folder: "",
    };
    current.phase = p.phase || current.phase;
    if (p.artifact) current.artifact = p.artifact;
    if (p.received != null) current.received = Number(p.received);
    if (p.size != null) current.size = Number(p.size);
    if (typeof p.folder === "string" && p.folder.trim()) current.folder = p.folder.trim();
    state.transfers.set(key, current);

    if (p.phase === "started") {
      setProgress(`[${p.kind}] ${p.index || "?"}/${p.total || "?"} ${p.artifact || ""}`);
    } else if (p.phase === "progress") {
      const received = Number(p.received || 0);
      const size = Number(p.size || 0);
      const pct = size > 0 ? ` ${Math.round((received / size) * 100)}%` : "";
      setProgress(`[${p.kind}] ${p.artifact || ""}${pct}`);
    } else if (p.phase === "failed") {
      setProgress(`[${p.kind}] failed: ${p.message || "unknown error"}`);
      logLine(`[${p.kind}] ${p.artifact || "download"} failed: ${p.message || "unknown error"}`);
      current.phase = "failed";
      state.transfers.delete(key);
      if ((p.message || "").toLowerCase().includes("cancel")) {
        endBusyDownload();
      }
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
      el.updateStatus.textContent = `Update: ${p.phase}`;
    }
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
  if (!el.modelId.value || !el.variantId.value) {
    logLine("Select a model and variant first.");
    return;
  }
  beginBusyDownload("Starting model download...");
  try {
    await invoke("download_model_assets", {
      modelId: el.modelId.value,
      variantId: el.variantId.value,
      ramTier: el.ramTier.value,
      comfyuiRoot: el.comfyRoot.value,
    });
    logLine("Model download started.");
  } catch (err) {
    logLine(String(err));
    endBusyDownload();
  }
});

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

switchTab("models");
updateDownloadButtons();
renderTransfers();

(async () => {
  await initEventListeners();
  try {
    await bootstrap();
    try {
      const startup = await invoke("auto_update_startup");
      if (startup?.available) {
        logLine(`Auto update triggered for v${startup.version}.`);
      }
    } catch (err) {
      logLine(`Startup update check failed: ${err}`);
    }
  } catch (err) {
    logLine(`Initialization failed: ${err}`);
  }
})();
