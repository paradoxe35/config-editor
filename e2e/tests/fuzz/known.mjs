// The failure classes the browser fuzz knows, each a finding to fix: a
// failure matching one is noted and the sequence goes on; any other fails
// the run (FUZZ_BROWSER_STRICT=1 stops on known ones too, to shrink them).
// `severity`: HIGH / MEDIUM for P1, P2, P4 and P5 breaks on common files,
// LOW for edge cases. `match(failure, page)` takes `{ check, detail }`.
// Remove an entry when its finding is fixed (its repro under `repros/`
// then passes: drop the `fixme`).
export const KNOWN = [
  {
    // P1: a placeholder whose key was split (the text no longer parses)
    // takes a paste inside it: another placeholder pasted at a caret
    // `revealRange` put inside it splits it (`«s«secret 3f2a»ecret 3f2a»`),
    // and the host hears the broken token. Repro: repros/GP38-8-*.
    id: 'GP38-8',
    severity: 'MEDIUM',
    check: 'secret',
    match: (failure) => /«[^»]*«/.test(failure.detail),
  },
  {
    // With a range lock, Enter then a letter on a framework documents page
    // (its host echoes late): undo all takes back the letter, not the line
    // break, and says nothing. Repro: repros/GP38-7-*.
    id: 'GP38-7',
    severity: 'LOW',
    check: 'undo-silent',
    pages: ['documents-react', 'documents-vue', 'documents-svelte'],
  },
  {
    // F8 lands off every problem `onProblems` lists: in an HCL view of a
    // text that does not convert back, a line put above the problem (F8
    // goes to the line above it); after a drag that a lock holds (F8 goes
    // below the problems). Repros: repros/GP38-6-*.
    id: 'GP38-6',
    severity: 'LOW',
    check: 'f8',
  },
  {
    // P4: an edit a lock held (its field read otherwise, `saveHeld` shown)
    // is not released when the host takes the locks away: the notice goes,
    // yet `value` and `onChange` keep the text before it, saying nothing,
    // until the next edit. Repro: repros/GP38-5-*.
    id: 'GP38-5',
    severity: 'MEDIUM',
    check: 'value-text',
    match: (failure) => failure.unlocked,
  },
  {
    // P1: each document's editor knows only the placeholders its own host
    // gave, so one pasted from another document is unflagged plain text
    // there, glued into a value or standing whole in a field (a host that
    // reads tokens across its documents would take the other document's
    // secret). Repros: repros/GP38-4-*.
    id: 'GP38-4',
    severity: 'MEDIUM',
    check: 'secret-cross',
    pages: ['documents', 'documents-react', 'documents-vue', 'documents-svelte'],
  },
];

export function knownClass(failure, page) {
  return (
    KNOWN.find(
      (entry) =>
        entry.check === failure.check && (!entry.pages || entry.pages.includes(page)) && (!entry.match || entry.match(failure, page)),
    ) ?? null
  );
}
