import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 58353 on svelte.
// Fails the `lock` check after step 2: locked "l server configuration\nlisten:" gone from "# Tunnel server configuration\npublic_address: tunnel.example.com\nlisten: 7835\n"
test('svelte: lock (seed 58353)', async ({ browser }) => {
  await replay(
    browser,
    'svelte',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.097,
          length: 30,
          line: false,
        },
      },
      {
        k: 'jump',
        at: 0.616,
        length: 4,
      },
      {
        k: 'key',
        key: 'Alt+ArrowUp',
        n: 1,
      },
    ],
    'lock',
  );
});
