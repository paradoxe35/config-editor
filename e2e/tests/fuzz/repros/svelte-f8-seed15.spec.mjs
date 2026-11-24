import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 15 on svelte.
// Fails the `f8` check after step 19: after F8 1: caret at 1:1, problems at [[2,7,null,8],[2,7,null,8]]
test('svelte: f8 (seed 15)', async ({ browser }) => {
  await replay(
    browser,
    'svelte',
    '?random',
    [
      { k: 'clip', key: 'Control+C' },
      { k: 'key', key: 'Control+End', n: 2 },
      { k: 'type', text: 'value\ntrue' },
      { k: 'key', key: 'Shift+ArrowLeft', n: 4 },
      { k: 'prop', prop: 'rename' },
      { k: 'drag', x1: 0.051, y1: 0.48, x2: 0.02, y2: 0.404 },
      { k: 'key', key: 'Delete', n: 1 },
      { k: 'foldClick', at: 0.362 },
      { k: 'goto', line: 18, column: 9 },
      { k: 'wheel', dy: -600 },
      { k: 'key', key: 'Enter', n: 3 },
      { k: 'editAs', language: 'hcl' },
      { k: 'wheel', dy: 3000 },
      { k: 'type', text: '日本é' },
      { k: 'key', key: 'PageDown', n: 2 },
      { k: 'key', key: 'Control+Home', n: 2 },
      { k: 'type', text: '😀日本' },
      { k: 'hideShow', wait: 50 },
      { k: 'key', key: 'ArrowUp', n: 4 },
      { k: 'f8', shift: false, n: 2 },
    ],
    'f8',
  );
});
