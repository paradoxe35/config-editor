import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 4500000250 on every documents page
// (?revisions). Failed the `caret` check after step 1: caret on line 1,
// active [3]. The click lands on the left edge of the text's resize grip,
// where Chrome collapses the selection to the start and tells only the
// document.
for (const name of ['documents', 'documents-react', 'documents-vue', 'documents-svelte']) {
  test(`${name}: caret (seed 4500000250)`, async ({ browser }) => {
    await replay(
      browser,
      name,
      '?revisions',
      [
        { k: 'key', key: 'Control+A', n: 1 },
        { k: 'click', x: 0.987, y: 0.981 },
      ],
      'caret',
    );
  });
}
