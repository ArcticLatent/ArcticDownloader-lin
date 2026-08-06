export const PATH_SEP = "/";

export function normalizeSlashes(value) {
  const raw = String(value || "").trim();
  if (!raw) return "";
  const normalized = raw.replace(/[\\/]+/g, PATH_SEP);
  const nestedHome = `${PATH_SEP}src-tauri${PATH_SEP}home${PATH_SEP}`;
  const index = normalized.indexOf(nestedHome);
  if (index >= 0) {
    return normalized.slice(index + `${PATH_SEP}src-tauri`.length);
  }
  return normalized;
}

export function parentDir(path) {
  const normalized = normalizeSlashes(path);
  const index = normalized.lastIndexOf(PATH_SEP);
  if (index <= 0) return normalized;
  return normalized.slice(0, index);
}
