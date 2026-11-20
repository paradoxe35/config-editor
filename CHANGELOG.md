# Changelog

All notable changes to the crate `config-editor-leptos` and the npm packages
`@pngwasi/config-editor`, `@pngwasi/config-editor-react`,
`@pngwasi/config-editor-vue` and `@pngwasi/config-editor-svelte`. Their
versions move together.

## 0.1.0 — unreleased

The first release: a fast, VS Code–style editor for
configuration files that runs in the browser, written in Rust and compiled to
WebAssembly.

### Languages

- YAML, JSON, TOML, HCL and XML, and your own: in Rust through the
  `ConfigLanguage` trait, or in JavaScript with `registerLanguage` (schema
  suggestions and hover included, from `separators` alone for flat formats
  such as `.env`, from the brackets, item separator and item marker they
  declare for values on one line, or through callbacks for nested ones).
- One JSON Schema drives every language: they all parse into the same data.
  The schema is given as JSON or YAML text.
- The shared code knows no language's characters: brackets, separators,
  quotes, item markers, document markers and markup come from the language
  (`LanguageConfig`'s `map_brackets`, `list_brackets`, `item_separator`,
  `item_marker`, `literal_quotes`, and hooks such as `opens_list_item`,
  `key_markup_len` and `element_end`), so a custom language with syntax of
  its own (`(a | b)` lists, `:=`, `@` items) gets the same exact Edit-as
  saves, list moves and locks as the built-in ones. A JavaScript language
  declares them too: `mapBrackets`, `listBrackets`, `itemSeparator`,
  `itemMarker`, `literalQuotes`, and `writtenSeparator` for how a new entry
  writes its separator (`": "`).

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
- Sticky scroll (`stickyScroll`, `stickyScrollMaxLines`): the lines that open
  the blocks around the top line stay at the top while the text scrolls, in
  every language, and a click goes back to one.
- Fast on large files: only the visible lines are rendered, and typing stays
  responsive in documents of nearly 10,000 lines. A keystroke reads the whole
  document once for each thing that needs it, and the colors of constructs
  over several lines, the fold markers and the change bars just after the
  frame that shows the typed text; a documents view handed its whole list
  after each edit reads only the edited document again.

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
- Secret placeholders are opaque and bound to their place: a token is
  whatever the host's pattern matches, kept whole as written (nothing is
  read from inside it); one the host wrote in a comment is real in that
  text's comments only, and one moved into another field, a field's into a
  comment or a comment's into a field, is plain text there and flagged at
  that spot.
- A host problem with no end (`HostDiagnostic::at_position`; in JavaScript a
  `range` with no `endColumn`) is a point: on a secret placeholder it marks
  the whole token, elsewhere one character.
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

### Behaviour notes

Some things work the way they do on purpose, or because a file format cannot
do more. Here they are in one place.

**Limits of the file formats**

- XML has no types. In an XML view, a value is read as the schema says:
  - where the file holds a string the schema allows, a value you type that
    looks like a number or `true` is saved as a string;
  - where the file holds a number and the schema takes a number or a string
    there, a number you type stays a number; where it takes only text, the
    value is saved as text;
  - a number typed in another spelling (`08`, `2.50`) where the file holds a
    number is saved as that number;
  - a value the schema refuses in every reading keeps the file's type.
- XML writes a list as repeated elements:
  - without a schema that says a field is a list, a list of one item is shown
    as that item (the view says it cannot show it exactly), and the file
    keeps its list when you save;
  - in a file that writes lists this way (XML, HCL blocks, TOML `[[tables]]`),
    a list cut down to one item is written as one item, which reads back as
    a single value. In a file of another language it stays a list;
  - a list you empty in a view of such a file waits to save, because those
    files write no key for an empty list;
  - XML has no list of lists: in an XML view the items of an inner list are
    shown as items of the outer one.
- An empty XML element (`<a/>`) reads as an empty object where the schema
  allows one. An element you empty where the file holds text is saved as empty
  text.
- TOML and XML have no `null`, and XML no empty list. When a required key can
  only take such a value, the fill leaves it out and the problem names it. A
  `null` in a view waits to save in a TOML or XML file.
- In HCL, a block where the schema wants a list is read as that list's one
  item.

**Working with a host**

- After `replaceValue`, a host text equal to the one the host gave before the
  reset is read as a stale render and ignored. To go back to it, call
  `replaceValue` again.
- Without `valueRevision`, when your store still holds an older text after an
  echo, the editor waits 250 ms and then sends the document again. With
  `valueRevision` there is no pause, but a store that loses a text (a failed
  save, a dropped render) must send it again.
- Give `readOnlyRanges` with the text they were worked out for (the `value`,
  an echo, or `rangesFor`); the editor moves them to the document. A range
  whose text was changed before it was locked locks nothing, and the save
  waits until that text is back.
- In a custom language that gives no positions (`spans`), a locked field is
  checked by value, not by place. While the text does not parse, edits go
  through and the save waits until it parses again.

**Decided behaviours**

- A key whose list must have items is inserted as an empty list with the
  caret inside, and the problem shows until you type the items.
- Under `oneOf` and `anyOf`, the fill writes the keys one option alone takes,
  and tries the next option when two share them. When no option can be valid,
  the value is left for you to type.
- A key whose schema needs another key beside it is still offered. The
  problem it adds names the key to write next. A key that would make the
  schemas refuse a value you already wrote beside it is not offered.
- An empty file shows no schema problems, even when the schema asks for keys.
- With a schema no value can meet (`enum: []`, or `allOf` of a boolean and a
  string), the fill writes nothing that passes, and an XML view may save such
  a field in another type. The problem is shown. In an XML view, a `oneOf`
  with an "anything" option (`{}`) may show a problem differently from the
  file.
- A value you did not touch keeps its exact text in the file, even when the
  view reads it as another type (`'8080'` stays quoted).
- A `null` in a view is saved as `null` where the file's language has one
  (YAML, JSON, HCL).
- A save that would drop the comment of an item it keeps waits, with "this
  edit would drop the comment …".
- When an edit leaves a locked value's text alone but the document now reads
  it differently (a duplicate key, a new parent, a commented-out line), the
  edit is kept and the save waits. The reason is shown at the field, and no
  change event or `getValue` carries the changed value.
- Cutting a read-only value is refused.
- A secret placeholder you copy and paste is plain text: it has no chip and is
  saved as its text. It still moves as one piece: the caret steps over it,
  and Backspace or Cut removes it whole. Another document's placeholder pasted
  anywhere is flagged as plain text.
- A syntax message does not quote text of a line that may hold a secret; it
  shows `…` there.
- `Ctrl+/` with a selection comments exactly the selected lines. With only a
  caret, it comments the line, widened to the whole list, string or tag over
  several lines that it is in.
- With a deeply recursive schema, the problem list may end with "The schema
  check stopped early; more problems may follow."
- A block of several lines copied into a map or list that the file writes on
  one line is written on that line, in its one-line form. The values inside
  keep their own forms.
- Typing is fast in files up to about 10,000 lines. In files near 100,000
  lines each key takes about a third of a second, and switching to Edit as
  takes a few seconds.
