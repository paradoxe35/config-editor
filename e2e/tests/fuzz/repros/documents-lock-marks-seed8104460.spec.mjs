import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 8104460 on documents.
// Fails the `lock-marks` check after step 2: line 2: locked "listen: 7835", marked ""
test.fixme("documents: lock-marks (seed 8104460)", async ({ browser }) => {
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
