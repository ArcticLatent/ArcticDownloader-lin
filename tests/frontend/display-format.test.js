import test from "node:test";
import assert from "node:assert/strict";

import {
  formatBytes,
  formatFileSize,
  formatVramMbToGb,
  trimDescription,
} from "../../src-tauri/dist/lib/display-format.js";

test("byte formatters preserve their existing progress and catalog precision", () => {
  assert.equal(formatBytes(0), "0 B");
  assert.equal(formatBytes(1536), "1.5 KB");
  assert.equal(formatFileSize(9 * 1024 * 1024), "9.0 MB");
  assert.equal(formatFileSize(12 * 1024 * 1024), "12 MB");
});

test("VRAM and descriptions handle empty and bounded values", () => {
  assert.equal(formatVramMbToGb(24576), "24.0 GB VRAM");
  assert.equal(formatVramMbToGb(null), null);
  assert.equal(trimDescription(""), "-");
  assert.equal(trimDescription("abcdef", 4), "abcd...");
});
