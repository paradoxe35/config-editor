# Changelog

All notable changes to the crate `config-editor-leptos` and the npm packages
`@pngwasi/config-editor`, `@pngwasi/config-editor-react`,
`@pngwasi/config-editor-vue` and `@pngwasi/config-editor-svelte`. Their
versions move together.

## 0.1.0 — unreleased

The first release: a lightweight and fast, VS Code–style editor for
configuration files that runs in the browser, written in Rust and compiled to
WebAssembly.

### Languages

- YAML, JSON, TOML, HCL and XML, and your own: in Rust through the
  `ConfigLanguage` trait, or in JavaScript with `registerLanguage` (schema
  suggestions and hover included, from `separators` alone for flat formats
  such as `.env`, or through callbacks for nested ones).
- One JSON Schema drives every language: they all parse into the same data.

### Editing

- Schema completion for keys and values, with required child keys and `Tab`
  through empty values; titles of `oneOf`/`anyOf` options.
- Validation as you type, underlined at the exact key or value; hover docs
  (title, description, type, default, examples), also from the keyboard
  (`Ctrl+K Ctrl+I`) and on touch screens.
- Edit as another language: a YAML file edited as TOML (or any pair) is saved
  back in its own language, changing only what was edited and keeping
  comments, quoting, block styles, number forms and line endings.
- The editing you expect from VS Code: auto-closed brackets, quotes and tags,
  smart indentation, comment toggling, find and replace with regular
  expressions, go to line, folding, `F8` through problems, undo and redo.
- Fast on large files: only the visible lines are rendered, and typing stays
  responsive in documents of nearly 10,000 lines.

### For hosts

- Read-only fields and regions that hold even while the text is invalid, and
  secret placeholders that are never shown.
- Problems from the host's own checks (a service, a build step) shown with
  the editor's own, placed by path or range (also while editing as another language). `on_problems` /
  `onProblems` (on `ConfigEditor`, and per document on `ConfigDocuments`)
  hands you the editor's own problems (syntax, schema, secret placeholders,
  conversion; never the host's) when they change, to gate a Save button
  without parsing again.
- A secret placeholder pasted whole stays whole: over another placeholder,
  into an empty value, over a selection cutting into one, and from an
  `insertFromPaste` input that no `beforeinput` announced (a synthetic
  paste), read off the caret the paste left.
- Saving with `Ctrl+S`, an unsaved-edits flag, a diff gutter against a
  baseline with a before/after preview, a change summary and a unified diff,
  and a notice when the file changes outside the editor.
- Several documents as tabs, each with its own language and state; a "+"
  after the tabs (`on_add` / `onAdd`) asks the host for a new file, which
  opens selected with the focus in it, and documents can be kept open
  (`closable: false`: no close button, middle click or `Delete`).
- Each document tells the host what it shows: the language on screen
  (`on_edit_as_change` / `onEditAsChange`, also for a switch that waited and
  for undo across a switch) and whether the text on screen is written back
  to its value (`on_sync_change` / `onSyncChange`, also on `ConfigEditor`),
  each exactly when it changes. `DocumentsHandle::edit_as` /
  `editAs(id, language)` asks for a language again after an undo; a
  document can open from the text the user had on screen
  (`Document::edit_as_text` / `editAsText`), and say why it is read-only
  (`read_only_reason` / `readOnlyReason`, also on `ConfigEditor`: on the
  tab, in the read-only notice and in the accessible descriptions).
- A path API to jump to a field and follow the caret for a breadcrumb, and
  `reveal_range` / `revealRange` to jump to any stretch of text (a comment, a
  line); jumps and the editor's own focus never scroll the host page.
  Paths are JSON pointers as RFC 6901 reads them (each segment verbatim:
  `/ a` and `/01` are keys, only `0` or digits without a leading zero index
  a list), and a path the document does not have (a missing parent, list
  index or leaf) is found nowhere, never at another field.
- Eleven themes, per-token colour overrides, and every built-in text
  replaceable (`strings`) for translations.
- Runs under a strict Content-Security-Policy (`style-src 'self'`, no
  `'unsafe-inline'`): with `styled: false` and the preset served as a file
  (`preset.css` in the crate, `@pngwasi/config-editor/preset.css` in npm),
  nothing inline is needed; inline styles go through the CSSOM and icons are
  inline SVG.
- A headless API (parse, serialize, convert, validate, diff, positions) for
  servers and build steps, in Rust and in `@pngwasi/config-editor/tools`.
- Optional Cargo features (each language, schema validation, search, diff,
  folding, protection) to keep the WebAssembly small. A build links one
  regular-expression engine: `regex` with `schema-validation` (its
  validator needs it), else the smaller `regex-lite`.

### Packages

- `config-editor-leptos`: the Leptos component and the headless API.
- `@pngwasi/config-editor`: plain JavaScript (`ConfigEditor`,
  `ConfigDocuments`, one `update()` for every option), with `/tools` and
  `/binding` entries.
- `@pngwasi/config-editor-react`, `-vue` and `-svelte`: components for each
  framework, built on the same binding.
