import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 6700000176 on svelte.
// A lock given by range is marked on the same characters after an edit
// and a hide and show of the editor.
test("svelte: lock-marks (seed 6700000176)", async ({ browser }) => {
  await replay(browser, "svelte", "", [
    { "k": "lock", "range": { "at": 0.692, "length": 14, "line": false } },
    { "k": "suggest", "open": true, "pick": 0 },
    { "k": "hideShow", "wait": 0 }
  ], "lock-marks");
});
