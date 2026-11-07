# Contributing

How the editor is built, and the rules that keep it working. Each rule
exists because breaking it caused a real bug; the tests named with it fail
when it is broken.

## Checks

`make help` lists every command below as a target (`make check`, `make e2e`,
`make fmt`, `make docs-generate`, …).

`scripts/ci.sh` runs every check CI runs (CI calls its steps one by one):
formatting, clippy in every configuration, the tests, each language feature
on its own, every language without schema validation, each optional feature
off, and the generated docs. Run `cargo fmt --all` (and `cd js/config-editor && cargo fmt`)
first, then the checks, before every commit:

```sh
CI_MEMORY_LIMIT=4000000000 CARGO_BUILD_JOBS=4 scripts/ci.sh   # or one step: scripts/ci.sh languages
cd js && npm run build && npm run typecheck                   # the npm packages
trunk build && cd e2e && npx playwright test                  # the end-to-end tests
```

The end-to-end tests run on 4 workers locally (`E2E_WORKERS` to change it)
and on one in CI; the specs that measure time run after the others, alone.
A test that fails only under that load assumes a timing it should not.

`CI_MEMORY_LIMIT` caps the test binaries' memory: a runaway test once ate
all of a machine's memory. A loop that scans text must advance at least one
character on every turn.

A test or helper that needs a built-in language is gated by it
(`#[cfg(all(test, feature = "yaml"))]`, or `any(feature = "yaml", …)`):
`scripts/ci.sh languages` builds and tests each language alone.

## Design rules

### The editor state is one pointer

`EditorState` is `&'static EditorFields` (`src/editor/state.rs`): every
handler and view closure copies it, and an unoptimized (debug) wasm build
keeps each copy on its stack, so a state held inline overflowed the stack
once it grew past about a kilobyte. Add fields to the group of the module
that owns them (`state.doc`, `state.sync`, `state.fold`, …), never inline.
Guarded by `the_state_handlers_copy_is_one_pointer`.

### Every change to the document goes through the edit pipeline

An edit is an `Edit` with an `Origin`; `EditorState::commit`
(`src/editor/edit/`) runs it through prepare (secret tokens, folds), check
(read-only fields), apply, record (undo) and notify (`on_change`, Edit-as
conversion). Never write `doc.text` around it: folding, read-only fields,
secrets, undo and Edit-as all rely on seeing every edit.

### Views that come and go read long-lived state through a relay

Leptos 0.8 never removes a disposed effect or memo from the subscriber list
of a signal it read, nor a disposed owner from its parent: a view that
mounts and unmounts while reading a signal that outlives it leaks a little
every time (virtualized rows leaked over a megabyte per scroll and fold
cycle). So any such view — a popup, a row, a widget, anything shown by a
setting or `.then(|| view! { … })` — reads longer-lived state through
`Relay` or `Fanout`, and lists use `KeyedList` (`src/reactive/`). Content
built once with no reactive reads needs nothing. Guarded by
`e2e/tests/memory.spec.mjs` (including a soak over every surface) and
`a_view_reading_a_long_lived_signal_stays_in_its_subscribers`, which fails
once Leptos prunes disposed subscribers (then the relays can go).

### Language syntax stays behind the language hooks

Shared code never names YAML, JSON, TOML, HCL or XML: what differs between
languages is a `ConfigLanguage` hook with a default (`src/editor/language.rs`),
so a language of your own gets every feature.

### An optional feature is a module with an inert stand-in

