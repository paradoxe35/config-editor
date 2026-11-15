import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 15 on documents-react.
// Fails the `secret-cross` check after step 0: main: line 2 "public_address: tunnel.«secret 9b1c»example.com" holds another document's «secret 9b1c», and no problem is listed there
test.fixme('GP38-4 documents-react: secret-cross (seed 15)', async ({ browser }) => {
  await replay(
    browser,
    'documents-react',
    '',
    [
      {
        k: 'crossCopy',
        from: 1,
        to: 0,
        load: true,
        token: true,
        at: 0.404,
        length: 1,
        at2: 0.754,
        over: 0,
      },
    ],
    'secret-cross',
  );
});
