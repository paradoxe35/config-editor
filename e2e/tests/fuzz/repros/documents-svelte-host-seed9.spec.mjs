import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 9 on documents-svelte.
// The handle's `applyChanges(id, …)` is the host's own change: the parent's
// list hears it through `onChange` and ends on the document.
test("documents-svelte: host (seed 9)", async ({ browser }) => {
  await replay(browser, "documents-svelte", "", [
    {
      "k": "hostChange",
      "at": 0.735,
      "base": false,
      "line": "host_key: 1"
    }
  ], "host");
});
