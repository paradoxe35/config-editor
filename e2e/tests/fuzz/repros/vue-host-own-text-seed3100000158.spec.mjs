import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// The class behind seed 3100000158 on vue (?random&revisions): the host's
// own text (the echo action sets the text it holds, with no revision) given
// while an older echo is still on its way. The host keeps the newest
// revision it heard, so that echo is dropped and the host stays on the
// document. The echoes come back in a random order: each round catches it
// now and then, so there are several.
for (const name of ['react', 'vue', 'svelte']) {
  test(`${name}: an older echo after the host's own text (seed 3100000158)`, async ({ browser }) => {
    const rounds = ['abcdefghijkl', 'mnopqrstuvwx', 'ABCDEFGHIJKL', 'MNOPQRSTUVWX', '012345678901', 'yzYZyzYZyzYZ'];
    const actions = rounds.flatMap((text) => [{ k: 'type', text }, { k: 'echo' }]);
    await replay(browser, name, '?random&revisions', [...actions, { k: 'pause' }], 'host');
  });
}
