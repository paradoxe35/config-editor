import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 8800000081 on react.
// P1: a range the host locks stays marked whole after the parent
// re-mounts the editor under a new file name.
test("react: lock-marks (seed 8800000081)", async ({ browser }) => {
  await replay(browser, "react", "", [
    {
      "k": "lock",
      "range": {
        "at": 0.25,
        "length": 30,
        "line": false
      }
    },
    {
      "k": "suggestMouse",
      "outside": false,
      "pick": 2
    },
    {
      "k": "prop",
      "prop": "rename"
    }
  ], "lock-marks");
});
