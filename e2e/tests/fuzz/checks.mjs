// What must hold after every step, judged from one snapshot (see
// `pages.mjs`): the page against what the editor itself says. Each check
// returns null or `{ check, detail }`.
import { checkSecrets } from './secrets.mjs';

// The textarea's text: the document with the lines inside closed folds cut
// out (`startLine` stays, `startLine + 1 ..= endLine` hidden).
export function projection(text, folded) {
  const lines = text.split('\n');
  const hidden = new Set();
  for (const { startLine, endLine } of folded) for (let line = startLine + 1; line <= endLine; line += 1) hidden.add(line);
  return {
    visible: lines.map((_, index) => index + 1).filter((line) => !hidden.has(line)),
    text: lines.filter((_, index) => !hidden.has(index + 1)).join('\n'),
    lines,
  };
}

const short = (value) => JSON.stringify(value).slice(0, 160);

// The document line the textarea offset `offset` is on.
function lineAt(projected, taValue, offset) {
  const row = taValue.slice(0, offset).split('\n').length - 1;
  return projected.visible[row];
}

// Every failed check of `snap`: at most one per group (what the page
// shows, locks and secrets, secrets anywhere on the page, the caret), so
// a known caret class does not hide a lock break after it.
export function checkSnapshot(snap, run) {
  if (snap.missing) return [{ check: 'mounted', detail: 'no editor on screen' }];
  if (typeof snap.text !== 'string') return [{ check: 'api', detail: `text is ${short(snap.text)}` }];
  const projected = projection(snap.text, snap.folded);
  const numbers = snap.rows.map((row) => row.n);
  return [checkView(snap, projected), checkHost(snap, run), checkSecrets(snap, run), checkCaret(snap, projected, numbers)].filter(Boolean);
}

function checkView(snap, projected) {
  if (snap.ta.value !== projected.text) {
    return { check: 'textarea', detail: `textarea ${short(snap.ta.value)} but text with folds cut ${short(projected.text)}` };
  }
  // Each rendered row shows its line.
  for (const row of snap.rows) {
    const want = projected.lines[row.n - 1];
    if (want === undefined) return { check: 'rows', detail: `row ${row.n} rendered past the end (${projected.lines.length} lines)` };
    if (row.t !== want) return { check: 'dom-text', detail: `line ${row.n} shows ${short(row.t)}, text has ${short(want)}` };
  }
  // The rendered rows: in order, none twice, none missing, none hidden in a
  // fold, and the rows in view among them.
  const numbers = snap.rows.map((row) => row.n);
  if (new Set(numbers).size !== numbers.length) return { check: 'rows', detail: `a line rendered twice: ${short(numbers)}` };
  if (numbers.length) {
    const first = projected.visible.indexOf(numbers[0]);
    if (first < 0) return { check: 'rows', detail: `line ${numbers[0]} rendered inside a fold` };
    for (let index = 0; index < numbers.length; index += 1) {
      if (projected.visible[first + index] !== numbers[index]) {
        return { check: 'rows', detail: `rendered ${short(numbers)}; row ${first + index} is line ${projected.visible[first + index]}` };
      }
    }
    const { scrollTop, clientHeight, lineHeight, paddingTop } = snap.ta;
    if (lineHeight > 0 && clientHeight > 0) {
      const top = Math.max(0, Math.floor((scrollTop - paddingTop) / lineHeight));
      const bottom = Math.min(projected.visible.length - 1, Math.floor((scrollTop + clientHeight - paddingTop - 1) / lineHeight));
      if (top < first || bottom > first + numbers.length - 1) {
        return { check: 'rows', detail: `rows ${top}..${bottom} in view, rendered rows ${first}..${first + numbers.length - 1}` };
      }
    }
  } else if (projected.visible.length > 1) {
    return { check: 'rows', detail: 'no rows rendered' };
  }
  if (snap.gutter.join() !== numbers.join()) return { check: 'gutter', detail: `gutter ${short(snap.gutter)} code ${short(numbers)}` };
  return null;
}

