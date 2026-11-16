import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 100000073 on documents.
// Failed the `undo-silent` check after step 19: undo all stopped at "keylisten: 7835\npublic_address: tunnel.example.com\n" (opened with "listen: 7835\npublic_address: tunnel.example.com\n") with no notice
test('documents: undo all after typing beside a range lock and Edit as TOML gives the opening text (seed 100000073)', async ({
  browser,
}) => {
  await replay(
    browser,
    'documents',
    '',
    [
      { k: 'lock', range: { at: 0.048, length: 5, line: false } },
      { k: 'type', text: 'k' },
      { k: 'jump', at: 0.388, length: 4 },
      { k: 'type', text: 't' },
      { k: 'editAs', language: 'toml' },
    ],
    'undo-silent',
  );
});
