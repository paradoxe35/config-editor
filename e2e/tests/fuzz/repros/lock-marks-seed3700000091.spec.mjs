import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 3700000091 on vanilla, react, svelte
// and vue: a span locked, a word typed, text dragged, a second span locked.
const actions = [
  { k: 'lock', range: { at: 0.306, length: 8, line: false } },
  { k: 'type', text: 'listen' },
  { k: 'dragMove', at: 0.124, length: 14, x: 0.3, y: 0.723 },
  { k: 'lock', range: { at: 0.171, length: 5, line: false } },
];
for (const [page, query] of [['vanilla', ''], ['react', '?random'], ['svelte', '?random'], ['vue', '?random']]) {
  test(`${page}: lock-marks (seed 3700000091)`, async ({ browser }) => {
    await replay(browser, page, query, actions, 'lock-marks');
  });
}
