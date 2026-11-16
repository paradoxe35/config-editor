import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 500000078 on react, svelte and vue:
// a line copied, a range locked, then the host's own text set: the lock
// is marked where it is in that text.
for (const page of ['react', 'svelte', 'vue']) {
  test(`${page}: lock-marks (seed 500000078)`, async ({ browser }) => {
    await replay(browser, page, '', [
      { k: 'key', key: 'Shift+Alt+ArrowDown', n: 1 },
      { k: 'lock', range: { at: 0.756, length: 5, line: false } },
      {
        k: 'hostValue',
        text: '# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n',
        how: 'setValue',
      },
    ], 'lock-marks');
  });
}
