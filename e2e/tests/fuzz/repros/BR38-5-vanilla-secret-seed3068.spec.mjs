import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 3068 on vanilla.
// Fails the `secret` check after step 8: "«sec#3f2a»" in "  clients:\n  acme:\n    secret: «sec#3f2a»\n"
test.fixme('BR38-5 vanilla: secret (seed 3068)', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      {
        k: 'hostValue',
        text: 'listen: 9000\n',
        how: 'setValue',
      },
      {
        k: 'clip',
        key: 'Control+X',
      },
      {
        k: 'hostValue',
        text: 'clients:\n  acme:\n    secret: «secret 3f2a»\n',
        how: 'setValue',
      },
      {
        k: 'key',
        key: 'Control+]',
        n: 1,
      },
      {
        k: 'wheel',
        dy: -120,
      },
      {
        k: 'goto',
        line: 21,
        column: null,
      },
      {
        k: 'key',
        key: 'Control+Shift+Z',
        n: 1,
      },
      {
        k: 'jump',
        at: 0.788,
        length: 4,
      },
      {
        k: 'type',
        text: '#',
      },
    ],
    'secret',
  );
});
