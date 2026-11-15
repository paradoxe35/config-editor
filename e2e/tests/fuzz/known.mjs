// The failure classes the browser fuzz knows, each a finding to fix: a
// failure matching one is noted and the sequence goes on; any other fails
// the run (FUZZ_BROWSER_STRICT=1 stops on known ones too, to shrink them).
// `severity`: HIGH / MEDIUM for P1, P2, P4 and P5 breaks on common files,
// LOW for edge cases. `match(failure, page)` takes `{ check, detail }`.
// Remove an entry when its finding is fixed (its repro under `repros/`
// then passes: drop the `fixme`).
export const KNOWN = [
  {
    id: 'BR38-1',
    severity: 'LOW',
    check: 'selection-head',
    summary:
      'With a forward selection (Shift+Down, a drag down) the editor takes the selection start for the caret: the active line, the breadcrumb (onPathChange / currentPath) stay on the anchor line while the caret is on another.',
    cause: 'crates/config-editor-leptos/src/editor/input.rs (selection sync and key_up note_caret(start))',
  },
  {
    id: 'BR38-2',
    severity: 'LOW',
    check: 'caret',
    summary:
      'After the host replaces the text (value prop, replaceValue) the textarea selection is mapped but the editor does not record the caret: no line (or the old one) is marked active and currentPath answers for the old caret, until the caret moves.',
    cause: 'crates/config-editor-leptos/src/editor/edit/pipeline.rs (place_selection, After::Mapped sets the selection without note_caret)',
  },
  {
    id: 'BR38-3',
    severity: 'HIGH',
    check: 'lock',
    // Each line of the locked text is still a line (its first piece ends
    // one, its last starts a later one, the ones between are whole lines):
    // the stretch changed only between or after its lines.
    match: ({ locked, value }) => !!locked && linesKept(locked, value),
    summary:
      'P1: a read-only range over a line break changes while each of its lines stays: a line typed, pasted or Entered inside a multi-line lock (at the start of its second line) goes in, and a lock ending with its line break loses it when everything after is deleted or the last line is cut (Ctrl+X with nothing selected).',
    cause:
      'crates/config-editor-leptos/src/editor/protect/guard/edit.rs (kept() falls back to lines_kept / kept_by_one_diff: a line diff keeps every locked line, so the stretch between or after them is not judged)',
  },
  {
    id: 'BR38-5',
    severity: 'HIGH',
    check: 'secret',
    summary:
      'P1: after a cut that empties the text, a host replaceValue with a secret placeholder and an indent (Ctrl+]), the placeholder is no longer marked (its mark goes on the next render) and typing over part of it goes in: «secret 3f2a» becomes «sec#3f2a».',
    cause:
      'crates/config-editor-leptos/src/editor/protect (secrets.rs / follow.rs: the placeholder spans are not followed through the indent after the replace, so neither snap_to_tokens nor the guard knows the placeholder)',
  },
  {
    id: 'BR38-6',
    severity: 'MEDIUM',
    check: 'textarea',
    summary:
      'P4 (timing-dependent): while editing as XML with late echoes (?random), an IME commit ends up in the textarea but not in the document (textarea "日本", text "\\n"), and stays so: what the user sees is not what the editor holds or sends.',
    cause:
      'crates/config-editor-leptos/src/ime.rs / editor/input.rs (composition end vs the Edit-as typing pause and a late echo resetting the textarea): likely, not confirmed',
  },
];

function linesKept(locked, value) {
  const pieces = locked.split('\n');
  if (pieces.at(-1) === '') pieces.pop();
  if (pieces.length === 0) return false;
  const lines = value.split('\n');
  for (let first = 0; first < lines.length; first += 1) {
    if (!lines[first].endsWith(pieces[0])) continue;
    let at = first;
    let ok = true;
    for (let index = 1; index < pieces.length && ok; index += 1) {
      const last = index === pieces.length - 1;
      const found = lines.findIndex((line, row) => row > at && (last ? line.startsWith(pieces[index]) : line === pieces[index]));
      if (found < 0) ok = false;
      else at = found;
    }
    if (ok) return true;
  }
  return false;
}

export function knownClass(failure, page) {
  return (
    KNOWN.find(
      (entry) =>
        entry.check === failure.check && (!entry.pages || entry.pages.includes(page)) && (!entry.match || entry.match(failure, page)),
    ) ?? null
  );
}
