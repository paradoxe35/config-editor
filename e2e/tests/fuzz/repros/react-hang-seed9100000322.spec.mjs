import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 9100000322 on react, vue and svelte.
// Failed the `hang` check after step 2: step 2 (prop) took over 8000 ms.
// The hover card opened above its line, over the page's header, and
// covered `#rename` (with CI's fonts: its text wraps to one more line);
// the pointer click on the button waited for the card, which stays while
// the pointer rests on its key. Each step is timed (8 s).
for (const name of ['react', 'vue', 'svelte']) {
  test(`${name}: hang (seed 9100000322)`, async ({ browser }) => {
    await replay(
      browser,
      name,
      '',
      [
        { k: 'dragMove', at: 0.228, length: 3, x: 0.415, y: 0.691 },
        { k: 'hover', at: 0.259, column: 0.418, into: false, escape: false },
        { k: 'prop', prop: 'rename' },
      ],
      'hang',
    );
  });
}

// The same with any fonts: a key on the first line puts its card above it,
// over the header, where `#rename` is. (`#read-only` is clicked the same
// way; on these pages its middle is a few pixels above such a card.)
for (const name of ['react', 'vue', 'svelte']) {
  test(`${name}: a hover card over the header's rename button`, async ({ browser }) => {
    await replay(
      browser,
      name,
      '',
      [
        { k: 'key', key: 'Control+Home', n: 1 },
        { k: 'type', text: 'listen: 1\n' },
        { k: 'hover', at: 0, column: 0.1, into: false, escape: false },
        { k: 'prop', prop: 'rename' },
      ],
      'hang',
    );
  });
}
