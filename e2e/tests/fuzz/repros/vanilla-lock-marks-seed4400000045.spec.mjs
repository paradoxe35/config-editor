import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 4400000045 on vanilla.
// A lock holding a piece of a secret placeholder: the placeholder shows as a secret, so the check judges no mark there.
test("vanilla: lock-marks (seed 4400000045)", async ({ browser }) => {
  await replay(browser, "vanilla", "", [
    {
      "k": "hostValue",
      "text": "clients:\n  acme:\n    secret: «secret 3f2a»\n",
      "how": "prop"
    },
    {
      "k": "lock",
      "range": {
        "at": 0.734,
        "length": 8,
        "line": false
      }
    }
  ], "lock-marks");
});
