import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 69 on documents.
// Fails the `drag-move` check after step 1: dragging "f2a»\"\n" made "f2a»\"\n[clients.acme]\nsec«secret 3f2a»\"\n= \"" of "[clients.acme]\nsec«secret 3f2a»\"\n= \"«secret 3f2a»\"\n"
test('documents: drag-move (seed 69)', async ({ browser }) => {
  await replay(
    browser,
    'documents',
    '',
    [
      {
        k: 'crossCopy',
        from: 1,
        to: 1,
        load: true,
        token: false,
        at: 0.946,
        length: 14,
        at2: 0.454,
        over: 4,
      },
      {
        k: 'dragMove',
        at: 0.893,
        length: 40,
        x: 0.063,
        y: 0.181,
      },
    ],
    'drag-move',
  );
});
