import test from "node:test";
import assert from "node:assert/strict";

import { normalizeSlashes, parentDir } from "../../src-tauri/dist/lib/path.js";
import { isSafeHttpUrl, isVideoPreviewUrl } from "../../src-tauri/dist/lib/url.js";

test("path helpers normalize Windows paths and preserve the nested-home workaround", () => {
  assert.equal(normalizeSlashes(" C:\\Users\\burce\\ComfyUI "), "C:/Users/burce/ComfyUI");
  assert.equal(normalizeSlashes("/tmp/src-tauri/home/burce/ComfyUI"), "/home/burce/ComfyUI");
  assert.equal(parentDir("C:\\Users\\burce\\ComfyUI"), "C:/Users/burce");
});

test("URL helpers reject local and executable schemes", () => {
  assert.equal(isSafeHttpUrl("https://example.com/model"), true);
  assert.equal(isSafeHttpUrl("http://example.com/model"), true);
  assert.equal(isSafeHttpUrl("file:///tmp/model"), false);
  assert.equal(isSafeHttpUrl("javascript:alert(1)"), false);
  assert.equal(isSafeHttpUrl("relative/model"), false);
});

test("video previews are recognized with or without query parameters", () => {
  assert.equal(isVideoPreviewUrl("https://example.com/preview.WEBM"), true);
  assert.equal(isVideoPreviewUrl("https://example.com/preview.mp4?download=1"), true);
  assert.equal(isVideoPreviewUrl("https://example.com/preview.png"), false);
});
