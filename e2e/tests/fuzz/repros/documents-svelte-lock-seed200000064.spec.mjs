import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// P1: a host range holding a line break (`\npubl`) keeps its text through an
// Edit-as XML view where Ctrl+/ comments out the line above it.
// Shrunk by the browser fuzz from seed 200000064 on documents-svelte.
test('documents-svelte: lock (seed 200000064)', async ({ browser }) => {
  await replay(browser, 'documents-svelte', '', [
    { k: 'lock', range: { at: 0.263, length: 5, line: false } },
    { k: 'editAs', language: 'xml' },
    { k: 'key', key: 'Control+/', n: 1 },
  ], 'lock');
});
