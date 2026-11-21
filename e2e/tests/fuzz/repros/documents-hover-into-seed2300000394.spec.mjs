import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 2300000394 on documents.
// Fails the `hover-into` check after step 10: the card for "«secret 3f2a» acme" closed as the pointer moved into it
test('documents: hover-into (seed 2300000394)', async ({ browser }) => {
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
        at: 0.659,
        length: 1,
        at2: 0.236,
        over: 0,
      },
      {
        k: 'hover',
        at: 0.522,
        column: 0.124,
        into: true,
        escape: false,
      },
    ],
    'hover-into',
  );
});
