import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 45 on documents-react.
// Fails the `f8` check after step 3: after F8 1: caret at 3:1, problems at [[1,1,null,13],[2,1,null,9]]
test('GP38-6 documents-react: f8 (seed 45)', async ({ browser }) => {
  await replay(
    browser,
    'documents-react',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.57,
          length: 30,
          line: false,
        },
      },
      {
        k: 'dragMove',
        at: 0.375,
        length: 6,
        x: 0.482,
        y: 0.029,
      },
      {
        k: 'click',
        x: 0.972,
        y: 0.629,
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
