import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 2900000495 on documents.
// Fails the `secret-glue` check after step 4: line 3 "    secret: x«secret 3f2a»" glues «secret 3f2a», and no problem is listed there
test("documents: secret-glue (seed 2900000495)", async ({ browser }) => {
  await replay(browser, "documents", "", [
    {
      "k": "crossCopy",
      "from": 0,
      "to": 2,
      "load": true,
      "token": true,
      "at": 0.938,
      "length": 5,
      "at2": 0.944,
      "over": 4
    },
    {
      "k": "tab",
      "index": 3
    },
    {
      "k": "type",
      "text": "x"
    },
    {
      "k": "key",
      "key": "Shift+Alt+ArrowUp",
      "n": 1
    },
    {
      "k": "clip",
      "key": "Control+V"
    }
  ], "secret-glue");
});
