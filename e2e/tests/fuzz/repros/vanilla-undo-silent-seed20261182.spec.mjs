import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 20261182 on vanilla.
// Failed the `undo-silent` check: undo all stopped short of the opening text with no notice.
test('vanilla: undo all after Edit as JSON, typing, Edit as XML and a range lock gives the opening text or says why (seed 20261182)', async ({
  browser,
}) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      { k: 'click', x: 0.012, y: 0.267 },
      { k: 'editAs', language: 'json' },
      { k: 'type', text: 'p' },
      { k: 'editAs', language: 'xml' },
      { k: 'lock', range: { at: 0.672, length: 30, line: false } },
    ],
    'undo-silent',
  );
});
