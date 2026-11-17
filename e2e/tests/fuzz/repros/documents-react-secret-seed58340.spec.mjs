import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 58340 on documents-react.
// Fails the `secret` check after step 1: "«secret 3f2aten: 7835" in "lis«secret 3f2aten: 7835\npublic_address: tunnel.example.com\n"
test('documents-react: secret (seed 58340)', async ({ browser }) => {
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
        at: 0.144,
        length: 14,
        at2: 0.082,
        over: 0,
      },
      {
        k: 'key',
        key: 'Backspace',
        n: 1,
      },
    ],
    'secret',
  );
});
