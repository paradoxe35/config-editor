import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 2000000212 on svelte.
// Failed the `lock-marks` check after step 2: line 3: locked "tunnel.exampl.com", marked ""
test('svelte: lock-marks (seed 2000000212)', async ({ browser }) => {
  await replay(
    browser,
    'svelte',
    '',
    [
      { k: 'drag', x1: 0.2, y1: 0.279, x2: 0.798, y2: 0.617 },
      { k: 'key', key: 'Backspace', n: 1 },
      { k: 'lockPath', on: true },
    ],
    'lock-marks',
  );
});
