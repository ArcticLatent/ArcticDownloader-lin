import test from "node:test";
import assert from "node:assert/strict";

import { debounce } from "../../src-tauri/dist/lib/timing.js";

test("debounce invokes only the latest call", async () => {
  const calls = [];
  const debounced = debounce((value) => calls.push(value), 5);
  debounced("first");
  debounced("second");
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.deepEqual(calls, ["second"]);
});
