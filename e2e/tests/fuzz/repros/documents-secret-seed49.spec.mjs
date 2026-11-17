import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 49 on documents.
// Fails the `secret` check after step 1: "«s«secret 3f2a»" in "[clients.acme]\nsec«secret 3f2a»\"\nret = \"«s«secret 3f2a»ecret 3f2a»\"\n"
test('documents: secret (seed 49)', async ({ browser }) => {
  await replay(
    browser,
    'documents',
    '',
    [
      {
        k: 'crossCopy',
        from: 1,
        to: 1,
        load: true,
        token: false,
        at: 0.65,
        length: 14,
        at2: 0.458,
        over: 0,
      },
      {
        k: 'crossCopy',
        from: 0,
        to: 1,
        load: true,
        token: true,
        at: 0.811,
        length: 1,
        at2: 0.772,
        over: 0,
      },
    ],
    'secret',
  );
});
