import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// P1, P4: a placeholder pasted into another document, then a line break typed after it: while the host value differs from the text shown, a "Not saved" problem says why.
// Shrunk by the browser fuzz from seed 11000039 on documents.
// Failed the `value-text` check after step 1: value "clients:\n  acme:\n  «secret 3f2a»port  secret: «secret 3f2a»\n" text "clients:\n  acme:\n  «secret 3f2a»port\n  secret: «secret 3f2a»\n"
test("documents: value-text (seed 11000039)", async ({ browser }) => {
  await replay(browser, "documents", "", [
    {
      "k": "crossCopy",
      "from": 0,
      "to": 0,
      "load": true,
      "token": true,
      "at": 0.738,
      "length": 1,
      "at2": 0.456,
      "over": 0
    },
    {
      "k": "type",
      "text": "port\n"
    }
  ], "value-text");
});