`search`, `diff`, `folding` and `protect` are Cargo features (all on by
default). Each is one module (`src/editor/search/`, `changes/`,
`folding/`, `protect/`) holding everything it adds, its state, keys and
components included; the rest of the editor reaches it only through the
hooks that module exports. With the feature off,
`#[cfg_attr(not(feature = "…"), path = "editor/off/….rs")]` compiles
`src/editor/off/<module>.rs` in its place: the same hooks doing nothing.
Shared code calls the hooks and never names the feature; the few `cfg`s
outside are at hook points (a field of `EditorFields`, a language's
`fold_ranges`), never inside a function's logic. What a feature builds on
stays shared (the textarea's projection, `fold_ranges`, the Myers diff).

Without a feature, what it adds to the API is gone (handle methods,
headless functions), except the `ConfigEditor` props (a component's props
cannot be compiled out) and the `EditorSettings` fields (one settings value
fits every build): they stay and do nothing, and their docs name the
feature. `scripts/ci.sh optional` builds and tests each feature off and all
of them off; a test that needs one is gated by it. A new heavy part of the
editor that a host could do without becomes such a feature, and
`scripts/wasm-sizes.sh` prices it.

### One definition per setting

`EditorSettings` (`src/settings.rs`) defines every on/off setting once, with
its doc comment and default. The TypeScript type and the settings tables in
the READMEs are generated from it (`UPDATE_GENERATED=1 cargo test -p
config-editor-js --test settings`); React's props extend that type. Every
setting updates live, and switching one off cleans up after itself.

### The JS and React packages follow the Rust API

`e2e/tests/api-parity.spec.mjs` fails when a public Rust prop, handle method
or exported function has no JS and React counterpart: add it to both, or to
its exemptions with the reason.

### Every built-in text comes from the strings setting

A text the editor writes itself (a label, a tooltip, a notice, a count, an
announcement, a problem it words) is a field of `EditorStrings`
(`src/strings.rs`, the `strings` setting) with its English default and a
doc comment; views and messages read it from there (`state.text(…)`, or a
feature's `texts.rs` that fills its placeholders), never as a literal. A
count is a `…_one` / `…_other` pair; a template takes the placeholders of its
default. Guarded by `views_take_every_text_from_the_strings_setting` (no
text literal in a `view!`) and `every_text_is_shown_somewhere`.

### Saving never breaks the config

Editing as another language converts back to the host's language on save,
keeping its comments and untouched formatting: the host's text changes only
where the user changed it. A value the host's language cannot hold exactly
(a `null` for TOML, a key XML cannot name) is reported as a problem and the
save waits; the editor never writes something the host would read back
differently. Every feature works while editing as another language (its
locks, tokens, folds and diagnostics follow the field there), and
`save_properties.rs` checks random edits in every language pair.

### Light, generic and themable

- A heavy dependency goes behind a Cargo feature (as `schema-validation`
  does), and a change reports its wasm size (gzip of the release
  `config_editor_js.wasm`) before and after.
- Nothing is specific to one host: a feature any host can use, configured
  through props and settings.
- Colors only through theme CSS variables: a `ThemeColor` and its
  `--config-editor-*` variable, documented with the others.

### Everyone can use it

- Every control has a label and is reachable with the keyboard; what changes
  out of sight is announced (`aria-live`). `Tab` indents, so the editor
  keeps a way out (`Ctrl+M`, `Escape` then `Tab`).
- Phone widths (390 and 320 px) never scroll the page sideways.
- Large documents stay fast: `e2e/tests/large-files.spec.mjs` holds typing,
  scrolling, find and fold to their budgets on 6–8k-line documents.

### Code organisation

Each concern has its module, and each part of the screen its component in
`components/`; event handlers are functions, not closures inlined in
`view!`. A file past about 600 lines, or code nested past about six levels,
is split. The project is not yet 1.0: a better design replaces the old one
outright, with no compatibility shims, as long as every feature and test
still works.

### Documentation

Every public feature is documented with an example in the crate's README
and in both JavaScript packages' READMEs.

## Generated docs

- The README contents lists: `node scripts/readme-toc.mjs` after adding or
  renaming a section.
- The screenshots in `docs/screenshots` and their gallery page:
  `cd e2e && npm run screenshots` (after `trunk build --release` and the npm
  build). A UI change reruns the scenes it affects; a new visible feature
  gets a scene in `e2e/screenshots.mjs` and an entry in
  `e2e/screenshots-gallery.mjs`.

Markdown is formatted by Prettier (the version pinned in `js/package.json`,
the same one an editor's Prettier extension runs): `git ls-files '*.md' |
xargs js/node_modules/.bin/prettier --write`. The generators above write its
output, so formatting a file never fights them.

`scripts/ci.sh docs` fails when either is stale, when a file is not
formatted, and when a Markdown table
is broken (`scripts/markdown-tables.mjs`: text written between a table's
rows makes the rest render as one paragraph of `|`s).

## Tests

Every fix comes with a test that fails without it, and fixes the whole
class of the bug rather than the one case reported: a behaviour that differs
by language or by form (block or inline, an XML attribute or element) gets
a test across all five languages and forms (one matrix test that lists
every failing case), or a case in the property tests. End-to-end tests drive the demo and the example
pages with real keys and mouse; a test that depends on timing (undo
grouping, latency budgets) says so, and no test relies on a timing window it
does not test.

## Quality bar

A problem that breaks a config or handles it wrongly (a changed value, a
lost comment or line in YAML, an XML attribute turned into an element, an
HCL block turned into an attribute) is high severity; a lost comment or
format in another language is medium. Releases go out with no known high or
medium problem.

## Commits

One-line subject, no body.
