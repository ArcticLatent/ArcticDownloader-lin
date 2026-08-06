import { readFileSync, readdirSync } from "node:fs";
import { dirname, extname, join, relative, resolve } from "node:path";
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

const files = roots.flatMap(javascriptFiles);

for (const file of files) {
  const result = spawnSync(process.execPath, ["--check", file], { stdio: "inherit" });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const frontendFiles = javascriptFiles("src-tauri/dist").map((file) => resolve(file));
const frontendFileSet = new Set(frontendFiles);
const dependencyGraph = new Map(frontendFiles.map((file) => [file, []]));
const importPattern = /(?:import|export)\s+(?:[^"']*?\s+from\s+)?["']([^"']+)["']/g;

for (const file of frontendFiles) {
  const source = readFileSync(file, "utf8");
  for (const match of source.matchAll(importPattern)) {
    const specifier = match[1];
    if (!specifier.startsWith(".")) continue;
    const dependency = resolve(dirname(file), specifier);
    if (!frontendFileSet.has(dependency)) {
      throw new Error(`Missing frontend module imported by ${relative(".", file)}: ${specifier}`);
    }
    dependencyGraph.get(file).push(dependency);
  }
}

const visiting = new Set();
const visited = new Set();

function visit(file, trail = []) {
  if (visiting.has(file)) {
    const cycleStart = trail.indexOf(file);
    const cycle = trail.slice(cycleStart).concat(file).map((item) => relative(".", item));
    throw new Error(`Frontend module cycle detected: ${cycle.join(" -> ")}`);
  }
  if (visited.has(file)) return;
  visiting.add(file);
  for (const dependency of dependencyGraph.get(file)) {
    visit(dependency, trail.concat(file));
  }
  visiting.delete(file);
  visited.add(file);
}

for (const file of frontendFiles) visit(file);
