// Secret placeholders (P1): nothing on the page or the clipboard holds a
// token the host did not give (a piece of one, one cut or glued), a token
// the host gave a document is never glued to other text in it without a
// problem saying so, and copying a token from one document into another
// keeps it whole. The pages hold no stored secret, only the placeholders
// standing for them: what the host gives is all a page could show.
import { DOC_LANGUAGE, copyFits, host, settle } from './perform.mjs';
import { docsFor } from './corpus.mjs';

const short = (value) => JSON.stringify(value).slice(0, 160);

// The tokens (and pieces of one) in `text`.
const tokens = (text) => text.match(/«[^»\n]{0,60}»?/g) ?? [];

// What may stand right before and after a placeholder written whole in a
// field: the quotes, separators and brackets around a value.
const BEFORE = /[\s"'=:,>([{|]/;
const AFTER = /[\s"',<)\]}|;]/;

// The checks of one snapshot: tokens anywhere on the page, then a token
// given to the document on screen glued to other text on a line with no
// problem listed.
export function checkSecrets(snap, run) {
  const { whole = [], pieces = [] } = snap.pageTokens ?? {};
  for (const token of whole) {
    if (!run.secrets.has(token)) return { check: 'secret-dom', detail: `the page shows ${short(token)}` };
  }
  for (const piece of pieces) {
    if (![...run.secrets].some((token) => token.startsWith(piece)))
      return { check: 'secret-dom', detail: `the page shows ${short(piece)}` };
  }
  if (typeof snap.value !== 'string' || !snap.ownLanguage) return null;
  for (const token of run.givenOf(snap.activeId)) {
    for (let at = snap.value.indexOf(token); at >= 0; at = snap.value.indexOf(token, at + 1)) {
      const before = snap.value[at - 1];
      const after = snap.value[at + token.length];
      if ((before === undefined || BEFORE.test(before)) && (after === undefined || AFTER.test(after))) continue;
      const line = snap.value.slice(0, at).split('\n').length;
      if (listedOn(snap, line)) continue;
      const text = snap.value.split('\n')[line - 1];
      return { check: 'secret-glue', detail: `line ${line} ${short(text)} glues ${token}, and no problem is listed there` };
    }
  }
  // One of another document's placeholders, which this one was not given:
  // it never stands for that secret here, and says so.
  for (const token of run.givenElsewhere(snap.activeId)) {
    const at = snap.value.indexOf(token);
    if (at < 0) continue;
    const line = snap.value.slice(0, at).split('\n').length;
    if (listedOn(snap, line)) continue;
    const text = snap.value.split('\n')[line - 1];
    return {
      check: 'secret-cross',
      detail: `${snap.activeId}: line ${line} ${short(text)} holds another document's ${token}, and no problem is listed there`,
    };
  }
  return null;
}

// Whether a problem is listed on `line` of the document on screen.
const listedOn = (snap, line) =>
  (snap.problems ?? []).some((problem) => problem.line === line || (problem.line < line && problem.endLine >= line));

// The clipboard holds no token the host did not give.
export async function clipboardTokens(run) {
  const copied = await run.page.evaluate(() => navigator.clipboard.readText().catch(() => ''));
  const wrong = tokens(copied).find((token) => !run.secrets.has(token));
  return wrong ? { check: 'secret-clipboard', detail: `the clipboard holds ${short(wrong)} (${short(copied)})` } : null;
}

async function selectTab(page, index) {
  const tabs = page.locator('[role="tab"]:visible');
  const count = await tabs.count();
  if (count < 2) return false;
  await tabs
    .nth(index % count)
    .click({ timeout: 2000 })
    .catch(() => {});
  return true;
}

// Selects in the document on screen a token (`token`, when it holds one)
// or `length` characters at `fraction` of its text: the text selected.
function selectIn(page, { token, fraction, length }) {
  return page.evaluate(
    ({ token, fraction, length }) => {
      const editor = window.__fz.editor();
      const text = editor?.text;
      if (typeof text !== 'string') return null;
      const found = token ? /«[^»\n]*»/.exec(text) : null;
      let start = found ? found.index : Math.floor(fraction * text.length);
      let end = found ? start + found[0].length : Math.min(text.length, start + length);
      ({ start, end } = window.__fz.snap(text, start, end));
      editor.revealRange({ start, end });
      return text.slice(start, end);
    },
    { token, fraction, length },
  );
}

// Copies from one document (a token most of the time, the host giving it
// first with `load`) and pastes into another, at a place or over a few
// characters there. The copy holds the text selected (a token whole).
export async function crossCopy(run, action) {
  const { page } = run;
  if (!(await selectTab(page, action.from))) return null;
  if (action.load) {
    const id = await page.evaluate(() => window.__fz.activeId());
    const secret = docsFor(DOC_LANGUAGE[id] ?? 'yaml').filter((doc) => doc.includes('«'));
    const text = secret[Math.floor(action.at2 * secret.length)];
    if (text) {
      run.hostReplaced = true;
      const ranges = run.keepLocks(id, text);
      run.give(id, text);
      await host(page, 'docValue', { text, how: 'file', ranges });
      await settle(page);
    }
  }
  const picked = await selectIn(page, { token: action.token, fraction: action.at, length: action.length });
  if (!picked) return null;
  // What the selection is once the editor placed it (never inside a
  // character or a placeholder).
  const want = await page.evaluate(async () => {
    await navigator.clipboard.writeText('␀ before the copy');
    return window.__fz.selected();
  });
  await page.keyboard.press('Control+C');
  await page.waitForTimeout(40);
  const wrong = await clipboardTokens(run);
  if (wrong) return wrong;
  if (want?.text) {
    const unlike = await copyFits(page, want);
    if (unlike) return unlike;
  }
  await settle(page);
  if (!(await selectTab(page, action.to))) return null;
  await run.remember?.();
  await page.evaluate(
    ({ fraction, over }) => {
      const editor = window.__fz.editor();
      const text = editor?.text;
      if (typeof text !== 'string') return;
      let start = Math.floor(fraction * text.length);
      let end = Math.min(text.length, start + over);
      ({ start, end } = window.__fz.snap(text, start, end));
      editor.revealRange({ start, end });
    },
    { fraction: action.at2, over: action.over },
  );
  await page.keyboard.press('Control+V');
  await page.waitForTimeout(40);
  run.reached(/«/.test(picked) ? 'placeholder pasted into another document' : 'text pasted into another document');
  return null;
}
