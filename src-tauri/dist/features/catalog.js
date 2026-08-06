import {
  ALWAYS_ONLY_VARIANT_ID,
  DOT_SEP,
  el,
  invoke,
  ramOptions,
  state,
  tierStrength,
  vramOptions,
  vramTierLabels,
} from "../lib/app-context.js";
import { formatFileSize, trimDescription } from "../lib/display-format.js";
import { isSafeHttpUrl, isVideoPreviewUrl } from "../lib/url.js";

export function createCatalogFeature({ logLine, updateDownloadButtons }) {
function applyLoraPreview(previewUrl, previewKind) {
  const rawUrl = String(previewUrl || "").trim();
  const url = isSafeHttpUrl(rawUrl) ? rawUrl : "";
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

function setOptions(select, options, selectedValue = null) {
  const current = selectedValue ?? select.value;
  select.innerHTML = "";
  options.forEach((item) => {
    const opt = document.createElement("option");
    opt.value = item.value;
    opt.textContent = item.label;
    opt.disabled = Boolean(item.disabled);
    select.appendChild(opt);
  });
  if (options.find((item) => item.value === current)) {
    select.value = current;
  }
}

function catalogCounts(catalog = state.catalog) {
  return {
    models: Array.isArray(catalog?.models) ? catalog.models.length : 0,
    loras: Array.isArray(catalog?.loras) ? catalog.loras.length : 0,
    workflows: Array.isArray(catalog?.workflows) ? catalog.workflows.length : 0,
  };
}

function catalogHasContent(catalog = state.catalog) {
  const counts = catalogCounts(catalog);
  return counts.models > 0 || counts.loras > 0 || counts.workflows > 0;
}

function updateCatalogStatusElement(target, text, mode = "loading") {
  if (!target) return;
  if (!text) {
    target.classList.add("hidden");
    target.classList.remove("error", "ready");
    target.textContent = "";
    return;
  }
  target.textContent = text;
  target.classList.remove("hidden", "error", "ready");
  if (mode === "error") {
    target.classList.add("error");
  } else if (mode === "ready") {
    target.classList.add("ready");
  }
}

function renderCatalogStatus() {
  if (state.catalogLoading) {
    updateCatalogStatusElement(el.modelCatalogStatus, "Loading models from the cloud catalog...");
    updateCatalogStatusElement(el.loraCatalogStatus, "Loading LoRAs from the cloud catalog...");
    updateCatalogStatusElement(el.workflowCatalogStatus, "Loading workflows from the cloud catalog...");
    return;
  }

  if (state.catalogError) {
    const message = state.catalogError;
    updateCatalogStatusElement(el.modelCatalogStatus, message, "error");
    updateCatalogStatusElement(el.loraCatalogStatus, message, "error");
    updateCatalogStatusElement(el.workflowCatalogStatus, message, "error");
    return;
  }

  const counts = catalogCounts();
  updateCatalogStatusElement(
    el.modelCatalogStatus,
    counts.models ? "" : "No models are available in the cloud catalog.",
    counts.models ? "ready" : "error",
  );
  updateCatalogStatusElement(
    el.loraCatalogStatus,
    counts.loras ? "" : "No LoRAs are available in the cloud catalog.",
    counts.loras ? "ready" : "error",
  );
  updateCatalogStatusElement(
    el.workflowCatalogStatus,
    counts.workflows ? "" : "No workflows are available in the cloud catalog.",
    counts.workflows ? "ready" : "error",
  );
}

function setCatalogLoading(loading, message = "") {
  state.catalogLoading = Boolean(loading);
  if (loading) {
    state.catalogError = "";
  } else if (message) {
    state.catalogError = message;
  }
  renderCatalogStatus();
}


function switchTab(tab) {
  state.activeTab = tab;
  const comfyui = tab === "comfyui";
  const models = tab === "models";
  const loras = tab === "loras";
  const workflows = tab === "workflows";
  el.tabComfyui.classList.toggle("active", comfyui);
  el.tabModels.classList.toggle("active", models);
  el.tabLoras.classList.toggle("active", loras);
  el.tabWorkflows.classList.toggle("active", workflows);
  el.contentComfyui.classList.toggle("hidden", !comfyui);
  el.contentModels.classList.toggle("hidden", !models);
  el.contentLoras.classList.toggle("hidden", !loras);
  el.contentWorkflows.classList.toggle("hidden", !workflows);
  el.downloadsStatusPanel.classList.toggle("hidden", comfyui);
}

function familyOptions(models) {
  const families = [...new Set(models.map((m) => m.family))].sort();
  return [
    { value: "", label: "MODELS", disabled: true },
    ...families.map((f) => ({ value: f, label: modelFamilyLabel(f, models) })),
  ];
}

function prettyFamilyId(family) {
  return String(family || "")
    .trim()
    .replace(/[_-]+/g, " ")
    .replace(/\bflux\s*(\d+)\b/i, "FLUX $1")
    .replace(/\bwan\b/i, "WAN")
    .replace(/\bltx\s*(\d)(\d)\b/i, "LTX $1.$2")
    .replace(/\s+/g, " ");
}

function commonDisplayPrefix(names) {
  if (!names.length) return "";
  let prefix = names[0];
  for (const name of names.slice(1)) {
    let i = 0;
    while (i < prefix.length && i < name.length && prefix[i].toLowerCase() === name[i].toLowerCase()) {
      i += 1;
    }
    prefix = prefix.slice(0, i);
    if (!prefix) break;
  }
  return prefix
    .replace(/[\s._/-]+$/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

function modelFamilyLabel(family, models = state.catalog?.models || []) {
  const familyId = String(family || "").trim();
  const names = (Array.isArray(models) ? models : [])
    .filter((model) => String(model?.family || "").trim() === familyId)
    .map((model) => String(model?.display_name || "").trim())
    .filter(Boolean);
  if (names.length === 1) return names[0];
  const common = commonDisplayPrefix(names);
  if (common.length >= 4) return common;
  return prettyFamilyId(familyId) || familyId;
}

function loraFamilyOptions(loras) {
  const families = [...new Set(loras.map((l) => l.family).filter(Boolean))].sort();
  return [{ value: "all", label: "All LoRA Families" }, ...families.map((f) => ({ value: f, label: f }))];
}

function workflowFamilyOptions(workflows) {
  const families = [...new Set((workflows || []).map((w) => w.family).filter(Boolean))].sort();
  return [{ value: "all", label: "All Workflow Families" }, ...families.map((f) => ({ value: f, label: f }))];
}

function filteredModelsForCurrentSelection() {
  if (!state.catalog) return [];
  const family = String(el.modelFamily.value || "").trim();
  const search = String(el.modelSearch?.value || "").trim().toLowerCase();
  if (!family && !search) return [];
  return state.catalog.models.filter((model) => {
    if (family && model.family !== family) return false;
    if (!search) return family ? model.family === family : false;
    const variantText = (model.variants || [])
      .map((variant) => [variant.id, variant.model_size, variant.quantization, variant.note].filter(Boolean).join(" "))
      .join(" ");
    const haystack = `${model.display_name} ${model.family} ${model.id} ${variantText}`.toLowerCase();
    return haystack.includes(search);
  });
}

function modelHasAlwaysArtifacts(model) {
  return Array.isArray(model?.always)
    && model.always.some((group) => Array.isArray(group?.artifacts) && group.artifacts.length);
}

function variantVramHint(variant, selectedTier = "") {
  const variantTier = String(variant?.tier || "").trim();
  const target = String(selectedTier || "").trim();
  const label = vramTierLabels[variantTier] || variantTier.toUpperCase();
  if (!target) return label;
  if (variantTier === target) return `${label} detected fit`;
  const variantStrength = tierStrength[variantTier];
  const targetStrength = tierStrength[target];
  if (!Number.isFinite(variantStrength) || !Number.isFinite(targetStrength)) return label;
  return variantStrength > targetStrength
    ? `${label} may need more VRAM`
    : label;
}

function variantSortRank(variant, selectedTier = "") {
  const variantTier = String(variant?.tier || "").trim();
  const target = String(selectedTier || "").trim();
  if (!target) return 0;
  const variantStrength = tierStrength[variantTier];
  const targetStrength = tierStrength[target];
  if (!Number.isFinite(variantStrength) || !Number.isFinite(targetStrength)) return 50;
  if (variantTier === target) return 0;
  if (variantStrength < targetStrength) return 10 + (targetStrength - variantStrength);
  return 20 + (variantStrength - targetStrength);
}

function variantsForModel(model, selectedTier = "") {
  const variants = [...(model?.variants || [])];
  if (!selectedTier) return variants;
  return variants
    .map((variant, index) => ({ variant, index }))
    .sort((a, b) => {
      const rankDelta = variantSortRank(a.variant, selectedTier) - variantSortRank(b.variant, selectedTier);
      if (rankDelta) return rankDelta;
      return a.index - b.index;
    })
    .map((entry) => entry.variant);
}

function recommendedVariantIdForModel(model, selectedTier = selectedVramTierValue()) {
  return variantsForModel(model, selectedTier)[0]?.id || "";
}

function clearModelArtifactChoiceState(modelId, variantId = "") {
  const prefix = variantId ? `${modelId}::${variantId}::` : `${modelId}::`;
  for (const key of Array.from(state.selectedModelArtifactChoices.keys())) {
    if (key.startsWith(prefix)) {
      state.selectedModelArtifactChoices.delete(key);
    }
  }
}

function setSelectedModelVariant(modelId, variantId, manuallySelected = false) {
  if (variantId) {
    state.selectedModelVariants.set(modelId, variantId);
  } else {
    state.selectedModelVariants.delete(modelId);
  }
  if (manuallySelected) {
    state.manuallySelectedModelVariants.add(modelId);
  } else {
    state.manuallySelectedModelVariants.delete(modelId);
  }
}

function modelVariantLabel(variant, selectedTier = "") {
  return [variant.model_size, variant.quantization, variant.note, variantVramHint(variant, selectedTier)]
    .filter(Boolean)
    .join(DOT_SEP);
}

function formatRamGb(value) {
  const num = Number(value || 0);
  if (!Number.isFinite(num)) return "0";
  const rounded = Math.round(num * 10) / 10;
  return Math.abs(rounded - Math.round(rounded)) < 0.05
    ? String(Math.round(rounded))
    : String(rounded.toFixed(1));
}

function resolvedModelRamThresholds(model) {
  const cfg = model?.ram_tier_thresholds || {};
  return {
    tier_a: Number.isFinite(Number(cfg.tier_a_min_gb)) ? Number(cfg.tier_a_min_gb) : 64,
    tier_b: Number.isFinite(Number(cfg.tier_b_min_gb)) ? Number(cfg.tier_b_min_gb) : 32,
    tier_c: Number.isFinite(Number(cfg.tier_c_min_gb)) ? Number(cfg.tier_c_min_gb) : 0,
  };
}

function ramTierForGb(gb, thresholds = null) {
  const value = Number(gb);
  if (!Number.isFinite(value)) return "";
  const mins = thresholds || { tier_a: 64, tier_b: 32, tier_c: 0 };
  if (value >= Number(mins.tier_a || 64)) return "tier_a";
  if (value >= Number(mins.tier_b || 32)) return "tier_b";
  return "tier_c";
}

function vramTierForMb(mb) {
  const value = Number(mb);
  if (!Number.isFinite(value) || value <= 0) return "";
  if (value >= 32000) return "tier_s";
  if (value >= 16000) return "tier_a";
  if (value >= 12000) return "tier_b";
  return "tier_c";
}

function normalizeRamTier(value) {
  const normalized = String(value || "").trim().toLowerCase().replace(/\s+/g, "_");
  if (normalized === "tier_a" || normalized === "a") return "tier_a";
  if (normalized === "tier_b" || normalized === "b") return "tier_b";
  if (normalized === "tier_c" || normalized === "c") return "tier_c";
  return "";
}

function selectedRamTierValue() {
  const selected = String(el.ramTier?.value || "").trim();
  if (!selected) {
    return Number.isFinite(state.detectedRamGb)
      ? ramTierForGb(state.detectedRamGb, ramThresholdsForDropdownContext())
      : state.detectedRamTier || "";
  }
  if (selected === "tier_a" || selected === "tier_b" || selected === "tier_c") return selected;
  const option = ramOptions.find((item) => item.id === selected);
  return option ? ramTierForGb(option.gb, ramThresholdsForDropdownContext()) : "";
}

function selectedVramTierValue() {
  const selected = String(el.vramTier?.value || "").trim();
  if (!selected) return state.detectedVramTier || "";
  if (selected === "tier_s" || selected === "tier_a" || selected === "tier_b" || selected === "tier_c") return selected;
  return vramOptions.find((item) => item.id === selected)?.tier || "";
}

function customRamOptionLabel(tierId, thresholds) {
  const mins = thresholds || { tier_a: 64, tier_b: 32, tier_c: 0 };
  if (tierId === "tier_a") {
    return `Tier A (${formatRamGb(mins.tier_a)} GB+)`;
  }
  if (tierId === "tier_b") {
    return `Tier B (${formatRamGb(mins.tier_b)}-${formatRamGb(mins.tier_a)} GB)`;
  }
  return `Tier C (<${formatRamGb(mins.tier_b)} GB)`;
}

function hasCustomRamThresholds(model) {
  const cfg = model?.ram_tier_thresholds || {};
  return Number.isFinite(Number(cfg.tier_a_min_gb))
    || Number.isFinite(Number(cfg.tier_b_min_gb))
    || Number.isFinite(Number(cfg.tier_c_min_gb));
}

function ramThresholdKey(thresholds) {
  if (!thresholds) return "";
  return [thresholds.tier_a, thresholds.tier_b, thresholds.tier_c].join("::");
}

function sharedCustomRamThresholds(models) {
  const entries = (Array.isArray(models) ? models : [])
    .filter((model) => hasCustomRamThresholds(model))
    .map((model) => resolvedModelRamThresholds(model));
  if (!entries.length) return null;
  const firstKey = ramThresholdKey(entries[0]);
  return entries.every((entry) => ramThresholdKey(entry) === firstKey)
    ? entries[0]
    : null;
}

function ramThresholdsForDropdownContext() {
  const selectedModels = selectedModelItems().map((item) => item.model).filter(Boolean);
  const selectedShared = sharedCustomRamThresholds(selectedModels);
  if (selectedShared) return selectedShared;
  return sharedCustomRamThresholds(filteredModelsForCurrentSelection());
}

function updateRamTierOptions() {
  if (!el.ramTier) return;
  const current = el.ramTier.value || "";
  const options = [
    { value: "", label: "RAM", disabled: true },
    ...ramOptions.map((item) => ({
      value: item.id,
      label: item.label,
    })),
  ];
  setOptions(el.ramTier, options, current);
}

function vramOptionsWithPlaceholder() {
  return [
    { value: "", label: "GPU VRAM", disabled: true },
    ...vramOptions.map((item) => ({
      value: item.id,
      label: item.label,
    })),
  ];
}

function artifactSupportedOnRam(artifact, availableTier) {
  const bucketTier = String(artifact?.ram_bucket || "").trim();
  if (bucketTier) {
    return String(availableTier || "").trim() === bucketTier;
  }
  const requiredTier = String(artifact?.min_ram_tier || "").trim();
  if (!requiredTier) return true;
  const currentTier = String(availableTier || "").trim();
  if (!currentTier) return false;
  const currentStrength = tierStrength[currentTier];
  const requiredStrength = tierStrength[requiredTier];
  if (!Number.isFinite(currentStrength) || !Number.isFinite(requiredStrength)) return false;
  return currentStrength >= requiredStrength;
}

function artifactSelectableInQueue(artifact, availableTier) {
  if (String(artifact?.ram_bucket || "").trim()) return true;
  return artifactSupportedOnRam(artifact, availableTier);
}

function artifactFileName(artifact) {
  const fromPath = String(artifact?.path || "").trim().split("/").filter(Boolean).pop();
  if (fromPath) return fromPath;
  const direct = String(artifact?.direct_url || "").trim();
  if (!direct) return String(artifact?.repo || "").trim() || "artifact";
  const noQuery = direct.split("?")[0];
  return noQuery.split("/").filter(Boolean).pop() || direct;
}

function artifactDisplayBaseName(artifact) {
  return artifactFileName(artifact).replace(/\.(safetensors|gguf|ckpt|pt|pth|bin|onnx|json|ya?ml|zip)$/i, "");
}

function artifactSearchText(artifact) {
  return [
    artifactFileName(artifact),
    String(artifact?.path || "").trim(),
    String(artifact?.direct_url || "").trim(),
    String(artifact?.target_category || "").trim(),
  ].join(" ").toLowerCase();
}

function isTextEncoderArtifact(artifact) {
  return /\b(text[_\s-]*encoders?|clip)\b/.test(artifactSearchText(artifact));
}

function isTextEncoderProjectionArtifact(artifact) {
  return /(?:^|[_\s\-/])(m?m?proj|projection)(?:$|[_\s\-.])/i.test(artifactSearchText(artifact));
}

function isClipLTextEncoderArtifact(artifact) {
  if (!isTextEncoderArtifact(artifact) || isTextEncoderProjectionArtifact(artifact)) return false;
  return /(?:^|[_\s\-/.])clip[_\s\-.]?l(?:$|[_\s\-.])/i.test(artifactSearchText(artifact));
}

function isQuantizedTextEncoderArtifact(artifact) {
  if (!isTextEncoderArtifact(artifact) || isTextEncoderProjectionArtifact(artifact)) return false;
  const text = artifactSearchText(artifact);
  return /\bgguf\b/.test(text)
    || /\bqat\b/.test(text)
    || /(?:^|[_\-.])q\d(?:[_\-.]|$)/i.test(text)
    || /(?:^|[_\-.])q\d_[a-z](?:[_\-.]|$)/i.test(text)
    || /(?:^|[_\-.])fp[2-8](?:[_\-.]|$)/i.test(text)
    || /(?:^|[_\-.])int\d+(?:[_\-.]|$)/i.test(text);
}

function quantizedTextEncoderSortRank(artifact) {
  const text = artifactSearchText(artifact);
  const fpMatch = text.match(/(?:^|[_\-.])fp([2-8])(?:[_\-.]|$)/i);
  if (fpMatch) return 10 + (8 - Number(fpMatch[1]));
  const intMatch = text.match(/(?:^|[_\-.])int(\d+)(?:[_\-.]|$)/i);
  if (intMatch) return 20 + (8 - Math.min(Number(intMatch[1]), 8));
  const qMatch = text.match(/(?:^|[_\-.])q(\d)(?:[_\-.]|$)|(?:^|[_\-.])q(\d)_[a-z](?:[_\-.]|$)/i);
  if (qMatch) return 30 + (8 - Number(qMatch[1] || qMatch[2]));
  if (/\bqat\b/.test(text)) return 40;
  if (/\bgguf\b/.test(text)) return 50;
  return 99;
}

function quantizedTextEncoderLabel(artifact) {
  const text = artifactSearchText(artifact);
  const fpMatch = text.match(/(?:^|[_\-.])fp([2-8])(?:[_\-.]|$)/i);
  if (fpMatch) return `FP${fpMatch[1]}`;
  const intMatch = text.match(/(?:^|[_\-.])int(\d+)(?:[_\-.]|$)/i);
  if (intMatch) return `INT${intMatch[1]}`;
  const qMatch = text.match(/(?:^|[_\-.])q(\d)(?:[_\-.]|$)|(?:^|[_\-.])q(\d)_[a-z](?:[_\-.]|$)/i);
  if (qMatch) return `Q${qMatch[1] || qMatch[2]}`;
  if (/\bqat\b/.test(text)) return "QAT";
  if (/\bgguf\b/.test(text)) return "GGUF";
  return "quantized";
}

function isFullPrecisionTextEncoderArtifact(artifact) {
  if (!isTextEncoderArtifact(artifact) || isTextEncoderProjectionArtifact(artifact)) return false;
  return !isQuantizedTextEncoderArtifact(artifact);
}

function artifactSizeBytes(artifact) {
  const size = Number(artifact?.size_bytes);
  return Number.isFinite(size) && size > 0 ? size : 0;
}

function artifactDisplayName(artifact) {
  const name = artifactDisplayBaseName(artifact);
  const size = formatFileSize(artifactSizeBytes(artifact));
  return size ? `${name} (File Size: ${size})` : name;
}

function artifactRuntimeRamBytes(artifact) {
  const nested = Number(artifact?.memory_estimate?.runtime_ram_bytes);
  if (Number.isFinite(nested) && nested > 0) return nested;
  const flat = Number(artifact?.runtime_ram_bytes);
  return Number.isFinite(flat) && flat > 0 ? flat : 0;
}

function artifactRuntimeRamLabel(artifact) {
  const size = formatFileSize(artifactRuntimeRamBytes(artifact));
  return size ? `Estimated RAM while running: ${size}` : "";
}

function appendArtifactMetaLine(parent, text) {
  const value = String(text || "").trim();
  if (!value) return;
  const meta = document.createElement("span");
  meta.className = "queue-artifact-choice-meta";
  meta.textContent = value;
  parent.appendChild(meta);
}

function artifactChoiceKey(artifact) {
  return [
    String(artifact?.target_category || "").trim(),
    String(artifact?.repo || "").trim(),
    String(artifact?.path || "").trim(),
    String(artifact?.direct_url || "").trim(),
    artifactFileName(artifact),
  ].join("::");
}

function artifactChoiceStateKey(item, artifact) {
  return [item.modelId, item.variantId, artifactChoiceKey(artifact)].join("::");
}

function groupPreferredTierATextEncoderKey(group, ramTier) {
  if (String(ramTier || "").trim() !== "tier_a") return "";
  const artifacts = Array.isArray(group?.artifacts) ? group.artifacts : [];
  let preferred = null;
  artifacts.forEach((artifact) => {
    if (!artifactSelectableInQueue(artifact, ramTier)) return;
    if (!artifactDefaultSupportedOnRam(artifact, ramTier)) return;
    if (!isFullPrecisionTextEncoderArtifact(artifact)) return;
    if (!preferred || artifactSizeBytes(artifact) > artifactSizeBytes(preferred)) {
      preferred = artifact;
    }
  });
  return preferred ? artifactChoiceKey(preferred) : "";
}

function artifactDefaultSupportedOnRam(artifact, ramTier) {
  const bucketTier = String(artifact?.ram_bucket || "").trim();
  if (bucketTier) {
    const currentTier = String(ramTier || "").trim();
    return currentTier ? bucketTier === currentTier : true;
  }
  return artifactSupportedOnRam(artifact, ramTier);
}

function artifactDefaultChecked(artifact, ramTier, group = null) {
  const preferredTierATextEncoderKey = groupPreferredTierATextEncoderKey(group, ramTier);
  if (preferredTierATextEncoderKey && isTextEncoderArtifact(artifact) && !isTextEncoderProjectionArtifact(artifact)) {
    if (isClipLTextEncoderArtifact(artifact)) {
      return artifactDefaultSupportedOnRam(artifact, ramTier);
    }
    return artifactChoiceKey(artifact) === preferredTierATextEncoderKey;
  }
  return artifactDefaultSupportedOnRam(artifact, ramTier);
}

function artifactChoiceChecked(item, artifact, ramTier, group = null) {
  const key = artifactChoiceStateKey(item, artifact);
  if (state.selectedModelArtifactChoices.has(key)) {
    return state.selectedModelArtifactChoices.get(key);
  }
  return artifactDefaultChecked(artifact, ramTier, group);
}

function ramBucketLabel(tierId, artifact = null) {
  if (isFullPrecisionTextEncoderArtifact(artifact)) {
    return "Highest fidelity, largest memory use";
  }
  if (isQuantizedTextEncoderArtifact(artifact)) {
    const label = quantizedTextEncoderLabel(artifact);
    if (/^FP8$/i.test(label)) return "High fidelity, lower memory than full precision";
    if (/^Q[56]$/i.test(label)) return `Good quality, lower memory than FP8 (${label})`;
    if (/^(Q4|FP4|INT4)$/i.test(label)) return `Balanced quality and memory use (${label})`;
    if (/^(Q[123]|FP[23]|INT[123])$/i.test(label)) return `Lowest memory use, most quality tradeoff (${label})`;
    return `Quantized, lower memory than full precision (${label})`;
  }
  if (!tierId) return "";
  return customRamOptionLabel(tierId, ramThresholdsForDropdownContext());
}

function queueArtifactGroupLabel(group) {
  const categories = (Array.isArray(group?.artifacts) ? group.artifacts : [])
    .map((artifact) => targetCategoryLabel(artifact?.target_category))
    .filter(Boolean);
  const unique = Array.from(new Set(categories));
  if (unique.length) return unique.join(" / ");
  const id = String(group?.id || "").trim().replace(/[_-]+/g, " ");
  if (!id) return "Always";
  return id.replace(/\b\w/g, (char) => char.toUpperCase());
}

function targetCategoryLabel(category) {
  const normalized = String(category || "").trim().toLowerCase();
  const labels = {
    clip: "Text Encoders",
    text_encoders: "Text Encoders",
    loras: "LoRAs",
    upscale_models: "Upscale Models",
    vae: "VAE",
    clip_vision: "CLIP Vision",
    diffusion_models: "Diffusion Models",
    unet: "UNet",
    controlnet: "ControlNet",
    sams: "SAM Models",
    pulid: "PuLID",
    style_models: "Style Models",
    facerestore_models: "Face Restore Models",
  };
  if (labels[normalized]) return labels[normalized];
  return normalized
    .split("/")
    .map((part) => part.replace(/[_-]+/g, " ").replace(/\b\w/g, (char) => char.toUpperCase()))
    .filter(Boolean)
    .join(" / ");
}

function queueArtifactGroupRank(group) {
  const id = String(group?.id || "").trim().toLowerCase();
  const label = String(group?.label || "").trim().toLowerCase();
  const categories = (Array.isArray(group?.artifacts) ? group.artifacts : [])
    .map((artifact) => String(artifact?.target_category || "").trim().toLowerCase());
  const haystack = [id, label, ...categories].join(" ");
  if (/\b(text[_\s-]*encoders?|clip)\b/.test(haystack)) return 0;
  return 1;
}

function queueArtifactRank(artifact) {
  if (isFullPrecisionTextEncoderArtifact(artifact)) return 0;
  if (isTextEncoderProjectionArtifact(artifact)) return 1;
  if (isQuantizedTextEncoderArtifact(artifact)) return 2;
  return 3;
}

function sortQueueArtifacts(artifacts) {
  if (!artifacts.some((artifact) => isTextEncoderArtifact(artifact) || isTextEncoderProjectionArtifact(artifact))) {
    return artifacts;
  }
  return artifacts
    .map((artifact, index) => ({ artifact, index }))
    .sort((a, b) => {
      const rankDelta = queueArtifactRank(a.artifact) - queueArtifactRank(b.artifact);
      if (rankDelta) return rankDelta;
      if (isQuantizedTextEncoderArtifact(a.artifact) && isQuantizedTextEncoderArtifact(b.artifact)) {
        const quantDelta = quantizedTextEncoderSortRank(a.artifact) - quantizedTextEncoderSortRank(b.artifact);
        if (quantDelta) return quantDelta;
      }
      const sizeDelta = artifactSizeBytes(b.artifact) - artifactSizeBytes(a.artifact);
      if (sizeDelta) return sizeDelta;
      return a.index - b.index;
    })
    .map((entry) => entry.artifact);
}

function alwaysArtifactGroupsForModel(model, ramTier) {
  const groups = Array.isArray(model?.always) ? model.always : [];
  return groups
    .map((group, index) => {
      const seen = new Set();
      const artifacts = (Array.isArray(group?.artifacts) ? group.artifacts : []).filter((artifact) => {
        if (!artifactSelectableInQueue(artifact, ramTier)) return false;
        const key = artifactChoiceKey(artifact);
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });
      return {
        index,
        rank: queueArtifactGroupRank(group),
        label: queueArtifactGroupLabel(group),
        artifacts: sortQueueArtifacts(artifacts),
      };
    })
    .filter((group) => group.artifacts.length > 0)
    .sort((a, b) => (a.rank - b.rank) || (a.index - b.index));
}

function selectedVariantArtifactsForDisplay(variant, ramTier) {
  const artifacts = Array.isArray(variant?.artifacts) ? variant.artifacts : [];
  const seen = new Set();
  return artifacts.filter((artifact) => {
    if (!artifactSupportedOnRam(artifact, ramTier)) return false;
    const key = artifactChoiceKey(artifact);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function selectedArtifactKeysForDownload(item) {
  const ramTier = selectedRamTierValue();
  const keys = new Set();
  selectedVariantArtifactsForDisplay(item.variant, ramTier).forEach((artifact) => {
    keys.add(artifactChoiceKey(artifact));
  });
  alwaysArtifactGroupsForModel(item.model, ramTier).forEach((group) => {
    group.artifacts.forEach((artifact) => {
      if (artifactChoiceChecked(item, artifact, ramTier, group)) {
        keys.add(artifactChoiceKey(artifact));
      }
    });
  });
  return Array.from(keys);
}

function selectedModelItems() {
  const items = [];
  state.selectedModelVariants.forEach((variantId, modelId) => {
    const model = state.catalog?.models?.find((entry) => entry.id === modelId);
    if (!model) return;
    const variant = (model.variants || []).find((entry) => entry.id === variantId);
    const alwaysOnly = !variant && variantId === ALWAYS_ONLY_VARIANT_ID && modelHasAlwaysArtifacts(model);
    if (!variant && !alwaysOnly) return;
    items.push({
      modelId,
      variantId,
      model,
      variant: variant || null,
      alwaysOnly,
      label: alwaysOnly
        ? model.display_name
        : `${model.display_name}${variant ? `${DOT_SEP}${modelVariantLabel(variant)}` : ""}`,
    });
  });
  return items;
}

function updateModelSelectionSummary() {
  if (!el.modelSelectionSummary) return;
  const items = selectedModelItems();
  updateRamTierOptions();
  if (!items.length) {
    el.modelSelectionSummary.textContent = "No models selected.";
    return;
  }
  if (items.length === 1) {
    el.modelSelectionSummary.textContent = `1 model selected${DOT_SEP}${items[0].label}`;
    return;
  }
  el.modelSelectionSummary.textContent = `${items.length} models selected.`;
}

function renderSelectedModelQueue() {
  if (!el.selectedModelQueue) return;
  const items = selectedModelItems();
  const ramTier = selectedRamTierValue();
  el.selectedModelQueue.innerHTML = "";
  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "empty-msg";
    empty.textContent = "No models selected yet.";
    el.selectedModelQueue.appendChild(empty);
    return;
  }

  const note = document.createElement("div");
  note.className = "queue-note";
  note.textContent = "Minimum required files are selected automatically. You can adjust optional text encoders, LoRAs, upscalers, and other support files before downloading.";
  el.selectedModelQueue.appendChild(note);

  items
    .sort((a, b) => a.label.localeCompare(b.label))
    .forEach((item) => {
      const row = document.createElement("div");
      row.className = "queue-item";
      const selectedArtifacts = selectedVariantArtifactsForDisplay(item.variant, ramTier);
      const head = document.createElement("div");
      head.className = "queue-item-head";
      const title = document.createElement("div");
      title.className = "queue-item-title";
      title.textContent = selectedArtifacts.length === 1
        ? artifactDisplayName(selectedArtifacts[0])
        : selectedArtifacts.length > 1
          ? `${selectedArtifacts.length} selected variant files`
          : item.label;
      const remove = document.createElement("button");
      remove.type = "button";
      remove.textContent = "Remove";
      remove.addEventListener("click", () => {
        setSelectedModelVariant(item.modelId, "");
        clearModelArtifactChoiceState(item.modelId, item.variantId);
        renderModelSelectionList();
      });
      head.appendChild(title);
      head.appendChild(remove);
      row.appendChild(head);

      if (selectedArtifacts.length) {
        const selectedSection = document.createElement("div");
        selectedSection.className = "queue-item-artifacts";

        const selectedHeader = document.createElement("div");
        selectedHeader.className = "queue-item-subheader";
        selectedHeader.textContent = "Selected Model";
        selectedSection.appendChild(selectedHeader);

        const selectedList = document.createElement("div");
        selectedList.className = "queue-artifact-list";
        selectedArtifacts.forEach((artifact) => {
          const entry = document.createElement("div");
          entry.className = "queue-artifact-item";
          const text = document.createElement("span");
          text.className = "queue-artifact-choice-text";
          const name = document.createElement("span");
          name.textContent = artifactDisplayName(artifact);
          text.appendChild(name);
          appendArtifactMetaLine(text, artifactRuntimeRamLabel(artifact));
          entry.appendChild(text);
          selectedList.appendChild(entry);
        });
        selectedSection.appendChild(selectedList);
        row.appendChild(selectedSection);
      }

      const alwaysGroups = alwaysArtifactGroupsForModel(item.model, ramTier);
      if (alwaysGroups.length) {
        const section = document.createElement("div");
        section.className = "queue-item-artifacts";

        const header = document.createElement("div");
        header.className = "queue-item-subheader";
        header.textContent = item.alwaysOnly ? "All required files" : "Additional Model Files";
        section.appendChild(header);

        alwaysGroups.forEach((group) => {
          const groupWrap = document.createElement("div");
          groupWrap.className = "queue-artifact-group";

          const groupLabel = document.createElement("div");
          groupLabel.className = "queue-artifact-group-label";
          groupLabel.textContent = group.label;
          groupWrap.appendChild(groupLabel);

          const list = document.createElement("div");
          list.className = "queue-artifact-list";
          group.artifacts.forEach((artifact) => {
            const label = document.createElement("label");
            label.className = "queue-artifact-item queue-artifact-choice";

            const checkbox = document.createElement("input");
            checkbox.type = "checkbox";
            checkbox.checked = artifactChoiceChecked(item, artifact, ramTier, group);
            checkbox.addEventListener("change", () => {
              state.selectedModelArtifactChoices.set(
                artifactChoiceStateKey(item, artifact),
                checkbox.checked,
              );
            });

            const text = document.createElement("span");
            text.className = "queue-artifact-choice-text";
            const name = document.createElement("span");
            name.textContent = artifactDisplayName(artifact);
            text.appendChild(name);
            appendArtifactMetaLine(text, artifactRuntimeRamLabel(artifact));

            const bucket = String(artifact?.ram_bucket || "").trim();
            if (bucket) {
              appendArtifactMetaLine(text, ramBucketLabel(bucket, artifact));
            }

            label.appendChild(checkbox);
            label.appendChild(text);
            list.appendChild(label);
          });
          groupWrap.appendChild(list);
          section.appendChild(groupWrap);
        });

        row.appendChild(section);
      }

      el.selectedModelQueue.appendChild(row);
    });
}

function renderModelSelectionList() {
  if (!el.modelSelectionList) return;
  el.modelSelectionList.innerHTML = "";

  if (state.catalogLoading && !catalogHasContent()) {
    const empty = document.createElement("div");
    empty.className = "empty-msg";
    empty.textContent = "Loading models from the cloud catalog...";
    el.modelSelectionList.appendChild(empty);
    updateModelSelectionSummary();
    renderSelectedModelQueue();
    return;
  }

  if (state.catalogError && !catalogHasContent()) {
    const empty = document.createElement("div");
    empty.className = "empty-msg";
    empty.textContent = "Catalog unavailable. Check your connection and Supabase configuration.";
    el.modelSelectionList.appendChild(empty);
    updateModelSelectionSummary();
    renderSelectedModelQueue();
    return;
  }

  const models = filteredModelsForCurrentSelection();
  const tier = selectedVramTierValue();

  if (!String(el.modelFamily.value || "").trim() && !String(el.modelSearch?.value || "").trim()) {
    const empty = document.createElement("div");
    empty.className = "empty-msg";
    empty.textContent = "Choose a model family or search all models.";
    el.modelSelectionList.appendChild(empty);
    updateModelSelectionSummary();
    renderSelectedModelQueue();
    return;
  }

  if (!models.length) {
    const empty = document.createElement("div");
    empty.className = "empty-msg";
    empty.textContent = "No models available for this filter.";
    el.modelSelectionList.appendChild(empty);
    updateModelSelectionSummary();
    renderSelectedModelQueue();
    return;
  }

  models.forEach((model) => {
    const variants = variantsForModel(model, tier);
    const selectedVariantId = state.selectedModelVariants.get(model.id);
    const supportsAlwaysOnly = !variants.length && modelHasAlwaysArtifacts(model);
    const recommendedVariantId = recommendedVariantIdForModel(model, tier);
    const fallbackVariantId = recommendedVariantId || (supportsAlwaysOnly ? ALWAYS_ONLY_VARIANT_ID : "");
    const selectedVariantIsValid = variants.some((variant) => variant.id === selectedVariantId);
    const currentVariantId = state.manuallySelectedModelVariants.has(model.id) && selectedVariantIsValid
      ? selectedVariantId
      : (supportsAlwaysOnly && selectedVariantId === ALWAYS_ONLY_VARIANT_ID)
        ? ALWAYS_ONLY_VARIANT_ID
      : fallbackVariantId;
    const keepManualSelection = state.manuallySelectedModelVariants.has(model.id)
      && selectedVariantIsValid
      && currentVariantId === selectedVariantId;

    if (state.selectedModelVariants.has(model.id) && currentVariantId) {
      setSelectedModelVariant(model.id, currentVariantId, keepManualSelection);
    } else if (!currentVariantId) {
      setSelectedModelVariant(model.id, "");
    }

    const row = document.createElement("div");
    row.className = "model-select-item";
    const head = document.createElement("div");
    head.className = "model-select-head";

    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = Boolean(currentVariantId && state.selectedModelVariants.has(model.id));
    box.disabled = !currentVariantId;
    box.addEventListener("change", () => {
      const activeVariantId = String(variantSelect.value || "").trim() || currentVariantId;
      if (box.checked && activeVariantId) {
        setSelectedModelVariant(model.id, activeVariantId, state.manuallySelectedModelVariants.has(model.id));
      } else {
        setSelectedModelVariant(model.id, "");
      }
      updateModelSelectionSummary();
      renderSelectedModelQueue();
    });

    const labelWrap = document.createElement("div");
    labelWrap.className = "model-select-label";
    const title = document.createElement("div");
    title.className = "model-select-title";
    title.textContent = model.display_name;
    const meta = document.createElement("div");
    meta.className = "model-select-meta";
    const familyLabel = modelFamilyLabel(model.family);
    meta.textContent = supportsAlwaysOnly
      ? `${familyLabel}${DOT_SEP}All required files`
      : `${familyLabel}${variants.length ? `${DOT_SEP}${variants.length} manual variant${variants.length === 1 ? "" : "s"}${tier ? `${DOT_SEP}Detected GPU: ${vramTierLabels[tier] || tier.toUpperCase()}` : ""}` : `${DOT_SEP}No variants`}`;
    labelWrap.appendChild(title);
    labelWrap.appendChild(meta);

    const variantSelect = document.createElement("select");
    const variantOptions = variants.length
      ? variants.map((variant) => ({ value: variant.id, label: modelVariantLabel(variant, tier) }))
      : supportsAlwaysOnly
        ? [{ value: ALWAYS_ONLY_VARIANT_ID, label: "All required files" }]
      : [{ value: "", label: "No variants available", disabled: true }];
    setOptions(variantSelect, variantOptions, currentVariantId);
    variantSelect.disabled = !variants.length && !supportsAlwaysOnly;
    variantSelect.addEventListener("change", () => {
      const nextVariantId = String(variantSelect.value || "").trim();
      if (box.checked && nextVariantId) {
        setSelectedModelVariant(model.id, nextVariantId, true);
      } else if (nextVariantId) {
        state.manuallySelectedModelVariants.add(model.id);
      }
      updateModelSelectionSummary();
      renderSelectedModelQueue();
    });

    head.appendChild(box);
    head.appendChild(labelWrap);
    head.appendChild(variantSelect);
    row.appendChild(head);
    el.modelSelectionList.appendChild(row);
  });

  updateModelSelectionSummary();
  renderSelectedModelQueue();
}

async function refreshEffectiveDownloadDestination() {
  if (!el.effectiveDownloadDestination) return;
  const root = String(el.comfyRoot.value || "").trim();
  if (!root) {
    el.effectiveDownloadDestination.textContent = "Download destination: Select your ComfyUI root folder first.";
    return;
  }
  try {
    const result = await invoke("get_effective_download_destination", { comfyuiRoot: root });
    const effectiveRoot = String(result?.effective_root || root).trim();
    const usesShared = result?.uses_shared_default === true;
    el.effectiveDownloadDestination.textContent = usesShared
      ? `Download destination: ${effectiveRoot} (shared models default is active)`
      : `Download destination: ${effectiveRoot}`;
  } catch (err) {
    el.effectiveDownloadDestination.textContent = `Download destination unavailable: ${err}`;
  }
}

function refreshLoraSelectors() {
  if (!state.catalog) return;
  if (state.catalogLoading && !catalogHasContent()) {
    setOptions(el.loraFamily, [{ value: "", label: "Loading LoRAs...", disabled: true }], "");
    setOptions(el.loraId, [{ value: "", label: "Loading LoRAs...", disabled: true }], "");
    return;
  }
  if (state.catalogError && !catalogHasContent()) {
    setOptions(el.loraFamily, [{ value: "", label: "Catalog unavailable", disabled: true }], "");
    setOptions(el.loraId, [{ value: "", label: "Catalog unavailable", disabled: true }], "");
    return;
  }
  const family = el.loraFamily.value || "all";
  const filtered = state.catalog.loras.filter((l) => family === "all" || l.family === family);
  const options = filtered.map((l) => ({ value: l.id, label: l.display_name }));
  setOptions(el.loraId, options.length ? options : [{ value: "", label: "No LoRAs available", disabled: true }]);
}

function refreshWorkflowSelectors() {
  if (!state.catalog) return;
  if (state.catalogLoading && !catalogHasContent()) {
    setOptions(el.workflowFamily, [{ value: "", label: "Loading workflows...", disabled: true }], "");
    setOptions(el.workflowId, [{ value: "", label: "Loading workflows...", disabled: true }], "");
    loadWorkflowPreview();
    return;
  }
  if (state.catalogError && !catalogHasContent()) {
    setOptions(el.workflowFamily, [{ value: "", label: "Catalog unavailable", disabled: true }], "");
    setOptions(el.workflowId, [{ value: "", label: "Catalog unavailable", disabled: true }], "");
    loadWorkflowPreview();
    return;
  }
  const family = el.workflowFamily.value || "all";
  const filtered = (state.catalog.workflows || []).filter((w) => family === "all" || w.family === family);
  const options = filtered.map((w) => ({ value: w.id, label: workflowDisplayName(w) }));
  setOptions(el.workflowId, options.length ? options : [{ value: "", label: "No workflows available", disabled: true }]);
  loadWorkflowPreview();
}

function workflowDisplayName(workflow) {
  if (!workflow) return "Workflow";
  return (
    String(workflow.workflow_name || "").trim() ||
    String(workflow.name || "").trim() ||
    String(workflow.title || "").trim() ||
    String(workflow.display_name || "").trim() ||
    String(workflow.id || "").trim() ||
    "Workflow"
  );
}

function selectedWorkflow() {
  const selectedId = String(el.workflowId?.value || "").trim();
  if (!selectedId) return null;
  return (state.catalog?.workflows || []).find((w) => w.id === selectedId) || null;
}

function workflowExternalUrl(workflow) {
  if (!workflow) return "";
  const directLink =
    String(workflow.patreon_url || "").trim() ||
    String(workflow.workflow_url || "").trim() ||
    String(workflow.workflow_link_url || "").trim();
  if (directLink) return isSafeHttpUrl(directLink) ? directLink : "";

  const legacyUrl = String(workflow.workflow_json_url || "").trim();
  if (!legacyUrl || !isSafeHttpUrl(legacyUrl)) return "";
  try {
    const parsed = new URL(legacyUrl);
    const path = parsed.pathname.toLowerCase();
    const host = parsed.hostname.toLowerCase();
    if (host.includes("patreon.com") || !path.endsWith(".json")) {
      return legacyUrl;
    }
  } catch (_) {}
  return "";
}

function loadWorkflowPreview() {
  const workflow = selectedWorkflow();
  if (!workflow) {
    if (el.workflowPreviewImage) {
      el.workflowPreviewImage.classList.add("hidden");
      el.workflowPreviewImage.removeAttribute("src");
    }
    if (el.workflowPreviewCaption) {
      el.workflowPreviewCaption.textContent = "No workflow preview loaded.";
    }
    if (el.workflowYoutubeText) {
      el.workflowYoutubeText.textContent = "-";
    }
    if (el.workflowYoutubeLink) {
      el.workflowYoutubeLink.href = "#";
      el.workflowYoutubeLink.style.pointerEvents = "none";
    }
    updateDownloadButtons();
    return;
  }

  const rawPreviewUrl = String(workflow.preview_image_url || "").trim();
  const previewUrl = isSafeHttpUrl(rawPreviewUrl) ? rawPreviewUrl : "";
  if (!previewUrl) {
    if (el.workflowPreviewImage) {
      el.workflowPreviewImage.classList.add("hidden");
      el.workflowPreviewImage.removeAttribute("src");
    }
    if (el.workflowPreviewCaption) {
      el.workflowPreviewCaption.textContent = "No preview image available for this workflow.";
    }
    updateDownloadButtons();
    return;
  }

  if (el.workflowPreviewImage) {
    el.workflowPreviewImage.src = previewUrl;
    el.workflowPreviewImage.classList.remove("hidden");
  }
  if (el.workflowPreviewCaption) {
    el.workflowPreviewCaption.textContent = workflowDisplayName(workflow);
  }

  const rawYtUrl = String(workflow.youtube_url || "").trim();
  const ytUrl = isSafeHttpUrl(rawYtUrl) ? rawYtUrl : "";
  if (el.workflowYoutubeText) {
    el.workflowYoutubeText.textContent = ytUrl ? "Link" : "-";
  }
  if (el.workflowYoutubeLink) {
    el.workflowYoutubeLink.href = ytUrl || "#";
    el.workflowYoutubeLink.style.pointerEvents = ytUrl ? "auto" : "none";
  }
  updateDownloadButtons();
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
    const rawCreatorUrl = String(meta.creator_url || "").trim();
    const creatorUrl = isSafeHttpUrl(rawCreatorUrl) ? rawCreatorUrl : "";
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

  return {
    applyLoraPreview,
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
  };
}
