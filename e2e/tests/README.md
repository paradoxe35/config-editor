# Where the e2e tests live

The specs are grouped by feature, in one flat folder (the specs find the
repository by their own place, `../..`). A file's name starts with its
feature where it can. Each spec says what it covers in its first comment.
`fuzz/` holds the random browser suite.

Shared code: `helpers.mjs` (the demo editor driver `editor(page)`,
`setupEditor`, pointer helpers, the framework example pages),
`framework-cases.mjs` (the cases every framework component runs) and
`custom-languages.mjs` (languages written in JavaScript).

Timed specs (latency budgets) are listed in `TIMED` in
`../playwright.config.mjs`: they run after the others, one at a time.

## Editing

- `editing-enter`: Enter, Ctrl+Enter, Ctrl+Shift+Enter.
- `editing-tab-indent`: Tab and Shift+Tab.
- `editing-typed-pairs`: brackets, quotes and end tags typed or closed.
- `editing-comments`: Ctrl+/.
- `editing-lines-selection`: whole lines, selections, the caret kept in view.
- `editing-typing`: YAML block scalars with empty lines.
- `editing`, `editor`, `scenarios`, `formatting`, `line-commands`,
  `drag-move`, `undo-typed-pairs`, `ime-composition`, `empty-json-first-key`,
  `long-row`, `row-window`, `touch`.
- Per language: `json`, `json-parity`, `toml`, `hcl`, `xml`, `yaml-aliases`.

## Completion and schema

- `completion-accept`: accepting suggestions, Tab through inserted values.
- `completion-list`: the list itself (Ctrl+Space, keys, wheel, wrapping).
- `completion-window`, `completion-caret-keys`, `suggestions`.
- `schema-assist`, `schema-help`, `schema-warnings`, `yaml-schema`.

## Problems, hover and popups

- `problems-diagnostics`: problems found in documents.
- `problems-tips`: tips, underlines, F8, host problems.
- `problems-display`, `own-problems`, `host-diagnostics`, `problems-timing`
  (timed).
- `hover-cards`, `hover-cards-placement`, `hover-cards-reach`,
  `hover-cards-detours`, `hover-cards-pointer-rule`: the pointer and the
  hover card.
- `hover-keyboard`: Ctrl+K Ctrl+I and the focused card.
- `hover`, `hover-card-text`, `hover-host-replace`.
- `popups-placement`, `popups-follow`, `popups-late-frames`.

## Locks, read-only fields and secrets

- `locks-read-only`: read-only fields and blocks while editing.
- `locks-host-ranges`, `locks-host-ranges-echoes`, `locks-host-ranges-resets`:
  locks the host gives as ranges, across mounts, echoes and resets.
- `locks-typed-entries`, `read-only-fields`, `locks-cleared`, `locks-timing`
  (timed), `edit-as-range-lock`.
- `secrets-placeholders`, `secrets`, `secret-guarantees`.

## Host sync and echoes

- `sync-echoes`: hosts that echo late, in any order, or not at all.
- `sync-resets`: setValue and replaceValue with echoes on their way.
- `echoes-waiting`, `host-call-order`, `host-editing`, `js-update`,
  `destroy-pending`, `unmount`, `binding-errors`, `keystroke-work`.

## Host API and tools

- `host-api`: options, defaults, callbacks, destroy().
- `host-tools`: the headless tools (parse, serialize, rangeOf, pathAt).
- `host-byte-order-mark`: a leading byte order mark.
- `host-sizing`: height 100% and `--config-editor-height`.
- `custom-languages-host`, `custom-language-assist`: languages the host
  registers.
- `path-api`, `pointer-paths`, `reveal-range`, `settings`, `strings`,
  `long-custom-strings`, `host-css-shield`, `strict-csp`, `assorted-demo-host`.

## Edit as and conversion

- `edit-as-host-forms`: the host's comments and forms kept through another view.
- `edit-as-switching`: switching, undo across a switch, values that wait.
- `edit-as-notice`, `edit-as-read-back`, `edit-as-saving`,
  `edit-as-switch-pending`, `edit-as-undo-line-endings`, `conversion`.

## Documents

- `documents-host`: the ConfigDocuments API.
- `documents-tabs`: tabs, their names, keys after the focus moves.
- `documents-saving`: saves of documents shown or never shown.
- `documents`, `documents-dirty-now`, `documents-duplicate-id`,
  `documents-edit-as-text-unshown`, `documents-save-delivered`,
  `documents-screen`, `documents-tab-typing`, `typing-documents` (timed).

## Changes, find and folding

- `changes-and-baseline`, `changes`.
- `find-and-go-to`, `find-host-css`, `go-to-line`, `search` (timed),
  `highlight-geometry`.
- `folding-commands`, `folding` (timed).
- `sticky-scroll`: the header lines kept at the top while scrolling, in every
  language, Edit-as and with folds; the push-up, clicks, the settings, the
  caret kept below them, secret placeholders.

## Accessibility and look

- `a11y-announcements`, `a11y-aria-focus`, `a11y-forced-colors`,
  `a11y-read-only-editor`, `a11y-scrolling`, `a11y-target-sizes`.
- `aria-announce`, `keyboard-a11y`, `demo-chrome-a11y`, `target-size`,
  `zoom-reach`, `forced-colors-chip`, `forced-colors-selection`,
  `focus-in-place`, `focus-kept`, `focus-keeper-cost`, `page-stays`,
  `shortcut-hints`, `ux-focus-reveal-stack`.
- `look-demo-page`, `look-layout`, `look-themes`, `theme-contrast`,
  `preset-style`, `gutter-layout`, `highlight-multiline`.

## Packages and frameworks

- `js-packages`, `js-entries`, `js-api-parity`, `api-parity`,
  `vue-package`, `svelte-package`, `readme-samples`, `frameworks-readmes`.

## Size and speed

- `large-files`, `large-samples`, `insert-text-burst` (all timed), `memory`.
