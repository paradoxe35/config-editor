import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 9800000116 on documents.
// Fails the `drag-move` check after step 2: dragging "/ac" made "<clients>\n  <acme>\n    <secret>/ac</sec«secret 3f2a»ret>\n  <me>\n</clients>\n" of "<clients>\n  <acme>\n    <secret>«secret 3f2a»</sec«secret 3f2a»ret>\n  </acme>\n</clients>\n"
test('documents: drag-move (seed 9800000116)', async ({ browser }) => {
  await replay(
    browser,
    'documents',
    '',
    [
      { k: 'editAs', language: 'xml' },
      { k: 'crossCopy', from: 0, to: 0, load: true, token: true, at: 0.027, length: 14, at2: 0.663, over: 0 },
      { k: 'dragMove', at: 0.805, length: 3, x: 0.158, y: 0.312 },
    ],
    'drag-move',
  );
});
