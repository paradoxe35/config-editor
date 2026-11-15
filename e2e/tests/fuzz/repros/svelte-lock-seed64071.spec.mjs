import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 64071 on svelte.
// Fails the `lock` check after step 3: locked "gurat" gone from ""
test('svelte: lock (seed 64071)', async ({ browser }) => {
  await replay(
    browser,
    'svelte',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.281,
          length: 5,
          line: false,
        },
      },
      {
        k: 'editAs',
        language: 'xml',
      },
      {
        k: 'key',
        key: 'Control+A',
        n: 1,
      },
      {
        k: 'clip',
        key: 'Control+V',
      },
    ],
    'lock',
  );
});
