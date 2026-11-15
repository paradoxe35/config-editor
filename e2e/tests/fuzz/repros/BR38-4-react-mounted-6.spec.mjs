import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz on react (?random).
// Fails the `mounted` check after step 3: no editor on screen
test('BR38-4 react: mounted', async ({ browser }) => {
  await replay(
    browser,
    'react',
    '?random',
    [
      {
        k: 'lock',
        range: {
          at: 0.209,
          length: 3,
          line: true,
        },
      },
      {
        k: 'drag',
        x1: 0.643,
        y1: 0.071,
        x2: 0.304,
        y2: 0.507,
      },
      {
        k: 'key',
        key: 'Delete',
        n: 1,
      },
      {
        k: 'prop',
        prop: 'rename',
      },
    ],
    'mounted',
  );
});
