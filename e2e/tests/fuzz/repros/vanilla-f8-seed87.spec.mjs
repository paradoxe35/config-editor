import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 87 on vanilla.
// Fails the `f8` check after step 6: after F8 1: caret at 4:1, problems at [[5,1,null,3],[5,1,null,2]]
test('vanilla: f8 (seed 87)', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      {
        k: 'clip',
        key: 'Control+X',
      },
      {
        k: 'click',
        x: 0.312,
        y: 0.425,
      },
      {
        k: 'type',
        text: "'",
      },
      {
        k: 'editAs',
        language: 'hcl',
      },
      {
        k: 'key',
        key: 'Control+Shift+Enter',
        n: 1,
      },
      {
        k: 'goto',
        line: 2,
        column: 6,
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
