import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 1700000302 on react, vue and svelte (?random).
// Fails the `value-text` check after step 4: value "liste\n\n# Tunnel server configuration\n…" text "listen\n\n# Tunnel server configuration\n…"
for (const name of ['react', 'vue', 'svelte']) {
  test(`${name}: value-text (seed 1700000302)`, async ({ browser }) => {
    await replay(
      browser,
      name,
      '?random',
      [
        { k: 'lock', range: { at: 0.254, length: 30, line: true } },
        { k: 'lockPath', on: true },
        { k: 'type', text: 'listen' },
        { k: 'unlock' },
      ],
      'value-text',
    );
  });
}
