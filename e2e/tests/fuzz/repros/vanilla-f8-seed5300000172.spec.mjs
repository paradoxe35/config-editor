import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 5300000172 on vanilla.
// Fails the `f8` check after step 11: after Shift+F8 1: caret at 95:2, problems at [[9,1,null,2]]
test('vanilla: f8 (seed 5300000172)', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '?revisions',
    [
      {
        k: 'editAs',
        language: 'xml',
      },
      {
        k: 'hostChange',
        at: 0.796,
        base: false,
        line: 'host_key: 1',
      },
      {
        k: 'lateDiagnostics',
        at: 0.402,
        by: 'text',
        wait: 200,
      },
      {
        k: 'f8',
        shift: true,
        n: 2,
      },
    ],
    'f8',
  );
});
