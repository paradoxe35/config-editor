import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz on vanilla.
// Fails the `lock` check after step 3: locked "public_address: tunnel.example.com\ninsecure_plaintext: false\n" gone from "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false"
test('BR38-3 vanilla: lock', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      {
        k: 'click',
        x: 0.612,
        y: 0.676,
      },
      {
        k: 'type',
        text: 'v',
      },
      {
        k: 'lock',
        range: {
          at: 0.696,
          length: 5,
          line: true,
        },
      },
      {
        k: 'clip',
        key: 'Control+X',
      },
    ],
    'lock',
  );
});
