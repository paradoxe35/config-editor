import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 2300000348 on documents.
// Fails the `lock` check after step 3: locked "日本listen: 7835\npublic_address: tunnel.example.com\n" gone from "listen: 7835\npublic_address: tunnel.example.com\n"
test('documents: lock (seed 2300000348)', async ({ browser }) => {
  await replay(
    browser,
    'documents',
    '',
    [
      { k: 'editAs', language: 'hcl' },
      { k: 'type', text: '日' },
      { k: 'lock', range: { at: 0.24, length: 30, line: true } },
      { k: 'key', key: 'Control+Z', n: 1 },
    ],
    'lock',
  );
});
