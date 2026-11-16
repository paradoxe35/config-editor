import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// P1: another document's placeholder pasted into a document whose host gave
// none of its own, then Backspace at its end: the copy goes whole, no
// broken token reaches the host.
// Shrunk by the browser fuzz from seed 500000035 on documents-react.
test('documents-react: secret (seed 500000035)', async ({ browser }) => {
  await replay(browser, 'documents-react', '', [
    { k: 'crossCopy', from: 1, to: 0, load: true, token: true, at: 0.233, length: 14, at2: 0.099, over: 4 },
    { k: 'key', key: 'Backspace', n: 1 },
  ], 'secret');
});
