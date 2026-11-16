import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 8104462 on react.
// P1: a range the host locks while a save is held (a key typed into past
// an earlier range) names text the held edits already typed over: the
// save keeps waiting, and the host never hears a text without it.
test("react: lock (seed 8104462)", async ({ browser }) => {
  await replay(browser, "react", "", [
    {
      "k": "lock",
      "range": {
        "at": 0.707,
        "length": 3,
        "line": false
      }
    },
    {
      "k": "type",
      "text": "😀👨‍👩‍👧"
    },
    {
      "k": "dragMove",
      "at": 0.283,
      "length": 40,
      "x": 0.588,
      "y": 0.269
    },
    {
      "k": "type",
      "text": "k"
    },
    {
      "k": "lock",
      "range": {
        "at": 0.457,
        "length": 5,
        "line": false
      }
    }
  ], "lock");
});
