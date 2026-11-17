import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// A lock the host gives while an edit has not reached it (held, the
// renamed key reading a range-covered field otherwise) names places in
// the host's text: it is marked on the text it names there.
// Shrunk by the browser fuzz from seed 900000023 on documents-react.
test('documents-react: lock-marks (seed 900000023)', async ({ browser }) => {
  await replay(browser, 'documents-react', '', [
    { k: 'lock', range: { at: 0.041, length: 14, line: false } },
    { k: 'type', text: 'por' },
    { k: 'lock', range: { at: 0.124, length: 3, line: false } },
  ], 'lock-marks');
});
