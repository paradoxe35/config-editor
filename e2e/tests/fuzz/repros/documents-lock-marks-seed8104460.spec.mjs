import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 8104460 on documents.
// P1: a range the host locks while a save is held (a line copied) is
// locked, and marked, where the text on screen holds its text whole.
test("documents: lock-marks (seed 8104460)", async ({ browser }) => {
  await replay(browser, "documents", "", [
    {
      "k": "lock",
      "range": {
        "at": 0.011,
        "length": 3,
        "line": false
      }
    },
    {
      "k": "key",
      "key": "Shift+Alt+ArrowUp",
      "n": 1
    },
    {
      "k": "lock",
      "range": {
        "at": 0.165,
        "length": 8,
        "line": true
      }
    }
  ], "lock-marks");
});
