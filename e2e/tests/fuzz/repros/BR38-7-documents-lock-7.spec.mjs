import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz on documents.
// Fails the `lock` check after step 5: locked "insecure_plaintext: \n" gone from "listen: 7835\ninsecurQQe_plaintext: \nlisten: 7835\npublic_address: tunnel.example.com\n"
test('BR38-7 documents: lock', async ({ browser }) => {
  await replay(
    browser,
    'documents',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.542,
          length: 3,
          line: true,
        },
      },
      {
        k: 'key',
        key: 'Shift+Alt+ArrowDown',
        n: 1,
      },
      {
        k: 'suggest',
        open: true,
        pick: 2,
      },
      {
        k: 'lock',
        range: {
          at: 0.22,
          length: 3,
          line: true,
        },
      },
      {
        k: 'jump',
        at: 0.25,
        length: 0,
      },
      {
        k: 'type',
        text: 'QQ',
      },
    ],
    'lock',
  );
});
