import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 73056 on documents-react.
// Fails the `secret` check after step 3: "«secret c3f2a»" in "[clients«secret 3f2a»\"\ne]\nsecret = \"«secret c3f2a»\"\n"
test("documents-react: secret (seed 73056)", async ({ browser }) => {
  await replay(browser, "documents-react", "", [
    {
      "k": "crossCopy",
      "from": 1,
      "to": 1,
      "load": true,
      "token": false,
      "at": 0.871,
      "length": 14,
      "at2": 0.223,
      "over": 4
    },
    {
      "k": "key",
      "key": "Control+Z",
      "n": 1
    },
    {
      "k": "key",
      "key": "Control+Y",
      "n": 1
    },
    {
      "k": "crossCopy",
      "from": 2,
      "to": 1,
      "load": false,
      "token": false,
      "at": 0.298,
      "length": 1,
      "at2": 0.865,
      "over": 0
    }
  ], "secret");
});
