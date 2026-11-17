import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 1500000306 on react (?random).
// P4: with late echoes, the host follows the text on screen after drags,
// typing over the dropped text and a suggestion picked.
test("react: host (seed 1500000306)", async ({ browser }) => {
  await replay(browser, "react", "?random", [
    {
      "k": "click",
      "x": 0.051,
      "y": 0.285
    },
    {
      "k": "chord",
      "keys": [
        "Control+Shift+BracketLeft"
      ]
    },
    {
      "k": "key",
      "key": "Control+A",
      "n": 1
    },
    {
      "k": "dragMove",
      "at": 0.73,
      "length": 1,
      "x": 0.162,
      "y": 0.775
    },
    {
      "k": "dragMove",
      "at": 0.186,
      "length": 1,
      "x": 0.778,
      "y": 0.369
    },
    {
      "k": "jump",
      "at": 0.053,
      "length": 4
    },
    {
      "k": "suggest",
      "open": false,
      "pick": 0
    },
    {
      "k": "dragMove",
      "at": 0.133,
      "length": 40,
      "x": 0.885,
      "y": 0.303
    },
    {
      "k": "type",
      "text": "é"
    },
    {
      "k": "key",
      "key": "PageUp",
      "n": 1
    },
    {
      "k": "suggest",
      "open": true,
      "pick": 0
    },
    {
      "k": "pause"
    }
  ], "host");
});
