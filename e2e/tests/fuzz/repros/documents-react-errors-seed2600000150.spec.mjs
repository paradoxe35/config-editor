import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// A lock given after an `e` and its combining accent came back from the
// host in the wrong order (the host on the older text): the fuzz works the
// range out on the host's text, which it gives it with.
// Shrunk by the browser fuzz from seed 2600000150 on documents-react.
test('documents-react: errors (seed 2600000150)', async ({ browser }) => {
  await replay(browser, 'documents-react', '?random', [
    { k: 'lock', range: { at: 0.369, length: 3, line: true } },
    { k: 'type', text: 'é' },
    { k: 'click', x: 0.968, y: 0.114 },
    { k: 'lock', range: { at: 0.668, length: 30, line: false } },
  ]);
});
