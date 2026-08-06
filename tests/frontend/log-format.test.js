import test from "node:test";
import assert from "node:assert/strict";

import {
  ansiToHtml,
  detectRuntimeLogLevel,
  escapeHtml,
} from "../../src-tauri/dist/lib/log-format.js";

test("escapeHtml neutralizes text that could become markup", () => {
  assert.equal(escapeHtml("<node>&value>"), "&lt;node&gt;&amp;value&gt;");
});

test("ansiToHtml preserves text and maps supported styles to CSS classes", () => {
  assert.equal(
    ansiToHtml("plain \u001b[1;31mfailed <node>\u001b[0m ready"),
    "plain <span class=\"ansi-bold ansi-fg-31\">failed &lt;node&gt;</span> ready",
  );
});

test("ansiToHtml handles foreground and bold resets independently", () => {
  assert.equal(
    ansiToHtml("\u001b[92mgreen\u001b[39m normal \u001b[1mbold\u001b[22m plain"),
    "<span class=\"ansi-fg-92\">green</span> normal <span class=\"ansi-bold\">bold</span> plain",
  );
});

test("detectRuntimeLogLevel classifies important runtime messages", () => {
  assert.equal(detectRuntimeLogLevel("model load failed"), "error");
  assert.equal(detectRuntimeLogLevel("deprecated fallback selected"), "warn");
  assert.equal(detectRuntimeLogLevel("server listening on port 8188"), "success");
  assert.equal(detectRuntimeLogLevel("loading checkpoint"), "info");
});
