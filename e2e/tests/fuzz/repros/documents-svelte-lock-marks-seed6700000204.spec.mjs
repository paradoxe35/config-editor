import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 6700000204 on documents-svelte.
// A lock given by range keeps its mark through redo, typing, a copy into
// another document and a host change.
test("documents-svelte: lock-marks (seed 6700000204)", async ({ browser }) => {
  await replay(browser, "documents-svelte", "?random", [
    { "k": "lock", "range": { "at": 0.339, "length": 3, "line": false } },
    { "k": "key", "key": "Control+Y", "n": 2 },
    { "k": "click", "x": 0.362, "y": 0.966 },
    { "k": "type", "text": "  Ω" },
    { "k": "tab", "index": 2 },
    { "k": "crossCopy", "from": 1, "to": 0, "load": false, "token": false, "at": 0.616, "length": 5, "at2": 0.43, "over": 0 },
    { "k": "key", "key": "Control+]", "n": 1 },
    { "k": "key", "key": "Shift+ArrowUp", "n": 1 },
    { "k": "hostChange", "at": 0.914, "base": true, "line": "# host note" },
    { "k": "key", "key": "ArrowRight", "n": 1 }
  ], "lock-marks");
});
