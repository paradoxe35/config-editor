import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 1700000060 on the documents pages:
// another document's placeholder pasted, then a second one pasted over
// part of the first: no piece of a placeholder reaches the host.
for (const page of ['documents', 'documents-react', 'documents-svelte', 'documents-vue']) {
  test(`${page}: secret (seed 1700000060)`, async ({ browser }) => {
    await replay(browser, page, '', [
      { k: 'crossCopy', from: 1, to: 0, load: true, token: true, at: 0.133, length: 1, at2: 0.589, over: 0 },
      { k: 'crossCopy', from: 1, to: 0, load: true, token: true, at: 0.587, length: 5, at2: 0.491, over: 4 },
    ], 'secret');
  });
}
