// Runs a sequence of actions on a fresh page and checks the page after
// every step; shrinks a failing sequence and writes it as a Playwright test.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { generate, generateLock, simplify } from './actions.mjs';
import { TOKENS, YAML_DOCS } from './corpus.mjs';
import { checkSnapshot } from './checks.mjs';
import { knownClass } from './known.mjs';
import { PAGES, installProbe, pageUrl, snapshot } from './pages.mjs';
import { perform, settle } from './perform.mjs';
import { rng } from './rng.mjs';

const STEP_LIMIT_MS = 8000;
const here = dirname(fileURLToPath(import.meta.url));

// The sequence a seed gives on `name`: the page's query (late echoes or
// not) and its actions.
export function sequenceFor(name, seed, steps) {
  const r = rng(seed);
  const page = PAGES[name];
  // Half the hosts keep revisions and name the text of their lists; some
  // framework parents hand each edit back up to 600 ms late (a slow store),
  // so a hide and show finds several answers still to come.
  const query = page.vanilla
    ? r.pick(['', '?revisions'])
    : r.pick(['', '?random', '?revisions', '?random&revisions', '?random=600&revisions']);
  const userOnly = r.chance(0.35);
  const actions = Array.from({ length: steps }, () => generate(r, page, { userOnly }));
  // Most edits of a locked text happen long after the lock: start half the
  // host sequences with one (on the long text a quarter of the time, for
  // the rows in view).
  if (!userOnly && page.kind === 'single' && r.chance(0.25)) actions.unshift({ k: 'hostValue', text: YAML_DOCS[5], how: 'prop' });
  if (!userOnly && r.chance(0.5)) actions.splice(actions[0]?.k === 'hostValue' ? 1 : 0, 0, generateLock(r));
  return { query, actions: actions.slice(0, steps) };
}

function withLimit(promise, what, ms = STEP_LIMIT_MS) {
  let timer;
  const limit = new Promise((_, reject) => {
    timer = setTimeout(() => reject(Object.assign(new Error(`${what} took over ${ms} ms`), { hang: true })), ms);
  });
  return Promise.race([promise, limit]).finally(() => clearTimeout(timer));
}

// The run's host model: the locks it gave (per document), the secret
// placeholders it gives (`secrets`) and those in the text it last set on
// each document (`given`).
function hostModel(page, cdp, kind, name, strict, revisions) {
  const locks = new Map();
  const given = new Map();
  const key = (id) => id ?? '';
  return {
    name,
    strict,
    noted: new Map(),
    // How often each new action did what it is for (the run's coverage).
    coverage: new Map(),
    reached(what) {
      this.coverage.set(what, (this.coverage.get(what) ?? 0) + 1);
    },
    page,
    cdp,
    kind,
    // The page's own host (no framework component): its resets report nothing.
    vanilla: !!PAGES[name].vanilla,
    // A host keeping revisions and naming the text of its lists.
    revisions,
    secrets: new Set(TOKENS),
    hostReplaced: false,
    // The text the host last gave the single editor (its mount text, a
    // value it set): a path it names later locks the field only while the
    // field reads as there; `undefined` when the run cannot tell (a
    // re-mount, an Edit-as switch, a value equal to the editor's).
    hostText: undefined,
    // A path named after the user changed its field (see `lockPath`):
    // held with a reason, or locked once the field reads as the host's.
    pendingPath: null,
    footer: true,
    setLock(id, slot, lock) {
      const mine = locks.get(key(id)) ?? {};
      mine[slot] = lock;
      locks.set(key(id), mine);
    },
    // A new host text keeps the locks whose text it still holds, the range
    // lock moved to where the text now is; returns the ranges to give.
    keepLocks(id, text) {
      const mine = locks.get(key(id)) ?? {};
      for (const slot of Object.keys(mine)) if (mine[slot] && !text.includes(mine[slot].text)) mine[slot] = null;
      if (id === null) this.pendingPath = null;
      if (!mine.range) return [];
      const start = text.indexOf(mine.range.text);
      mine.range = { ...mine.range, range: { start, end: start + mine.range.text.length } };
      return [mine.range.range];
    },
    forgetLocks(id) {
      locks.delete(key(id));
      if (id === null) this.pendingPath = null;
    },
    // The host sets `text` on the document `id`: its placeholders are the
    // ones the document was given.
    give(id, text) {
      given.set(key(id), new Set(TOKENS.filter((token) => text.includes(token))));
    },
    givenOf(id) {
      return given.get(key(id)) ?? new Set();
    },
    // The placeholders other documents were given and `id` was not.
    givenElsewhere(id) {
      const own = given.get(key(id)) ?? new Set();
      const out = new Set();
      for (const [other, tokens] of given) if (other !== key(id)) for (const token of tokens) if (!own.has(token)) out.add(token);
      return out;
    },
    locksOf(id) {
      return Object.values(locks.get(key(id)) ?? {}).filter(Boolean);
    },
  };
}

