import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// P1: a change held back in the host language (a locked field reads otherwise) is not the host text; a switch to a view waits, and the host value stays the last one it heard.
// Shrunk by the browser fuzz from seed 11000063 on vue (?random).
// Fails the `value-text` check after step 5: value "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n" text "# Tunnel server configuan: 7835\npublic_address: tunnel.example.com\n"
test("vue: value-text (seed 11000063)", async ({ browser }) => {
  await replay(browser, "vue", "?random", [
    {
      "k": "lock",
      "range": {
        "at": 0.531,
        "length": 5,
        "line": false
      }
    },
    {
      "k": "jump",
      "at": 0.3,
      "length": 12
    },
    {
      "k": "type",
      "text": "a"
    },
    {
      "k": "editAs",
      "language": "json"
    },
    {
      "k": "editAs",
      "language": "hcl"
    },
    {
      "k": "editAs",
      "language": ""
    }
  ], "value-text");
});
