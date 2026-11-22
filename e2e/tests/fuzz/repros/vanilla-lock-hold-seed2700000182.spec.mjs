import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 2700000182 on vanilla: a host
// change, a line copied and pasted, Ctrl+], Ctrl+Shift+Enter, then the host
// names /public_address read-only.
test('vanilla: lock-hold (seed 2700000182)', async ({ browser }) => {
  await replay(browser, 'vanilla', '?random', [
    { k: 'hostChange', at: 0.281, base: false, line: 'host_key: 1' },
    { k: 'click', x: 0.226, y: 0.351 },
    { k: 'clip', key: 'Control+C' },
    { k: 'clip', key: 'Control+V' },
    { k: 'key', key: 'Control+]', n: 1 },
    { k: 'key', key: 'Control+Shift+Enter', n: 1 },
    { k: 'lockPath', on: true },
  ], 'lock-hold');
});
