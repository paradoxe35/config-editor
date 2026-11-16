import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 3206 on svelte (?random). Timing-dependent:
// it failed in 2 of 46 replays inside the long run (two workers busy) and not
// in 30 replays alone. Fails the `textarea` check after the last step: textarea "日本"
// but the editor's text (folds cut) "\n": the IME commit is in the textarea, not
// in the document.
test('BR38-6 svelte: textarea (seed 3206)', async ({ browser }) => {
  // Twenty replays of about 2.5 s each (every echo 0-200 ms late, and the
  // check after the pause): more than the default minute on a busy runner.
  test.setTimeout(20 * 10_000);
  for (let run = 0; run < 20; run += 1) {
    await replay(
      browser,
      'svelte',
      '?random',
      [
        {
          k: 'type',
          text: 'x中文字',
        },
        {
          k: 'editAs',
          language: 'xml',
        },
        {
          k: 'click',
          x: 0.254,
          y: 0.707,
        },
        {
          k: 'type',
          text: '# note',
        },
        {
          k: 'clip',
          key: 'Control+C',
        },
        {
          k: 'key',
          key: 'Shift+PageDown',
          n: 2,
        },
        {
          k: 'jump',
          at: 0.285,
          length: 1,
        },
        {
          k: 'ime',
          steps: ['z', 'zh', 'zho', 'zhong'],
          commit: '中',
        },
        {
          k: 'key',
          key: 'Shift+PageDown',
          n: 4,
        },
        {
          k: 'type',
          text: ': ',
        },
        {
          k: 'type',
          text: '&a',
        },
        {
          k: 'find',
          query: 'key',
          replace: '',
          all: true,
        },
        {
          k: 'key',
          key: 'Control+/',
          n: 1,
        },
        {
          k: 'wheel',
          dy: -120,
        },
        {
          k: 'ime',
          steps: ['にほん'],
          commit: '日本',
        },
      ],
      'textarea',
    );
  }
});
