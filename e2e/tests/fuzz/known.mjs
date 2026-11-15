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
    // GP38-8 seen on the page: the split placeholder shows as written.
    id: 'GP38-8',
    severity: 'MEDIUM',
    check: 'secret-dom',
    match: (failure) => /«[^»]*«/.test(failure.detail),
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
];

export function knownClass(failure, page) {
  return (
    KNOWN.find(
      (entry) =>
        entry.check === failure.check && (!entry.pages || entry.pages.includes(page)) && (!entry.match || entry.match(failure, page)),
    ) ?? null
  );
}
