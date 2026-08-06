// Catalog-sourced values may become preview/link targets or backend
// open_external_url arguments, so only absolute HTTP(S) URLs are allowed.
export function isSafeHttpUrl(value) {
  const trimmed = String(value || "").trim();
  if (!trimmed) return false;
  try {
    const parsed = new URL(trimmed);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

export function isVideoPreviewUrl(url) {
  const value = String(url || "").toLowerCase();
  return value.endsWith(".mp4") || value.endsWith(".webm") || value.endsWith(".mov")
    || value.includes(".mp4?") || value.includes(".webm?") || value.includes(".mov?");
}
