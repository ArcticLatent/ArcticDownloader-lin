import test from "node:test";
import assert from "node:assert/strict";

import { smoothedReceived } from "../../src-tauri/dist/features/download-progress.js";

test("download smoothing advances toward the received byte count without overshooting", () => {
  const transfer = { received: 1024 * 1024, displayReceived: 0, displayTs: 1000 };
  const first = smoothedReceived(transfer, 1100);
  assert.ok(first > 0);
  assert.ok(first <= transfer.received);

  transfer.received = 128 * 1024;
  const reduced = smoothedReceived(transfer, 1200);
  assert.ok(reduced <= transfer.received);
});
