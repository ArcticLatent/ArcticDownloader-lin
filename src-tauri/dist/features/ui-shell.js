import { DOT_SEP, el, state } from "../lib/app-context.js";
import { ansiToHtml, detectRuntimeLogLevel, escapeHtml } from "../lib/log-format.js";

export function createUiShell() {
// Caps how large a log panel's text can grow to. These panels prepend one
// line per event for the life of the app session; without a cap, a long
// session (or a chatty ComfyUI install) grows the text node -- and the
// cost of every future prepend -- without bound.
const LOG_MAX_CHARS = 200_000;

function prependLogLine(target, text) {
  const stamp = new Date()
    .toLocaleTimeString([], { hour: "numeric", minute: "2-digit", hour12: true })
    .replace(/\s+/g, " ")
    .toUpperCase();
  const combined = `[${stamp}] ${text}\n` + target.textContent;
  target.textContent = combined.length > LOG_MAX_CHARS
    ? combined.slice(0, LOG_MAX_CHARS)
    : combined;
}

function logLine(text) {
  prependLogLine(el.statusLog, text);
}

function logComfyLine(text) {
  if (!el.comfyInstallLog) return;
  prependLogLine(el.comfyInstallLog, text);
}

function runtimeLogMatchesFilter(entry) {
  const filter = String(el.comfyRuntimeLogFilter?.value || "all");
  if (filter === "stderr") return entry.stream === "stderr";
  if (filter === "stdout") return entry.stream === "stdout";
  if (filter === "important") return entry.level === "warn" || entry.level === "error";
  return true;
}

// ComfyUI's stdout/stderr can emit many lines per second (model loading,
// verbose custom nodes); logComfyRuntimeLine used to call
// renderComfyRuntimeLogs() -- a full innerHTML wipe + ANSI-parsed rebuild
// of up to 500 rows -- synchronously on every single line, which visibly
// janks the UI exactly when it should feel responsive (during install/
// startup). Coalescing into at most one render per animation frame keeps
// the log visually live without redoing that work per line.
let comfyRuntimeLogRenderScheduled = false;

function scheduleComfyRuntimeLogRender() {
  if (comfyRuntimeLogRenderScheduled) return;
  comfyRuntimeLogRenderScheduled = true;
  requestAnimationFrame(() => {
    comfyRuntimeLogRenderScheduled = false;
    renderComfyRuntimeLogs();
  });
}

function renderComfyRuntimeLogs() {
  if (!el.comfyRuntimeLog) return;
  el.comfyRuntimeLog.innerHTML = "";
  const visible = state.comfyRuntimeLogs.filter(runtimeLogMatchesFilter);
  if (!visible.length) {
    el.comfyRuntimeLog.textContent = "Ready";
    return;
  }
  visible.forEach((entry) => {
    const line = document.createElement("div");
    line.className = `ansi-log-line stream-${entry.stream} level-${entry.level}`;
    line.innerHTML = `<span class="ansi-log-meta">[${escapeHtml(entry.stamp)}]</span><span class="ansi-log-dot" aria-hidden="true"></span>${ansiToHtml(entry.text)}`;
    el.comfyRuntimeLog.appendChild(line);
  });
}

function logComfyRuntimeLine(text, stream = "stdout") {
  const stamp = new Date()
    .toLocaleTimeString([], { hour: "numeric", minute: "2-digit", hour12: true })
    .replace(/\s+/g, " ")
    .toUpperCase();
  state.comfyRuntimeLogs.unshift({
    text: String(text || ""),
    stream: String(stream || "stdout").toLowerCase() === "stderr" ? "stderr" : "stdout",
    stamp,
    level: detectRuntimeLogLevel(text),
  });
  if (state.comfyRuntimeLogs.length > 500) {
    state.comfyRuntimeLogs.length = 500;
  }
  scheduleComfyRuntimeLogRender();
}

function setStartupStatus(text) {
  if (!el.startupStatus) return;
  el.startupStatus.textContent = String(text || "Preparing workspace...");
}

function hideStartupOverlay() {
  const overlay = el.startupOverlay;
  if (!overlay || overlay.classList.contains("hidden")) return;
  overlay.classList.add("is-hiding");
  window.setTimeout(() => {
    overlay.classList.add("hidden");
    overlay.classList.remove("is-hiding");
    overlay.setAttribute("aria-busy", "false");
  }, 240);
}

function showBlockingOverlay(text) {
  const overlay = el.startupOverlay;
  if (!overlay) return;
  setStartupStatus(text || "Working...");
  overlay.classList.remove("hidden");
  overlay.classList.remove("is-hiding");
  overlay.setAttribute("aria-busy", "true");
}

function notifySystem(title, body) {
  const tauriNotify = window.__TAURI__?.notification;
  if (tauriNotify?.sendNotification) {
    try {
      tauriNotify.sendNotification({ title, body });
      return;
    } catch (_) {}
  }
  if (!("Notification" in window)) return;
  const send = () => {
    try {
      new Notification(title, { body });
    } catch (_) {}
  };
  if (Notification.permission === "granted") {
    send();
    return;
  }
  if (Notification.permission !== "denied") {
    Notification.requestPermission().then((perm) => {
      if (perm === "granted") send();
    }).catch(() => {});
  }
}

function setToggleBusy(box, busy) {
  if (!box) return;
  box.disabled = Boolean(busy);
  const label = box.closest("label");
  if (!label) return;
  label.classList.toggle("busy", Boolean(busy));
}

function showConfirmDialog(message) {
  return new Promise((resolve) => {
    const overlay = el.confirmOverlay;
    const messageEl = el.confirmMessage;
    const yesBtn = el.confirmYes;
    const noBtn = el.confirmNo;
    if (!overlay || !messageEl || !yesBtn || !noBtn) {
      resolve(window.confirm(message));
      return;
    }

    let settled = false;
    const close = (value) => {
      if (settled) return;
      settled = true;
      overlay.classList.add("hidden");
      overlay.setAttribute("aria-hidden", "true");
      yesBtn.removeEventListener("click", onYes);
      noBtn.removeEventListener("click", onNo);
      overlay.removeEventListener("click", onOverlay);
      window.removeEventListener("keydown", onKeyDown);
      resolve(value);
    };
    const onYes = () => close(true);
    const onNo = () => close(false);
    const onOverlay = (event) => {
      if (event.target === overlay) close(false);
    };
    const onKeyDown = (event) => {
      if (event.key === "Escape") close(false);
    };

    messageEl.textContent = String(message || "Are you sure?");
    yesBtn.addEventListener("click", onYes);
    noBtn.addEventListener("click", onNo);
    overlay.addEventListener("click", onOverlay);
    window.addEventListener("keydown", onKeyDown);
    window.requestAnimationFrame(() => {
      overlay.classList.remove("hidden");
      overlay.setAttribute("aria-hidden", "false");
      window.requestAnimationFrame(() => yesBtn.focus());
    });
  });
}

function waitForNextPaint() {
  return new Promise((resolve) => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(resolve);
    });
  });
}
function renderTitleMeta() {
  const base = state.titleSystemText || "Loading system info...";
  const comfy = String(state.selectedComfyVersion || "").trim();
  if (!comfy) {
    el.version.textContent = base;
    return;
  }
  const label = comfy.toLowerCase().startsWith("v") ? comfy : `v${comfy}`;
  el.version.textContent = `${base}${DOT_SEP}ComfyUI ${label}`;
  const latest = String(state.comfyLatestVersion || "").trim();
  if (state.comfyUpdateAvailable) {
    const badge = document.createElement("span");
    badge.className = "latest-version-badge";
    if (latest) {
      const latestLabel = latest.toLowerCase().startsWith("v") ? latest : `v${latest}`;
      badge.textContent = `${DOT_SEP}(latest ${latestLabel})`;
    } else {
      badge.textContent = `${DOT_SEP}(update available)`;
    }
    el.version.appendChild(badge);
  }
}

