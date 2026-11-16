import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 91000087 on react (svelte and vue too).
// Failed the `hover` check: the card showed "public_address" with the pointer on 2:3 ("/listen"),
// where the card (above line 3) covers line 2 and the pointer went from the key into it.
test('react: a pointer moving from a key into its card over the line above keeps the card (seed 91000087)', async ({ browser }) => {
  await replay(
    browser,
    'react',
    '',
    [
      { k: 'hover', at: 0.719, column: 0.227, into: false, escape: false },
      { k: 'hover', at: 0.516, column: 0.184, into: false, escape: false },
    ],
    'hover',
  );
});
