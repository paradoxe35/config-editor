import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 930000037 on react, svelte and vue:
// text dragged, typed, then `/public_address` locked by path.
// Open (C28, harness side): the text then does not parse
// (`public_address: tunnel.example.com` then ` name:`). The fuzz takes the
// field's text from `rangeOf`, which reads it off the lines (through the
// `:`); the editor locks it as the stand-in reads it (the text with its
// breaking `:` blanked), so the `:` stays free for the repair. Neither
// says what a value of a text that does not parse is.
for (const page of ['react', 'svelte', 'vue']) {
  test.fixme(`${page}: lock-marks (seed 930000037)`, async ({ browser }) => {
    await replay(browser, page, '', [
      { k: 'dragMove', at: 0.32, length: 14, x: 0.854, y: 0.543 },
      { k: 'type', text: ' ' },
      { k: 'type', text: 'name:' },
      { k: 'lockPath', on: true },
    ], 'lock-marks');
  });
}
