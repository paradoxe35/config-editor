import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 8800000288 on vanilla.
// Fails the `f8` check after step 4: after F8 1: caret at 2:9, problems at [[1,1,null,6]]
test('vanilla: f8 (seed 8800000288)', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      {
        k: 'lateDiagnostics',
        at: 0.819,
        by: 'revision',
        wait: 200,
      },
      {
        k: 'key',
        key: 'Control+A',
        n: 1,
      },
      {
        k: 'type',
        text: '#',
      },
      {
        k: 'type',
        text: 'é😀\n4',
      },
      {
        k: 'f8',
        shift: false,
        n: 1,
      },
    ],
    'f8',
  );
});
