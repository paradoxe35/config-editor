# Changelog

All notable changes to the crate `config-editor-leptos` and the npm packages
`@pngwasi/config-editor`, `@pngwasi/config-editor-react`,
`@pngwasi/config-editor-vue` and `@pngwasi/config-editor-svelte`. Their
versions move together.

## 0.2.0 — unreleased

### Schema formats

- Schemas in other formats than JSON Schema written as JSON, compiled into
  JSON Schema by a `SchemaCompiler` wrapped in a `SchemaFormat` (Rust) or a
  compiler registered with `registerSchemaFormat` (JavaScript); the format
  is always named, never guessed. Built in: JSON Schema written in YAML
  (`json-schema-yaml`).
- The files a schema refers to come from the host (`SchemaResources`,
  `schemaResources`; `schemaImports` lists them): nothing is fetched.
- A format may check what JSON Schema cannot hold beside it: a
  `SchemaCheck` (Rust) or a `checks` entry of `compile()`'s result
  (JavaScript) runs each time the editor validates, its problems placed by
  JSON pointer in every view with the schema's.
- What a format checks neither way is a loss (`Schema::losses`,
  `compileSchema`'s `losses`), reported like a schema warning.
- `compileSchema` (and `Schema::json`) gives the JSON Schema, to compile at
  build time and ship plain JSON Schema.

### XML Schema

- XSD 1.0 as a schema format (`SchemaFormat::XSD`, cargo feature `xsd`;
  `schemaFormat: "xsd"` in JavaScript): compiled into JSON Schema in the XML
  mapping, it validates and completes every view of the document. Options
  `root`, `configRoot` and `lax`. Content models (element order, choices,
  substitution groups, occurrences, wildcards' namespaces) and `xs:key`,
  `xs:keyref` and `xs:unique` are checked beside the JSON Schema; what is
  not checked is a reported loss with its line. `xs:include`, `xs:import`
  and `xs:redefine` read their files from the host's resources.
- A JSON Schema's `$ref`s to other files are followed when the host passes
  those files as resources: they are bundled into the schema.

- OpenAPI 3.x and Swagger 2.0 documents as a schema format
  (`SchemaFormat::OPENAPI`, `schemaFormat: "openapi"`): one named schema is
  the document's, 3.0 keywords (`nullable`, boolean `exclusive*`) read as
  2020-12.

### Reading XML by the schema

- With a schema, XML attribute values and element text are read as the type
  the schema gives their place instead of guessed: `1.50`, `+5`, `007` and
  `1e3` are numbers where it wants a number, `true`, `null` and `80` stay
  strings where it wants a string, and the spellings a schema lists for a
  boolean (`"x-true"`, `"x-false"`) are booleans. Saves keep the text as written.
- A custom language whose values are all text gets the same reading
  (`Scalars::Text`; `scalars: "text"` in JavaScript).
- A property the schema marks `"xml": {"attribute": true}` is written as an
  attribute where the text does not write it yet (a conversion to XML, a key
  added in another view), and completion offers attributes in a start tag
  and elements in the content.

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
