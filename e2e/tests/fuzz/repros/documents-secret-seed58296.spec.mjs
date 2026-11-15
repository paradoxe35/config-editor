import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 58296 on documents.
// Fails the `secret` check after step 1: "«ret 3f2a»" in "[limits]\nmax_pending = «ret 3f2a»512\nsec"
test('documents: secret (seed 58296)', async ({ browser }) => {
  await replay(
    browser,
    'documents',
    '',
    [
      {
        k: 'crossCopy',
        from: 0,
        to: 1,
        load: true,
        token: true,
        at: 0.591,
        length: 14,
        at2: 0.87,
        over: 0,
      },
      {
        k: 'dragMove',
        at: 0.624,
        length: 3,
        x: 0.416,
        y: 0.538,
      },
    ],
    'secret',
  );
});
