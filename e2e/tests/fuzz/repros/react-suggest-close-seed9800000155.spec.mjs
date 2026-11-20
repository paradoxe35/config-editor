import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 9800000155 on react.
// Fails the `suggest-close` check after step 16: a click outside the editor left the suggestions open
test('react: suggest-close (seed 9800000155)', async ({ browser }) => {
  await replay(
    browser,
    'react',
    '',
    [
      {
        k: 'resize',
        width: 420,
        height: 300,
      },
      {
        k: 'wheel',
        dy: 3000,
      },
      {
        k: 'suggestMouse',
        outside: true,
        pick: 1,
      },
    ],
    'suggest-close',
  );
});
