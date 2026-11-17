import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 1500000297 on documents.
// P1: a range the host locks while a save is held (a drag moved part of a
// key) is mapped through the edits the host has not heard, onto its text.
test("documents: lock-marks (seed 1500000297)", async ({ browser }) => {
  await replay(browser, "documents", "", [
    {
      "k": "lock",
      "range": {
        "at": 0.837,
        "length": 5,
        "line": false
      }
    },
    {
      "k": "dragMove",
      "at": 0.291,
      "length": 3,
      "x": 0.309,
      "y": 0.287
    },
    {
      "k": "lock",
      "range": {
        "at": 0.615,
        "length": 14,
        "line": false
      }
    }
  ], "lock-marks");
});