// Where each secret placeholder stands in `text`: `[from, to)` pairs.
function placeholders(text) {
  return [...text.matchAll(/«[^»\n]*»?/g)].map((found) => [found.index, found.index + found[0].length]);
}

function checkHost(snap, run) {
  // Locks and secrets (P1), judged on the host's language.
  if (typeof snap.value === 'string') {
    for (const lock of run.locksOf(snap.activeId)) {
      if (!snap.value.includes(lock.text))
        return { check: 'lock', detail: `locked ${short(lock.text)} gone from ${short(snap.value)}`, locked: lock.text, value: snap.value };
    }
    // The editor marks each lock where it is (it may mark more: a lock
    // ending inside a word takes the word). Judged for a lock whose text
    // is in the text once, on its rows in view.
    if (snap.ownLanguage) {
      for (const lock of run.locksOf(snap.activeId)) {
        const at = snap.text.indexOf(lock.text);
        if (at < 0 || snap.text.indexOf(lock.text, at + 1) >= 0) continue;
        // A secret placeholder shows as a secret, locked or not: so does a
        // lock that holds only a piece of one.
        if (/[«»]/.test(lock.text)) continue;
        if (placeholders(snap.text).some(([from, to]) => from < at + lock.text.length && at < to)) continue;
        const first = snap.text.slice(0, at).split('\n').length;
        const pieces = lock.text.split('\n');
        for (let index = 0; index < pieces.length; index += 1) {
          const piece = pieces[index].trim();
          const row = snap.rows.find((candidate) => candidate.n === first + index);
          if (!piece || !row) continue;
          if (!row.m.includes(piece)) {
            return { check: 'lock-marks', detail: `line ${row.n}: locked ${short(piece)}, marked ${short(row.m)}` };
          }
        }
      }
    }
    for (const secret of snap.value.match(/«[^»\n]*»?/g) ?? []) {
      if (!run.secrets.has(secret)) return { check: 'secret', detail: `${short(secret)} in ${short(snap.value)}` };
    }
    // A lock reading otherwise holds the change, saying so (`saveHeld`):
    // the value stays the last one the host heard. So does a text that does
    // not parse writing a locked key again (the error shown).
    const held =
      (snap.problems ?? []).some((problem) => problem.message.startsWith('Not saved: ')) ||
      (run.locksOf(snap.activeId).length > 0 && snap.errored);
    if (snap.ownLanguage && snap.value !== snap.text && !held) {
      // `unlocked`: the host took locks away earlier in the run.
      return { check: 'value-text', detail: `value ${short(snap.value)} text ${short(snap.text)}`, unlocked: !!run.unlocked };
    }
  } else if (snap.value && snap.value.error) {
    return { check: 'api', detail: snap.value.error };
  }
  return null;
}

function checkCaret(snap, projected, numbers) {
  // The caret's line is the active one, and the path the API gives for the
  // caret is the path at the DOM's caret (the selection's moving end).
  const head = snap.ta.head;
  if (snap.ta.focused) {
    const caretLine = lineAt(projected, snap.ta.value, head);
    const name = snap.ta.start === snap.ta.end ? 'caret' : 'selection-head';
    if (numbers.includes(caretLine) && snap.active.join() !== String(caretLine)) {
      return { check: name, detail: `caret on line ${caretLine}, active ${short(snap.active)}` };
    }
  }
  if (snap.caretPath) {
    if (snap.caretPath.error) return { check: 'api', detail: snap.caretPath.error };
    if (snap.caretPath.at !== snap.caretPath.current) {
      const name = snap.ta.start === snap.ta.end ? 'caret-path' : 'selection-head';
      return {
        check: name,
        detail: `currentPath ${short(snap.caretPath.current)}, pathAt(caret ${snap.caretPath.offset}) ${short(snap.caretPath.at)}`,
      };
    }
  }
  return null;
}
