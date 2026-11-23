import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 7300000410 on vanilla.
// Fails the `undo-silent` check after step 3: undo all stops at "host_key: 1\n# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n" (opened with "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n") with no notice
test("vanilla: undo-silent (seed 7300000410)", async ({ browser }) => {
  await replay(browser, "vanilla", "", [
    {
      "k": "lock",
      "range": {
        "at": 0.613,
        "length": 8,
        "line": false
      }
    },
    {
      "k": "dragMove",
      "at": 0.297,
      "length": 14,
      "x": 0.553,
      "y": 0.951
    },
    {
      "k": "lock",
      "range": {
        "at": 0.042,
        "length": 30,
        "line": true
      }
    },
    {
      "k": "hostChange",
      "at": 0.079,
      "base": true,
      "line": "host_key: 1"
    }
  ], "undo-silent");
});