// Checks until the page agrees or a second passes: the DOM follows the
// state a frame or two later, which is not a finding. A failure of a known
// class is noted (`run.noted`) and the sequence goes on, unless
// `run.strict`; returns the first other one.
// `views`: only what the page shows (the rows while scrolling through).
const VIEW_CHECKS = ['mounted', 'api', 'textarea', 'rows', 'dom-text', 'gutter'];
async function checkStable(page, run, { views = false } = {}) {
  const started = Date.now();
  let failures = [];
  do {
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    failures = checkSnapshot(await page.evaluate(snapshot), run).filter((failure) => !views || VIEW_CHECKS.includes(failure.check));
    if (!failures.length) return null;
    // Only known classes: a shorter wait tells them from a late frame.
    const known = failures.every((failure) => tolerated(run, failure));
    if (known && Date.now() - started > 300) break;
    await page.waitForTimeout(80);
  } while (Date.now() - started < 1000);
  return pickFailure(run, failures);
}

// The known class `failure` is of, unless the run stops on it: `strict`
// is true (every class) or the one check a repro replays.
function tolerated(run, failure) {
  if (run.strict === true || run.strict === failure.check) return null;
  return knownClass(failure, run.name);
}

function pickFailure(run, failures) {
  for (const failure of failures) {
    const known = tolerated(run, failure);
    if (!known) return failure;
    run.noted.set(known.id, (run.noted.get(known.id) ?? 0) + 1);
  }
  return null;
}

// Scrolls the textarea from top to bottom, checking the rows in view at
// each place: every line rendered once, none missing.
async function checkScrolledThrough(page, run) {
  const height = await page.evaluate(() => {
    const input = window.__fz.root()?.querySelector('.config-editor__input');
    return input ? { total: input.scrollHeight, view: input.clientHeight, top: input.scrollTop } : null;
  });
  if (!height || height.total <= height.view + 4) return null;
  // A screen at a time, at most 30 places.
  for (let top = 0; top < height.total; top += Math.max(40, height.view * 0.8, height.total / 30)) {
    await page.evaluate((top) => {
      const input = window.__fz.root().querySelector('.config-editor__input');
      input.scrollTop = top;
      input.dispatchEvent(new Event('scroll'));
    }, top);
    const failure = await checkStable(page, run, { views: true });
    if (failure) return { ...failure, detail: `scrolled to ${top}: ${failure.detail}` };
  }
  return null;
}

async function undoAll(page) {
  await page.evaluate(() => window.__fz.root()?.querySelector('.config-editor__input')?.focus());
  let last = null;
  for (let round = 0; round < 15; round += 1) {
    for (let index = 0; index < 20; index += 1) await page.keyboard.press('Control+Z');
    const now = await page.evaluate(() => window.__fz.editor()?.value);
    if (now === last) return now;
    last = now;
  }
  return last;
}

async function openPage(browser, name, query) {
  const context = await browser.newContext({ viewport: { width: 1100, height: 620 } });
  await context.grantPermissions(['clipboard-read', 'clipboard-write']).catch(() => {});
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (error) => errors.push(`pageerror: ${error}`));
  page.on('console', (message) => message.type() === 'error' && errors.push(`console: ${message.text()}`));
  // A close the host confirms is part of the page, not a finding.
  page.on('dialog', (dialog) => dialog.accept().catch(() => {}));
  await page.goto(pageUrl(name, query));
  await page.waitForFunction(() => document.body.dataset.ready === 'true', null, { timeout: 20000 });
  await page.locator('.config-editor__input').first().waitFor({ timeout: 20000 });
  await page.evaluate(installProbe, PAGES[name]);
  // The clipboard outlives the context: a paste must not see the last
  // sequence's copy (a repro would not replay).
  await page.evaluate(() => navigator.clipboard.writeText('')).catch(() => {});
  await page.waitForFunction(() => !!window.__fz.editor(), null, { timeout: 10000 });
  const cdp = await context.newCDPSession(page);
  return { context, page, errors, cdp };
}

// Runs `actions` on a fresh `name` page. Returns `{ failure, step }` (the
// first failed check and the step it followed) or `{ failure: null }`.
export async function runSequence(browser, name, query, actions, { strict = process.env.FUZZ_BROWSER_STRICT === '1' } = {}) {
  const { kind } = PAGES[name];
  const { context, page, errors, cdp } = await openPage(browser, name, query);
  const run = hostModel(page, cdp, kind, name, strict, new URLSearchParams(query).has('revisions'));
  const result = await runSteps(run, context, page, errors, kind, actions);
  // A known class found by a check that ends the sequence (host, undo,
  // errors, hang) is noted too.
  if (result.failure && !pickFailure(run, [result.failure])) result.failure = null;
  return { ...result, noted: Object.fromEntries(run.noted), coverage: Object.fromEntries(run.coverage) };
}

