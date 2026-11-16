import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 500000059 on documents (and its
// React, Svelte and Vue pages): a whole-line lock given after a list item
// typed above it is marked on the line it locks.
for (const page of ['documents', 'documents-react', 'documents-svelte', 'documents-vue']) {
  test(`${page}: lock-marks (seed 500000059)`, async ({ browser }) => {
    await replay(browser, page, '', [
      { k: 'lock', range: { at: 0.079, length: 8, line: false } },
      { k: 'type', text: '- item\n' },
      { k: 'lock', range: { at: 0.151, length: 14, line: true } },
    ], 'lock-marks');
  });
}
