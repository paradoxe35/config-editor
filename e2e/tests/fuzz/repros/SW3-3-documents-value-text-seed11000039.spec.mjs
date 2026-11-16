import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// P1 guard (for the redesign): a placeholder pasted into another document, then a line break typed after it: the host value never gets the line break (it differs from the text shown, with no reason given).
// Shrunk by the browser fuzz from seed 11000039 on documents.
// Fails the `value-text` check after step 1: value "clients:\n  acme:\n  «secret 3f2a»port  secret: «secret 3f2a»\n" text "clients:\n  acme:\n  «secret 3f2a»port\n  secret: «secret 3f2a»\n"
test("SW3-3 documents: value-text (seed 11000039)", async ({ browser }) => {
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