async function runSteps(run, context, page, errors, kind, actions) {
  try {
    await page.evaluate(() => window.__fz.root()?.querySelector('.config-editor__input')?.focus());
    const opening = new Map();
    const remember = async () => {
      const state = await page.evaluate(() => ({ id: window.__fz.activeId(), value: window.__fz.editor()?.value }));
      if (!opening.has(state.id ?? '')) opening.set(state.id ?? '', state.value);
    };
    await remember();
    if (kind === 'single') run.hostText = opening.get('');
    // An action that edits another document than the one it starts on
    // notes that one's opening text before it edits.
    run.remember = remember;
    const touched = new Set();
    for (let step = 0; step < actions.length; step += 1) {
      const action = actions[step];
      try {
        if (action.k === 'tab' || action.k === 'tabKey' || action.k === 'crossCopy') {
          if (!(await withLimit(settle(page), `settling before step ${step}`))) {
            const state = await page.evaluate(() => JSON.stringify([window.__fz.editor()?.value, window.__fz.host()]));
            return {
              failure: { check: 'host', detail: `before the tab switch the host has not followed: ${state.slice(0, 200)}` },
              step: step - 1,
            };
          }
        }
        if (action.k === 'pause') {
          if (!(await withLimit(settle(page, 2500), `settling at step ${step}`))) {
            const state = await page.evaluate(() => JSON.stringify([window.__fz.editor()?.value, window.__fz.host()]));
            return { failure: { check: 'host', detail: `the host has not followed: ${state.slice(0, 300)}` }, step };
          }
          continue;
        }
        const wrong = await withLimit(perform(run, action), `step ${step} (${action.k})`);
        if (wrong && pickFailure(run, [wrong])) return { failure: wrong, step };
        if (action.k === 'prop' && ['rename', 'readOnly'].includes(action.prop)) {
          run.hostReplaced = true;
          run.hostText = undefined;
        }
        await remember();
        const failure = await withLimit(checkStable(page, run), `checking step ${step}`);
        if (process.env.FUZZ_BROWSER_TRACE) {
          const now = await page.evaluate(() => [window.__fz.editor()?.text, window.__fz.host()]);
          console.log(`[fuzz] step ${step} ${JSON.stringify(action)} -> ${JSON.stringify(now).slice(0, 200)}`);
        }
        if (failure) return { failure, step };
        if (errors.length) {
          const thrown = { check: 'errors', detail: errors.join(' | ').slice(0, 300) };
          errors.length = 0;
          if (pickFailure(run, [thrown])) return { failure: thrown, step };
        }
        touched.add(await page.evaluate(() => window.__fz.activeId()));
      } catch (error) {
        // A wait an action gives up on (a page that never comes back) is a
        // hang too: shrunk and written as a repro, not an error that ends the run.
        if (error.hang || error.name === 'TimeoutError') {
          // What the page threw on the way (a panic) says why.
          const thrown = errors.length ? `; ${errors.join(' | ')}` : '';
          return { failure: { check: 'hang', detail: `${error.message.split('\n')[0]}${thrown}`.slice(0, 600) }, step };
        }
        throw error;
      }
    }
    const end = actions.length - 1;
    try {
      // The host's value follows the editor once its echoes settle (P4).
      if (!(await withLimit(settle(page, 2500), 'settling at the end', 10000))) {
        const state = await page.evaluate(() => {
          const fz = window.__fz;
          return {
            value: fz.editor()?.value,
            host: fz.host(),
            label: fz.root()?.querySelector('.config-editor__frame')?.getAttribute('aria-label'),
          };
        });
        // While editing as another language that cannot convert back, the
        // host keeps the last text that could (the editor says so).
        const insync = state.label?.startsWith('YAML') || kind === 'documents';
        if (insync || typeof state.value === 'string') {
          return {
            failure: {
              check: 'host',
              detail: `editor ${JSON.stringify(state.value).slice(0, 140)} host ${JSON.stringify(state.host).slice(0, 140)}`,
            },
            step: end,
          };
        }
      }
      const scrolled = await withLimit(checkScrolledThrough(page, run), 'scrolling through', 20000);
      if (scrolled) return { failure: scrolled, step: end };
      // Undo all the way gives back the opening text.
      if (!run.hostReplaced) {
        const ids = kind === 'documents' ? [...touched] : [null];
        for (const id of ids) {
          if (id !== null) {
            const tab = await page.evaluate((id) => (window.documentsHandle?.current ?? window.documentsView).select(id), id);
            if (!tab) continue;
          }
          const back = await withLimit(undoAll(page), 'undoing all', 30000);
          const want = opening.get(id ?? '');
          // An undo a lock holds back says so (the read-only notice); one
          // that renames or moves a locked field goes through on screen,
          // and its save waits, saying why (`saveHeld`, a problem).
          const said = await page.evaluate(
            () =>
              [...(window.__fz.root()?.querySelectorAll('.config-editor__read-only-notice, .config-editor__protect-tip') ?? [])].some(
                (notice) => !notice.hidden && notice.getClientRects().length && notice.textContent.trim() !== '',
              ) || (window.__fz.problems[window.__fz.activeId() ?? ''] ?? []).some((problem) => problem.message?.startsWith('Not saved: ')),
          );
          if (want !== undefined && back !== want && run.locked && !said) {
            return {
              failure: {
                check: 'undo-silent',
                detail: `undo all stops at ${JSON.stringify(back).slice(0, 140)} (opened with ${JSON.stringify(want).slice(0, 140)}) with no notice`,
              },
              step: end,
            };
          }
          if (want !== undefined && back !== want && !run.locked) {
            return {
              failure: {
                check: 'undo',
                detail: `undo all gives ${JSON.stringify(back).slice(0, 140)}, opened with ${JSON.stringify(want).slice(0, 140)}`,
              },
              step: end,
            };
          }
        }
      }
      if (errors.length) return { failure: { check: 'errors', detail: errors.join(' | ').slice(0, 300) }, step: end };
    } catch (error) {
      if (error.hang || error.name === 'TimeoutError') return { failure: { check: 'hang', detail: error.message.split('\n')[0] }, step: end };
      throw error;
    }
    return { failure: null };
  } finally {
    await context.close().catch(() => {});
  }
}

