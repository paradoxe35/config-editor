import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 9400000096 on vanilla: edited as
// XML, a span locked, Backspace, a suggestion picked at the top, Enter.
test('vanilla: lock (seed 9400000096)', async ({ browser }) => {
  await replay(browser, 'vanilla', '', [
    { k: 'jump', at: 0.955, length: 4 },
    { k: 'editAs', language: 'xml' },
    { k: 'lock', range: { at: 0.226, length: 30, line: false } },
    { k: 'key', key: 'Backspace', n: 1 },
    { k: 'key', key: 'Control+Home', n: 1 },
    { k: 'suggest', open: true, pick: 1 },
    { k: 'key', key: 'Enter', n: 1 },
  ], 'lock');
});
