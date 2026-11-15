import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 14 on documents-react.
// Fails the `undo-silent` check after step 2: undo all stops at "public_address: tunnel.example.com\n" (opened with "listen: 7835\npublic_address: tunnel.example.com\n") with no notice
test('documents-react: undo-silent (seed 14)', async ({ browser }) => {
  await replay(
    browser,
    'documents-react',
    '',
    [
      {
        k: 'lock',
        range: {
          at: 0.408,
          length: 5,
          line: false,
        },
      },
      {
        k: 'clip',
        key: 'Control+X',
      },
      {
        k: 'type',
        text: 'Ω',
      },
    ],
    'undo-silent',
  );
});
