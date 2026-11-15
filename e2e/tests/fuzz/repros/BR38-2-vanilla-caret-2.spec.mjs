import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz on vanilla.
// Fails the `caret` check after step 1: caret on line 1, active []
test.fixme('BR38-2 vanilla: caret', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      {
        k: 'type',
        text: '👍🏽👨‍👩‍👧\n',
      },
      {
        k: 'hostValue',
        text: '',
        how: 'prop',
      },
    ],
    'caret',
  );
});
