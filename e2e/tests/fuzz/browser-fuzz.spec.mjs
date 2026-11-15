import { expect, test } from '@playwright/test';
import { knownClass } from './known.mjs';
import { BUILT, PAGES } from './pages.mjs';
import { runSequence, sequenceFor, shrink, writeRepro } from './runner.mjs';

// A seeded random suite in the real browser: random keyboard, mouse,
// clipboard, IME and host sequences on every example page, checking after
// each step that the page shows what the editor says (text, folds, rows in
// view, the caret's line), that the host hears the document, that locked
// and secret text stays, that undo gives back the opening text and that
// nothing throws or hangs.
//
// FUZZ_BROWSER_RUNS: sequences per page (default 16: about 2 minutes in all on 2 workers here, 3-4 on CI);
// FUZZ_BROWSER_STEPS: actions per sequence (default 20);
// FUZZ_BROWSER_SEED: the first seed (default 1; each page's seeds follow);
// FUZZ_BROWSER_PAGES: a comma list of pages (default all);
// FUZZ_BROWSER_STRICT=1: stop on known classes too (to shrink them).
const RUNS = Number(process.env.FUZZ_BROWSER_RUNS) || 16;
const STEPS = Number(process.env.FUZZ_BROWSER_STEPS) || 20;
const SEED = Number(process.env.FUZZ_BROWSER_SEED) || 1;
const ONLY = process.env.FUZZ_BROWSER_PAGES?.split(',');

// Each sequence opens its own context: no screenshots or traces of them all.
test.use({ trace: 'off', screenshot: 'off' });

test.describe('browser fuzz', () => {
  test.skip(!BUILT, 'js packages not built: run `npm run build` in js/');

  for (const name of Object.keys(PAGES)) {
    if (ONLY && !ONLY.includes(name)) continue;
    test(`random sequences on ${name}`, async ({ browser }, info) => {
      test.setTimeout(RUNS * 45_000 + 240_000);
      const unknown = [];
      const counts = {};
      const started = Date.now();
      for (let run = 0; run < RUNS; run += 1) {
        const seed = SEED + run;
        const { query, actions } = sequenceFor(name, seed, STEPS);
        const result = await runSequence(browser, name, query, actions);
        for (const [id, count] of Object.entries(result.noted)) counts[id] = (counts[id] ?? 0) + count;
        const noted = Object.keys(result.noted).join(',');
        if (!result.failure) {
          console.log(`[fuzz] ${name} seed ${seed}: ok${noted ? ` (known: ${noted})` : ''}`);
          continue;
        }
        const known = knownClass(result.failure, name);
        const label = known ? known.id : 'NEW';
        counts[label] = (counts[label] ?? 0) + 1;
        console.log(
          `[fuzz] ${name} seed ${seed}${query}: ${label} ${result.failure.check} after step ${result.step}: ${result.failure.detail}`,
        );
        if (known && process.env.FUZZ_BROWSER_STRICT !== '1') continue;
        // One shrink per new check a run: the next ones are likely the same class.
        if (!known && unknown.some((seen) => seen.failure.check === result.failure.check)) {
          unknown.push({ seed, failure: result.failure, file: null });
          continue;
        }
        const prefix = actions.slice(0, result.step + 1);
        const shrunk = await shrink(browser, name, query, prefix, result.failure.check, {
          log: (line) => console.log(`[fuzz] ${name} seed ${seed}: ${line}`),
        });
        const final = shrunk.result.failure ? shrunk.result : { ...result, step: prefix.length - 1 };
        const file = writeRepro({
          name,
          query,
          seed,
          actions: shrunk.result.failure ? shrunk.actions : prefix,
          failure: final.failure,
          step: final.step,
          known,
        });
        console.log(
          `[fuzz] ${name} seed ${seed}: ${shrunk.actions.length} actions after ${shrunk.tries} tries: ${JSON.stringify(shrunk.actions)}\n[fuzz] repro: ${file}`,
        );
        if (!known) unknown.push({ seed, failure: final.failure, file });
      }
      const seconds = Math.round((Date.now() - started) / 1000);
      console.log(`[fuzz] ${name}: ${RUNS} sequences of ${STEPS} in ${seconds} s; failures ${JSON.stringify(counts)}`);
      info.annotations.push({ type: 'fuzz', description: `${RUNS} sequences, ${JSON.stringify(counts)}` });
      expect(unknown, 'new failure classes (see the repros; list them in known.mjs)').toEqual([]);
    });
  }
});
