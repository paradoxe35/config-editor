import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 4700000248 on documents-react.
// Fails the `secret-dom` check after step 18: the page shows "«secret: this placeholder stands for the secret of /clients/a"
test('documents-react: secret-dom (seed 4700000248)', async ({ browser }) => {
  await replay(
    browser,
    'documents-react',
    '',
    [
      {
        k: 'editAs',
        language: 'xml',
      },
      {
        k: 'crossCopy',
        from: 0,
        to: 0,
        load: true,
        token: true,
        at: 0.382,
        length: 5,
        at2: 0.347,
        over: 1,
      },
      {
        k: 'editAs',
        language: 'xml',
      },
      {
        k: 'key',
        key: 'ArrowLeft',
        n: 1,
      },
      {
        k: 'tab',
        index: 0,
      },
    ],
    'secret-dom',
  );
});
