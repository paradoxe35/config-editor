import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// A lock the host gives after an edit is marked where it is: its marks are
// not moved by the edit before it a second time.
// Shrunk by the browser fuzz from seed 900000023 on documents-react.
test('documents-react: lock-marks (seed 900000023)', async ({ browser }) => {
  await replay(browser, 'documents-react', '', [
    { k: 'lock', range: { at: 0.041, length: 14, line: false } },
    { k: 'type', text: 'por' },
    { k: 'lock', range: { at: 0.124, length: 3, line: false } },
  ], 'lock-marks');
});
