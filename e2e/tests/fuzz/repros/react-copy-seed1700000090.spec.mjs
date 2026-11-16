import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 1700000090 on react (?random); svelte
// and vue failed alike. Fails the `copy` check after the last step: copied
// "␀ before the copy" (nothing), selection "👧", the end of a family emoji
// whose first half was typed after a joined pair already there.
test('react: copy of a selection inside a joined emoji (seed 1700000090)', async ({ browser }) => {
  await replay(
    browser,
    'react',
    '?random',
    [
      { k: 'jump', at: 0.527, length: 0 },
      { k: 'ime', steps: ['é'], commit: 'é' },
      { k: 'type', text: '👨‍👩‍' },
      { k: 'jump', at: 0.569, length: 1 },
      { k: 'clip', key: 'Control+C' },
    ],
    'copy',
  );
});
