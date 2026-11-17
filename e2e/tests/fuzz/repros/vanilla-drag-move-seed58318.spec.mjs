import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 58318 on vanilla.
// A switch waiting for the document (Edit as JSON) is made on the typing
// pause just before a drag: the drag is judged on the text it began in.
test('vanilla: drag-move (seed 58318)', async ({ browser }) => {
  await replay(
    browser,
    'vanilla',
    '',
    [
      {
        k: 'click',
        x: 0.208,
        y: 0.884,
      },
      {
        k: 'type',
        text: '😀',
      },
      {
        k: 'editAs',
        language: 'json',
      },
      {
        k: 'f8',
        shift: true,
        n: 1,
      },
      {
        k: 'type',
        text: ' ß',
      },
      {
        k: 'suggest',
        open: true,
        pick: 2,
      },
      {
        k: 'click',
        x: 0.082,
        y: 0.64,
      },
      {
        k: 'dragMove',
        at: 0.781,
        length: 6,
        x: 0.72,
        y: 0.91,
      },
    ],
    'drag-move',
  );
});