function renderAppVersionTag() {
  if (!el.appVersionTag) return;
  const normalizeVersion = (value) => String(value || "").trim().replace(/^v/i, "");
  const current = normalizeVersion(state.appVersion || "");
  const latest = normalizeVersion(state.updateVersion || "");
  if (state.updateInstalling) {
    el.appVersionTag.textContent = "Updating...";
    el.appVersionTag.classList.remove("update-available");
    return;
  }
  if (state.updateAvailable && state.updateVersion) {
    el.appVersionTag.textContent = latest;
    el.appVersionTag.classList.add("update-available");
    return;
  }
  el.appVersionTag.textContent = current || "...";
  el.appVersionTag.classList.remove("update-available");
}

function updateUpdateButton() {
  if (!el.checkUpdates) return;
  el.checkUpdates.classList.remove("update-available");
  if (state.updateChecking) {
    el.checkUpdates.textContent = "Checking...";
    el.checkUpdates.disabled = true;
    renderAppVersionTag();
    return;
  }
  if (state.updateInstalling) {
    el.checkUpdates.textContent = "Updating...";
    el.checkUpdates.disabled = true;
    renderAppVersionTag();
    return;
  }
  el.checkUpdates.disabled = false;
  el.checkUpdates.textContent = state.updateAvailable ? "Update" : "Check Updates";
  if (state.updateAvailable) {
    el.checkUpdates.classList.add("update-available");
  }
  renderAppVersionTag();
}

  return {
    hideStartupOverlay,
    logComfyLine,
    logComfyRuntimeLine,
    logLine,
    notifySystem,
    renderAppVersionTag,
    renderComfyRuntimeLogs,
    renderTitleMeta,
    setStartupStatus,
    setToggleBusy,
    showBlockingOverlay,
    showConfirmDialog,
    updateUpdateButton,
    waitForNextPaint,
  };
}
