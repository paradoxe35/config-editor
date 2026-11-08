# End-to-end tests

Playwright tests that drive the demo editor with real keyboard, mouse, and paste
events in headless Chrome.

```sh
npm install
npx playwright install chromium   # or set CHROME_PATH to a local Chrome
npm run build                     # trunk build (run from the repository root)
npm test
```

`global-setup.mjs` copies the trunk bundle from `../dist` into `.standalone/`, so
the suite runs from `file://` without a web server. Set `CHROME_PATH=/path/to/chrome`
to use a local Chrome instead of Playwright's bundled build.

Screenshots from the run are written to `test-results/screens/`.

## Coverage

- `tests/editor.spec.mjs`: key and value acceptance (mouse, Enter, Escape), bare
  colons, indented new lines, the single highlight, hover documentation, and focus.
- `tests/scenarios.spec.mjs`: a 60-line document, a 900-line scale run, enum and
  initials matching, required-first ordering, dynamic client keys, array items,
  undo/redo, Tab and comment shortcuts, bracket pairing, diagnostics and their
  tooltips, the diff gutter with Apply changes, theme font consistency, and hover on
  nested and dynamic keys.
- `tests/keyboard-a11y.spec.mjs`: leaving the editor with the keyboard (`Ctrl+M`,
  Escape then Tab), the caret's problem as the textarea's description, `F8`
  announcements, read-only reasons and secret hints at the caret, documents.
- `tests/touch.spec.mjs`: a phone (`hasTouch`, `isMobile`): taps place the caret,
  selections made without a click or key are followed, touch drags scroll the
  gutter along, suggestions, the footer and messages work by tap.
- `tests/large-files.spec.mjs`: the demo config generated 6000–8500 lines long
  (`samples/generate.py --scale 5`) in every language: load, keystroke latency
  (worst and 95th percentile), Ctrl+End and the wheel, find, fold all and Edit-as,
  each within a budget; the numbers are logged.
- `tests/strict-csp.spec.mjs`: the Leptos demo, the npm package and the
  React, Vue and Svelte components under a strict Content-Security-Policy
  (pages in `e2e/csp`, served with the header by a small server in the spec,
  `styled: false` with `preset.css` linked): typing, schema and host problems,
  hover, suggestions, find and go to line, folding, the change preview, the
  read-only and secret tips, Edit as, tabs, every theme from the one
  `preset.css` and custom colors, with no refusal.
- `tests/memory.spec.mjs`: memory over many cycles, through the npm package:
  editors mounted and destroyed, a long document scrolled and folded, underlines
  and suggestion lists redrawn, documents opened and closed in `ConfigDocuments`,
  and the React component remounting, and a soak of every interactive surface
  (hover, find, go to line, folding, completions, F8, protected-text tips,
  the baseline notice, every setting, tab focus, host diagnostics, documents
  switched, closed and reopened). WebAssembly memory never shrinks, so each
  test measures its growth after a warm-up. `MEMORY_SOAK_REPORT=1` (or a list
  of surfaces) with an `alloc-stats` build logs each surface's live bytes per
  cycle.

## Hunting leaks

Build the npm package with the `alloc-stats` feature (see
`js/config-editor/src/alloc_stats.rs`) to read the heap from the page:
`__allocStats()` gives the live bytes and allocations, `__allocSizes()` the live
allocations per size (diff two snapshots around repeated work), and
`__allocTrack([sizes])` then `__allocSites()` the stacks that allocated what is
still live. Most leaks so far were views that come and go subscribing to state
that stays; see `crates/config-editor-leptos/src/reactive`.
