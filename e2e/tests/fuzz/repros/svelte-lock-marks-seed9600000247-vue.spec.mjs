import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 9600000247 on vue (run on svelte too).
// A lock given by range is marked on its text after a host change and a
// hide and show of the editor, late echoes on their way.
test("svelte: lock-marks (seed 9600000247, vue sequence)", async ({ browser }) => {
  await replay(browser, "svelte", "?random", [
    { "k": "lock", "range": { "at": 0.025, "length": 3, "line": true } },
    { "k": "hostChange", "at": 0.181, "base": true, "line": "host_key: 1" },
    { "k": "hideShow", "wait": 0 },
    { "k": "suggest", "open": true, "pick": 2 }
  ], "lock-marks");
});
