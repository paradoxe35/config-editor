// The failure classes the browser fuzz knows, each a finding to fix: a
// failure matching one is noted and the sequence goes on; any other fails
// the run (FUZZ_BROWSER_STRICT=1 stops on known ones too, to shrink them).
// `severity`: HIGH / MEDIUM for P1, P2, P4 and P5 breaks on common files,
// LOW for edge cases. `match(failure, page)` takes `{ check, detail }`.
// Remove an entry when its finding is fixed (its repro under `repros/`
// then passes: drop the `fixme`).
export const KNOWN = [
];

export function knownClass(failure, page) {
  return (
    KNOWN.find(
      (entry) =>
        entry.check === failure.check && (!entry.pages || entry.pages.includes(page)) && (!entry.match || entry.match(failure, page)),
    ) ?? null
  );
}
