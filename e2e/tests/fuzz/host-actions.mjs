// The host's actions beside values and locks: its own change through the
// handle (`applyChanges`), unmounting and mounting the component again
// (`setShown`), and problems it found on a text it holds, given late.
import { lockIndices } from './perform.mjs';

// A line put in at a line start of the text the host holds (named by its
// revision, maybe behind the editor's: the editor rebases it over the
// typing since) or, with no revision to name, of the host's text now
// while it holds the editor's (the text the editor last told it).
// Never inside a locked text, wherever that text stands. The host's
// change is not undone by Undo: the opening text is no longer the goal.
export async function hostChange(run, action) {
  const { page } = run;
  const id = await page.evaluate(() => window.__fz.activeId());
  const locks = run.locksOf(id).map((lock) => lock.text);
  const result = await page.evaluate(
    ([action, locks]) => {
      const fz = window.__fz;
      const editor = fz.editor();
      if (!editor || typeof editor.applyChanges !== 'function') return { skipped: true };
      const parent = fz.parent();
      const named =
        fz.revisions && action.base && !!parent && typeof parent.text === 'string' && parent.revision !== undefined && parent.revision !== null;
      // With no revision the change is on the host's text now, the one the
      // editor last told it: only while the host holds it (settled).
      if (!named && (!parent || parent.text !== editor.value)) return { skipped: true };
      const text = named ? parent.text : editor.value;
      if (typeof text !== 'string') return { skipped: true };
      const at = text.lastIndexOf('\n', Math.floor(action.at * text.length) - 1) + 1;
      for (const locked of locks) {
        for (let from = text.indexOf(locked); from >= 0; from = text.indexOf(locked, from + 1)) {
          if (from < at && at < from + locked.length) return { skipped: true };
        }
      }
      try {
        const changes = [{ from: at, to: at, insert: `${action.line}\n` }];
        const options = named ? { revision: parent.revision } : undefined;
        // A framework documents page through its handle (which reports the
        // change to the parent), the others through the editor.
        const handle = fz.kind === 'documents' && !window.documentsView ? window.documentsHandle.current : null;
        const revision = handle ? handle.applyChanges(fz.activeId(), changes, options) : editor.applyChanges(changes, options);
        // A vanilla host knows its change: its text is the result (a
        // framework handle reports it through `onChange`).
        if (revision !== null && revision !== undefined && (window.editor || window.documentsView)) {
          if (fz.kind === 'single') {
            document.getElementById('saved').textContent = editor.value;
            window.savedRevision = revision;
          } else {
            window.updateFile(fz.activeId(), { value: editor.value, ...(fz.revisions ? { valueRevision: revision } : {}) });
          }
        }
        return { revision, named };
      } catch (error) {
        return { error: String(error) };
      }
    },
    [action, locks],
  );
  if (result.error) return { check: 'api', detail: `applyChanges: ${result.error}` };
  if (result.revision === null || result.revision === undefined) return null;
  run.reached(result.named ? 'host change on an older revision' : 'host change');
  run.hostReplaced = true;
  run.hostText = undefined;
  return null;
}

// The framework page's parent unmounts the component and mounts it again
// (`setShown`), `wait` ms apart, late echoes on their way or not. The new
// editor knows only what the parent renders (the README's "Hidden and
// shown again, the editor knows only what you render: give `rangesFor`
// ... so locks worked out on another text land on their text"): a
// revisions host names the text its range was worked out on; one with no
// revisions works its range out again on the value it shows, in the task
// that shows it, and names that text too. A late answer for a text the
// gone editor sent is the parent's own text for the new one (no
// `echoKey`): rendered with it, a list naming no text would be read in it.
export async function hideShow(run, { wait }) {
  const { page } = run;
  if (!(await page.evaluate(() => typeof window.setShown === 'function' && !window.editor))) return null;
  await page.evaluate(() => window.setShown(false));
  if (wait) await page.waitForTimeout(wait);
  // The range lock's text (as `keepLocks` finds it again).
  const locked = run.revisions ? null : (run.locksOf(null).find((lock) => lock.range)?.text ?? null);
  const shownWith = await page.evaluate((locked) => {
    const fz = window.__fz;
    const text = window.parentState().text;
    if (locked !== null) {
      const start = text.indexOf(locked);
      const readOnlyRanges = start < 0 ? [] : [{ start, end: start + locked.length }];
      fz.extra = { ...fz.extra, readOnlyRanges, rangesFor: text };
      window.setExtraProps({ ...fz.extra });
    }
    window.setShown(true);
    return text;
  }, locked);
  if (locked !== null) run.keepLocks(null, shownWith);
  await page.waitForFunction(() => !!window.__fz.editor() && !!window.__fz.root(), null, { timeout: 5000 });
  run.reached('hide and show');
  run.hostReplaced = true;
  run.hostText = undefined;
  return null;
}

// A warning on a line of the text the host holds, `wait` ms later, named
// by that text's revision or by the text itself (a revisions host; a
// v0.1.0 host gives it alone, read in its value).
export async function lateDiagnostics(run, { at, by, wait }) {
  const { page } = run;
  const state = await page.evaluate(() => window.__fz.parent());
  if (!state || typeof state.text !== 'string') return null;
  if (wait) await page.waitForTimeout(wait);
  const line = lockIndices(state.text, { at, length: 1, line: true });
  await page.evaluate(
    ([state, start, by]) => {
      const fz = window.__fz;
      const number = start === null ? 1 : state.text.slice(0, start).split('\n').length;
      const hostDiagnostics = [
        { range: { startLine: number, startColumn: 1 }, severity: 'warning', message: 'checked by the host', source: 'host' },
      ];
      const named = by === 'revision' && state.revision !== undefined && state.revision !== null ? state.revision : state.text;
      // Problems given alone are read in the text the ranges name: after a
      // hide and show that is a text, not the value (see `hideShow`).
      const alone = fz.extra?.rangesFor === undefined ? { hostDiagnostics } : { hostDiagnostics, diagnosticsFor: state.text };
      const props = fz.revisions ? { hostDiagnostics, diagnosticsFor: named } : alone;
      // F8 also walks onto the host's problem (at a line start, or in the
      // footer once the text has no such line).
      fz.hostDiagnosed = true;
      fz.hostMessage = hostDiagnostics[0].message;
      if (fz.kind === 'single' && window.editor) window.editor.update(props);
      else if (fz.kind === 'single') {
        fz.extra = { ...fz.extra, ...props };
        window.setExtraProps({ ...fz.extra });
      } else window.updateFile(fz.activeId(), props);
    },
    [state, line ? line.start : null, by],
  );
  run.reached('late diagnostics');
  return null;
}

// Takes the problems `lateDiagnostics` gave back, before the host gives a
// text of its own: they were for the text it replaces.
export async function clearHostDiagnostics(page) {
  await page.evaluate(() => {
    const fz = window.__fz;
    if (!fz.hostDiagnosed) return;
    fz.hostDiagnosed = false;
    const props = { hostDiagnostics: [], diagnosticsFor: undefined };
    if (fz.kind === 'single' && window.editor) window.editor.update({ hostDiagnostics: [] });
    else if (fz.kind === 'single') {
      fz.extra = { ...fz.extra, ...props };
      window.setExtraProps({ ...fz.extra });
    } else window.updateFile(fz.activeId(), props);
  });
}
