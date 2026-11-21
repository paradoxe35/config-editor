import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 11 on documents (?revisions).
// The host's change through a document's editor (`editor(id).applyChanges`)
// is that editor's value at once: listed back with its revision, the lock
// stays marked.
test("documents: lock-marks (seed 11)", async ({ browser }) => {
  await replay(browser, "documents", "?revisions", [
    {
      "k": "lock",
      "range": {
        "at": 0.123,
        "length": 3,
        "line": false
      }
    },
    {
      "k": "click",
      "x": 0.502,
      "y": 0.734
    },
    {
      "k": "hostChange",
      "at": 0.428,
      "base": true,
      "line": "# host note"
    }
  ], "lock-marks");
});
