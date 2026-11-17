import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 930000037 on react, svelte and vue:
// text dragged, typed, then `/public_address` locked by path while the
// text does not parse (`public_address: tunnel.example.com` then ` name:`).
// The editor locks the field as the text reads with its breaking `:`
// blanked, so the `:` stays free for the repair; the fuzz took the locked
// text from `rangeOf`, through the `:`. It now takes a lock named in such
// a text from the editor's marks (`markedLock` in `perform.mjs`).
for (const page of ['react', 'svelte', 'vue']) {
  test(`${page}: lock-marks (seed 930000037)`, async ({ browser }) => {
    await replay(browser, page, '', [
      { k: 'dragMove', at: 0.32, length: 14, x: 0.854, y: 0.543 },
      { k: 'type', text: ' ' },
      { k: 'type', text: 'name:' },
      { k: 'lockPath', on: true },
    ], 'lock-marks');
  });
}
