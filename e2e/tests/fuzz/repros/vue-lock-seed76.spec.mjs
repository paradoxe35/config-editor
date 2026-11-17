import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 76 on vue.
// Fails the `lock` check after step 5: locked "public_address: tunnel.example.com\n" gone from "# Tunnel server configuration\nlisten: 7835\ninsecure_plaintext: \npublic_address: tunnel.example.com "
test('vue: lock (seed 76)', async ({ browser }) => {
  await replay(
    browser,
    'vue',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.698,
          length: 3,
          line: true,
        },
      },
      {
        k: 'click',
        x: 0.324,
        y: 0.594,
      },
      {
        k: 'suggest',
        open: true,
        pick: 2,
      },
      {
        k: 'key',
        key: 'Alt+ArrowUp',
        n: 1,
      },
      {
        k: 'key',
        key: 'Control+End',
        n: 1,
      },
      {
        k: 'type',
        text: '[1, ',
      },
    ],
    'lock',
  );
});
