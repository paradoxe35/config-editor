import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// P1: another document's placeholder pasted into a document whose host gave
// none of its own: a Replace whose match is inside it leaves it whole.
// Shrunk by the browser fuzz from seed 62000036 on documents-react.
test('documents-react: secret (seed 62000036)', async ({ browser }) => {
  await replay(browser, 'documents-react', '', [
    { k: 'crossCopy', from: 0, to: 0, load: true, token: true, at: 0.68, length: 5, at2: 0.052, over: 0 },
    { k: 'crossCopy', from: 0, to: 1, load: false, token: true, at: 0.735, length: 5, at2: 0.492, over: 1 },
    { k: 'find', query: 'e', replace: 'Z', all: false },
  ], 'secret');
});
