import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 940000035 on documents-react.
// Failed the `undo-silent` check: undo all stopped at "日本listen: 7835\nxpublic_address: …" with no notice.
test('documents-react: undo all after a drag, Edit as XML, typing, Edit as HCL and a range lock gives the opening text or says why (seed 940000035)', async ({
  browser,
}) => {
  await replay(
    browser,
    'documents-react',
    '',
    [
      { k: 'dragMove', at: 0.905, length: 14, x: 0.255, y: 0.387 },
      { k: 'editAs', language: 'xml' },
      { k: 'type', text: 'x' },
      { k: 'editAs', language: 'hcl' },
      { k: 'lock', range: { at: 0.79, length: 3, line: false } },
    ],
    'undo-silent',
  );
});
