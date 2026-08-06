import { formatBytes } from "../lib/display-format.js";

const DOT_SEP = " \u2022 ";
const COMPLETED_HISTORY_MAX = 200;

export function smoothedReceived(item, now = Date.now()) {
  const target = Math.max(0, Number(item.received || 0));
  if (!Number.isFinite(item.displayReceived)) item.displayReceived = 0;
  if (!Number.isFinite(item.displayTs)) item.displayTs = now;
  if (item.displayReceived > target) item.displayReceived = target;

  const dtMs = Math.max(16, now - item.displayTs);
  item.displayTs = now;
  const delta = target - item.displayReceived;
  if (delta <= 0) return item.displayReceived;

  const minStep = 128 * 1024;
  const easedStep = delta * 0.25;
  const rateCapStep = (dtMs / 1000) * (320 * 1024 * 1024);
  const advance = Math.min(
    delta,
    Math.max(minStep, Math.min(rateCapStep, Math.max(minStep, easedStep))),
  );
  item.displayReceived += advance;
  return item.displayReceived;
}

export function createDownloadProgress({
  state,
  elements,
  invoke,
  logLine,
  selectedWorkflow,
  workflowExternalUrl,
}) {
  const el = elements;
  let progressSmoothTimer = null;

  function setProgress(text) {
    el.progressLine.textContent = text || "Idle";
  }

  function ensureProgressSmoother() {
    if (progressSmoothTimer) return;
    progressSmoothTimer = window.setInterval(() => {
      const active = [...state.transfers.values()]
        .filter((item) => item.phase !== "finished" && item.phase !== "failed");
      if (!active.length && state.busyDownloads <= 0) {
        window.clearInterval(progressSmoothTimer);
        progressSmoothTimer = null;
        return;
      }
      renderActiveTransfers();
      renderOverallProgress();
    }, 140);
  }

  function renderOverallProgress() {
    const active = [...state.transfers.values()]
      .filter((item) => item.phase !== "finished" && item.phase !== "failed");
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

    const lead = active[0];
    if (lead) {
      const leadSize = Number(lead.size || 0);
      const leadShown = leadSize > 0
        ? Math.min(smoothedReceived(lead), leadSize)
        : smoothedReceived(lead);
      const leadPct = leadSize > 0 ? ` ${Math.round((leadShown / leadSize) * 100)}%` : "";
      setProgress(`[${lead.kind || "download"}] ${lead.artifact || "file"}${leadPct}`);
    }

    const known = active.filter((item) => Number(item.size || 0) > 0);
    if (!known.length) {
      el.overallProgress.classList.add("indeterminate");
      el.overallProgressFill.style.removeProperty("width");
      const activeCount = Math.max(active.length, state.busyDownloads > 0 ? 1 : 0);
      el.overallProgressMeta.textContent = `Downloading ${activeCount} file(s)...`;
      return;
    }

    const totalBytes = known.reduce((sum, item) => sum + Number(item.size || 0), 0);
    const receivedBytes = known.reduce(
      (sum, item) => sum + Math.min(smoothedReceived(item), Number(item.size || 0)),
      0,
    );
    const pct = totalBytes > 0
      ? Math.max(0, Math.min(100, Math.round((receivedBytes / totalBytes) * 100)))
      : 0;
    const unknownCount = Math.max(0, active.length - known.length);

    el.overallProgress.classList.remove("indeterminate");
    el.overallProgressFill.style.width = `${pct}%`;
    el.overallProgressMeta.textContent = unknownCount > 0
      ? `${pct}%${DOT_SEP}${formatBytes(receivedBytes)} / ${formatBytes(totalBytes)}${DOT_SEP}${known.length} known + ${unknownCount} unknown`
      : `${pct}%${DOT_SEP}${formatBytes(receivedBytes)} / ${formatBytes(totalBytes)}${DOT_SEP}${known.length} active`;
  }

  function updateDownloadButtons() {
    const cancelling = state.busyDownloads > 0;
    if (cancelling) {
      el.downloadModel.textContent = "Cancel Download";
      el.downloadLora.textContent = "Cancel Download";
      el.downloadWorkflow.textContent = "Cancel Download";
    } else {
      el.downloadModel.textContent = "Download Model Assets";
      el.downloadLora.textContent = "Download LoRA";
      el.downloadWorkflow.textContent = workflowExternalUrl(selectedWorkflow())
        ? "Open Workflow Link"
        : "Download Workflow";
    }
  }

  function beginBusyDownload(label) {
    state.busyDownloads += 1;
    if (!state.activeDownloadKind) {
      if (state.activeTab === "loras") {
        state.activeDownloadKind = "lora";
      } else if (state.activeTab === "workflows") {
        state.activeDownloadKind = "workflow";
      } else {
        state.activeDownloadKind = "model";
      }
    }
    setProgress(label || "Downloading...");
    updateDownloadButtons();
    renderOverallProgress();
    ensureProgressSmoother();
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
    } catch (error) {
      logLine(`Cancel failed: ${error}`);
      endBusyDownload();
    }
  }

  function renderActiveTransfers() {
    const now = Date.now();
    const active = [...state.transfers.values()]
      .filter((item) => item.phase !== "finished" && item.phase !== "failed");
    el.transferList.innerHTML = "";
    if (!active.length) {
      const message = document.createElement("div");
      message.className = "empty-msg";
      message.textContent = "No active transfers.";
      el.transferList.appendChild(message);
    }
    for (const item of active) {
      const smoothed = smoothedReceived(item);
      const shownReceived = item.size > 0 ? Math.min(smoothed, item.size) : smoothed;
      const pct = item.size > 0
        ? Math.max(0, Math.min(100, Math.round((shownReceived / item.size) * 100)))
        : 0;
      const quietMs = now - Number(item.lastUpdateTs || now);
      const nearEnd = item.size > 0 && shownReceived >= item.size * 0.9;
      const finalizing = item.phase === "progress" && nearEnd && quietMs > 2500;
      const phaseLabel = finalizing ? "finalizing" : item.phase;
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
        ? `${phaseLabel}${DOT_SEP}${formatBytes(shownReceived)} / ${formatBytes(item.size)}`
        : phaseLabel;
      row.appendChild(title);
      row.appendChild(bar);
      row.appendChild(sub);
      el.transferList.appendChild(row);
    }
  }

  function renderCompletedTransfers() {
    el.completedList.innerHTML = "";
    if (!state.completed.length) {
      const message = document.createElement("div");
      message.className = "empty-msg";
      message.textContent = "No completed downloads.";
      el.completedList.appendChild(message);
      return;
    }

    const max = Math.min(30, state.completed.length);
    for (let index = 0; index < max; index += 1) {
      const item = state.completed[index];
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
          } catch (error) {
            logLine(`Open folder failed: ${error}`);
          }
        });
      }
      row.appendChild(title);
      row.appendChild(sub);
      row.appendChild(button);
      el.completedList.appendChild(row);
    }
  }

  function renderTransfers() {
    renderActiveTransfers();
    renderCompletedTransfers();
    renderOverallProgress();
  }

  function addCompleted(item) {
    const index = state.completed.findIndex(
      (completed) => completed.name === item.name
        && completed.status === item.status
        && completed.folder === (item.folder || ""),
    );
    if (index >= 0) {
      if (item.folder && item.folder.trim()) {
        state.completed[index].folder = item.folder;
      }
      return;
    }

    state.completed.unshift({
      id: `done-${Date.now()}-${state.completedSeq++}`,
      name: item.name,
      folder: item.folder || "",
      status: item.status,
    });
    if (state.completed.length > COMPLETED_HISTORY_MAX) {
      state.completed.length = COMPLETED_HISTORY_MAX;
    }
  }

  return {
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
  };
}
