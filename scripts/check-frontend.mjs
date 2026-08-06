import { readdirSync } from "node:fs";
import { extname, join } from "node:path";
import { spawnSync } from "node:child_process";

const roots = ["src-tauri/dist", "tests/frontend"];

function javascriptFiles(directory) {
  return readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) => {
      const path = join(directory, entry.name);
      return entry.isDirectory() ? javascriptFiles(path) : [path];
    })
    .filter((path) => [".js", ".mjs"].includes(extname(path)))
    .sort();
}

for (const file of roots.flatMap(javascriptFiles)) {
  const result = spawnSync(process.execPath, ["--check", file], { stdio: "inherit" });
  if (result.status !== 0) process.exit(result.status ?? 1);
}
