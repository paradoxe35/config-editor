import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 25 on documents-react.
// Fails the `undo-silent` check after step 3: undo all stops at "listen: 7835\n\npublic_address: tunnel.example.com\n" (opened with "listen: 7835\npublic_address: tunnel.example.com\n") with no notice
test.fixme('GP38-7 documents-react: undo-silent (seed 25)', async ({ browser }) => {
  await replay(
    browser,
    'documents-react',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.891,
          length: 14,
          line: false,
        },
      },
      {
        k: 'jump',
        at: 0.284,
        length: 0,
      },
      {
        k: 'key',
        key: 'Enter',
        n: 1,
      },
      {
        k: 'type',
        text: 't',
      },
    ],
    'undo-silent',
  );
});
