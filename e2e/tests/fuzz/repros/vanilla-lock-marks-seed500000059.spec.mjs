import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 500000059 on vanilla: text moved by
// a drag, then `/public_address` locked by path: its value is marked.
test('vanilla: lock-marks (seed 500000059)', async ({ browser }) => {
  await replay(browser, 'vanilla', '', [
    { k: 'dragMove', at: 0.187, length: 40, x: 0.424, y: 0.658 },
    { k: 'lockPath', on: true },
  ], 'lock-marks');
});
