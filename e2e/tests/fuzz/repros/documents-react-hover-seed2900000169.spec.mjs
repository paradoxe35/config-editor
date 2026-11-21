import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 2900000169 on documents-react.
// Failed the `hover` check after step 15: the card shows "public_address"
// with the pointer on 3:10 ("/insecure_plaintext"). The card covering line 3
// had taken the pointer; a headless browser left `:hover` on the text.
test('documents-react: hover (seed 2900000169)', async ({ browser }) => {
  await replay(
    browser,
    'documents-react',
    '',
    [
      // The caret at the text's end, as the CI page had it: the
      // suggestion makes a third line, under the card of the second.
      {
        k: 'key',
        key: 'Control+End',
        n: 1,
      },
      {
        k: 'suggestMouse',
        outside: false,
        pick: 2,
      },
      {
        k: 'jump',
        at: 0.884,
        length: 0,
      },
      {
        k: 'hover',
        at: 0.664,
        column: 0.243,
        into: false,
        escape: false,
      },
      {
        k: 'wheel',
        dy: 600,
      },
      {
        k: 'hover',
        at: 0.871,
        column: 0.283,
        into: false,
        escape: false,
      },
    ],
    'hover',
  );
});
