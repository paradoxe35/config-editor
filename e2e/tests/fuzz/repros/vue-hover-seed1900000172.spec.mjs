import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 1900000172 on vue (?random).
// A notice that shows late pushes the rows down under the resting pointer; the card (made for the row the pointer was on) follows it once the browser sends its mouse move, as the check now does.
test("vue: hover (seed 1900000172)", async ({ browser }) => {
  await replay(browser, "vue", "?random", [
    {
      "k": "editAs",
      "language": "xml"
    },
    {
      "k": "key",
      "key": "PageDown",
      "n": 1
    },
    {
      "k": "type",
      "text": "ß"
    },
    {
      "k": "key",
      "key": "Control+/",
      "n": 1
    },
    {
      "k": "hover",
      "at": 0.543,
      "column": 0.119,
      "into": false,
      "escape": false
    }
  ], "hover");
});
