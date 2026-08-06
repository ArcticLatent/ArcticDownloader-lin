export function escapeHtml(text) {
  return String(text || "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

export function ansiToHtml(text) {
  const input = String(text || "");
  let html = "";
  let classes = [];

  const flush = (chunk) => {
    if (!chunk) return;
    const escaped = escapeHtml(chunk);
    html += classes.length
      ? `<span class="${classes.join(" ")}">${escaped}</span>`
      : escaped;
  };

  let index = 0;
  while (index < input.length) {
    const escIndex = input.indexOf("\u001b[", index);
    if (escIndex < 0) {
      flush(input.slice(index));
      break;
    }
    flush(input.slice(index, escIndex));
    const match = /^\u001b\[([0-9;]*)m/.exec(input.slice(escIndex));
    if (!match) {
      flush(input.slice(escIndex, escIndex + 1));
      index = escIndex + 1;
      continue;
    }
    const codes = (match[1] || "0")
      .split(";")
      .map((part) => Number(part || 0))
      .filter((code) => Number.isFinite(code));
    if (codes.length === 0 || codes.includes(0)) {
      classes = [];
    }
    if (codes.includes(1)) {
      classes = classes.filter((name) => name !== "ansi-bold");
      classes.push("ansi-bold");
    }
    codes.forEach((code) => {
      if ((code >= 30 && code <= 37) || (code >= 90 && code <= 97)) {
        classes = classes.filter((name) => !/^ansi-fg-/.test(name));
        classes.push(`ansi-fg-${code}`);
      }
      if (code === 39) {
        classes = classes.filter((name) => !/^ansi-fg-/.test(name));
      }
      if (code === 22) {
        classes = classes.filter((name) => name !== "ansi-bold");
      }
    });
    index = escIndex + match[0].length;
  }
  return html;
}

export function detectRuntimeLogLevel(text) {
  const value = String(text || "").toLowerCase();
  if (
    /traceback|fatal|exception|error|failed|cannot|could not|invalid|denied/.test(value)
  ) {
    return "error";
  }
  if (/warn|warning|deprecated|retry|fallback|slow|stall/.test(value)) {
    return "warn";
  }
  if (/started|ready|listening|loaded|completed|success|using /.test(value)) {
    return "success";
  }
  return "info";
}