// Shrinks `actions` while the same check still fails: drops runs of actions
// (halves, quarters, … single ones), then simplifies each one left.
export async function shrink(browser, name, query, actions, check, { budget = 60, log = () => {}, strict } = {}) {
  let best = actions;
  let tries = 0;
  const options = strict === undefined ? {} : { strict };
  const fails = async (candidate) => {
    tries += 1;
    const result = await runSequence(browser, name, query, candidate, options);
    return result.failure?.check === check ? result : null;
  };
  for (let size = Math.ceil(best.length / 2); size >= 1 && tries < budget; size = size === 1 ? 0 : Math.ceil(size / 2)) {
    for (let start = 0; start < best.length && tries < budget; ) {
      const candidate = [...best.slice(0, start), ...best.slice(start + size)];
      if (candidate.length && (await fails(candidate))) {
        best = candidate;
        log(`shrunk to ${best.length} actions`);
      } else start += size;
    }
  }
  for (let index = 0; index < best.length && tries < budget; index += 1) {
    for (let simpler = simplify(best[index]); simpler && tries < budget; simpler = simplify(simpler)) {
      const candidate = best.map((action, at) => (at === index ? simpler : action));
      if (!(await fails(candidate))) break;
      best = candidate;
    }
  }
  const last = await runSequence(browser, name, query, best, options);
  return { actions: best, result: last, tries };
}

// Writes a shrunk failure as a test to paste into a spec (skipped with
// `test.fixme` until it is fixed).
export function writeRepro({ name, query, seed, actions, failure, step, known, file: base }) {
  const dir = join(here, 'repros');
  mkdirSync(dir, { recursive: true });
  const file = join(dir, base ?? `${known ? `${known.id}-` : ''}${name}-${failure.check}-seed${seed}.spec.mjs`);
  const title = `${known ? `${known.id} ` : ''}${name}: ${failure.check} (seed ${seed})`;
  const body = `import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed ${seed} on ${name}${query ? ` (${query})` : ''}.
// Fails the \`${failure.check}\` check after step ${step}: ${failure.detail.replace(/\n/g, ' ').replace(/\*\//g, '* /')}
test.fixme(${JSON.stringify(title)}, async ({ browser }) => {
  await replay(browser, ${JSON.stringify(name)}, ${JSON.stringify(query)}, ${JSON.stringify(actions, null, 2).replace(/\n/g, '\n  ')}, ${JSON.stringify(failure.check)});
});
`;
  writeFileSync(file, body);
  return file;
}

// Replays a sequence and throws on its first failed check that is not of
// another known class than `check`'s (for repros).
export async function replay(browser, name, query, actions, check) {
  const { failure, step } = await runSequence(browser, name, query, actions, { strict: check ?? true });
  if (failure) throw new Error(`${failure.check} after step ${step}: ${failure.detail}`);
}
