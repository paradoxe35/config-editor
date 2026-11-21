import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 34 on react (?revisions).
// The handle's `applyChanges` is the host's own change: the parent hears
// it through `onChange`, as `setValue`'s reset, and ends on the document.
test("react: host (seed 34)", async ({ browser }) => {
  await replay(browser, "react", "?revisions", [
    {
      "k": "hostChange",
      "at": 0.936,
      "base": true,
      "line": "host_key: 1"
    }
  ], "host");
});
