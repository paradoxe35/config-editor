import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 31 on vanilla.
// Fails the `value-text` check after step 3: value "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n" text "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\nΩnsecure_plaintext: false\n"
test.fixme('GP38-5 vanilla: value-text (seed 31)', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.927,
          length: 3,
          line: false,
        },
      },
      {
        k: 'dragMove',
        at: 0.75,
        length: 1,
        x: 0.417,
        y: 0.346,
      },
      {
        k: 'type',
        text: 'Ω',
      },
      {
        k: 'unlock',
      },
    ],
    'value-text',
  );
});
