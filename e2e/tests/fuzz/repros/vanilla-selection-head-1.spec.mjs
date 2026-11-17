import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz on vanilla.
// Fails the `selection-head` check after step 1: caret on line 3, active [1]
test('vanilla: selection-head', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      {
        k: 'key',
        key: 'Control+Home',
        n: 1,
      },
      {
        k: 'key',
        key: 'Shift+ArrowDown',
        n: 2,
      },
    ],
    'selection-head',
  );
});
