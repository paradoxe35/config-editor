import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz on vanilla.
// Fails the `lock` check after step 2: locked "el server configuration\nlisten" gone from "# Tunnel server configuration\n\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n"
test('vanilla: lock', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.06,
          length: 30,
          line: false,
        },
      },
      {
        k: 'jump',
        at: 0.289,
        length: 0,
      },
      {
        k: 'key',
        key: 'Enter',
        n: 1,
      },
    ],
    'lock',
  );
});
