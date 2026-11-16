import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz on react.
// Fails the `lock-marks` check after step 3: line 1: locked "835", marked ""
test('react: lock-marks', async ({ browser }) => {
  await replay(
    browser,
    'react',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.5,
          length: 8,
          line: false,
        },
      },
      {
        k: 'key',
        key: 'Control+Home',
        n: 1,
      },
      {
        k: 'key',
        key: 'Control+Shift+K',
        n: 1,
      },
      {
        k: 'prop',
        prop: 'rename',
      },
    ],
    'lock-marks',
  );
});
