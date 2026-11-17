import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 64018 on documents.
// Fails the `copy` check after step 2: copied "r", selection "«secret 3f2a»"
test('documents: copy (seed 64018)', async ({ browser }) => {
  await replay(
    browser,
    'documents',
    '',
    [
      {
        k: 'crossCopy',
        from: 0,
        to: 0,
        load: true,
        token: true,
        at: 0.825,
        length: 5,
        at2: 0.62,
        over: 0,
      },
      {
        k: 'key',
        key: 'Shift+ArrowLeft',
        n: 1,
      },
      {
        k: 'crossCopy',
        from: 0,
        to: 0,
        load: false,
        token: false,
        at: 0.546,
        length: 1,
        at2: 0.019,
        over: 0,
      },
    ],
    'copy',
  );
});
