import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 11 on documents.
// Fails the `secret-cross` check after step 0: main: line 2 "public_address: tunnel.exam«secret 9b1c»ple.com" holds another document's «secret 9b1c», and no problem is listed there
test.fixme('GP38-4 documents: secret-cross (seed 11)', async ({ browser }) => {
  await replay(
    browser,
    'documents',
    '',
    [
      {
        k: 'crossCopy',
        from: 1,
        to: 0,
        load: true,
        token: true,
        at: 0.45,
        length: 5,
        at2: 0.844,
        over: 0,
      },
    ],
    'secret-cross',
  );
});
