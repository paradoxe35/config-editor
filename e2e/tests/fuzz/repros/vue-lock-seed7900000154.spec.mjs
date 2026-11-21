import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 7900000154 on vue (?random).
// Failed the `lock` check (a reset set two ways) after step 13: locked "한" gone from "  "
test("vue: lock (seed 7900000154)", async ({ browser }) => {
  await replay(browser, "vue", "?random", [
    {
      "k": "hover",
      "at": 0.334,
      "column": 0.148,
      "into": true,
      "escape": true
    },
    {
      "k": "suggest",
      "open": true,
      "pick": 0
    },
    {
      "k": "type",
      "text": "-"
    },
    {
      "k": "jump",
      "at": 0.853,
      "length": 1
    },
    {
      "k": "suggest",
      "open": true,
      "pick": 0
    },
    {
      "k": "unlock"
    },
    {
      "k": "type",
      "text": "true"
    },
    {
      "k": "hostValue",
      "text": "",
      "how": "setValue"
    },
    {
      "k": "ime",
      "steps": [
        "ㅎ",
        "하",
        "한"
      ],
      "commit": "한"
    },
    {
      "k": "click",
      "x": 0.882,
      "y": 0.124
    },
    {
      "k": "key",
      "key": "Shift+Home",
      "n": 4
    },
    {
      "k": "type",
      "text": "  "
    },
    {
      "k": "lock",
      "range": {
        "at": 0.155,
        "length": 14,
        "line": true
      }
    },
    {
      "k": "type",
      "text": "|"
    }
  ], "lock");
});
