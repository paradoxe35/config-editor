# Changelog

All notable changes to the crate `leptos-config-editor` and the npm packages
`@pngwasi/config-editor`, `@pngwasi/config-editor-react`,
`@pngwasi/config-editor-vue` and `@pngwasi/config-editor-svelte`. Their
versions move together.

## 0.2.4 — 2026-11-23

### Changed

- A narrow editor (under 480px wide, as on a phone or in a narrow column
  of a page; the editor's own width counts, not the window's) has a
  narrower gutter, about 50px instead of 76px, so more of each line
  shows. The markers share a smaller column and the fold chevrons' column
  is as wide as the chevron; each control keeps its 24px hit area, so on
  a line that folds a tap on the line number folds it.

- The tip on protected text under the pointer (`readOnlyRanges`, locked
  fields, secret placeholders) now waits for the pointer to rest
  (`hoverDelay`), as the hover card and the problem tip do: a pass across
  the text no longer flashes it. From one popup to the next it is quick,
  it fades in (not with reduced motion) and goes when the pointer leaves
  the text. A refused edit's tip and `Ctrl+K Ctrl+I` still show at once.

- `F12` and `Shift+F12` that find nothing at the caret now say so there
  for a moment ("No definition for 'listen'", "No uses of 'listen'", "No
  definition here"), to screen readers too, in an editor whose schema
  names keys (`x-config-editor-keys`); there the key no longer reaches the
  browser (Chrome's `F12` opened its developer tools). Elsewhere `F12` is
  left to the browser as before. New strings: `noDefinitionFor`,
  `noDefinitionHere`, `noUsesOf`, `noUsesHere`.

- `Ctrl+Space` with nothing to suggest now also shows "No suggestions" at
  the caret for a moment (it was only said to screen readers), as VS Code
  does. A list emptied by typing still closes without a tip.

- The documents example pages (`?keys`) open a file that is not open as a
  tab when a definition or a use in it is chosen, and go there (they only
  logged it).

### Fixed

- A host's own change (`applyChanges`) made while the save is held (a key
  renamed above a value locked by `readOnlyRanges`) no longer lets the
  held text out: `onChange` heard the renamed text with the host's change,
  and the call returned its revision. The locked values are now read in
  the host's text with its change and the user's edits judged against
  them: the save keeps waiting, `applyChanges` returns `null`, and
  renaming the key back sends the host's change with the user's text. A
  host change rewriting its own locked value keeps the lock over what it
  wrote (it locked nothing before).

- A secret placeholder pasted (or typed to its last character) right
  against other text while the document does not parse (`secret: x`, a
  key written twice) is now refused, as text typed against a placeholder
  already was: it was taken, listed as no problem, and reached the host
  glued (`secret: x«secret 3f2a»`). Pasted alone as a field's value, in
  its quotes or element, it is still taken.

- Accepting a suggested key that adds one more item to a list written by
  repeating it (an XML element, an HCL block, a TOML `[[array]]` table) no
  longer moves the items already written when the list's items have places
  (`prefixItems`, a tuple `items`, also when an `allOf` member or a `$ref`
  gives them): it is offered after those items, where it is written at the
  end of the list, and not in front of them. A suggested XML element
  written above `<a>x</a>` used to push `x` to the next index, where the
  schema refused it.
- Pressing Enter around a value's text in an XML view (`<k3>` then the
  value on the next line) no longer changes the value's host form when
  the schema reads the text as a number there: the line break is layout,
  so a YAML `!!str 5`, a TOML `'5'` or a JSON `"5"` left alone is saved as
  written, not as the number `5`. Text the view reads with the line break
  in it is still saved as the user's.
- A host with revisions that forgets the newest one and takes back an
  older echo now hears the document again: the editor waited for a copy
  of the document it had sent before the newest text came back, which the
  host drops, so the host could keep the older text for good.
- Whether a key that adds one more item to a list written by repeating it
  is suggested no longer depends on how deep the document is: it is judged
  by what the new item changes, so an item already refused deep inside no
  longer hides the key near the top and shows it further down.

## 0.2.3 — 2026-11-23

### Schema formats

- OpenAPI: a `discriminator` picks the schema a value is checked by, in
  OpenAPI 3.0, 3.1 and Swagger 2.0: beside `oneOf`/`anyOf` options by the
  `mapping`'s values or the options' names, and on a base schema (Swagger's
  polymorphism) by the names of the schemas extending it. A valid pet is no
  longer "more than one of the options", an invalid one is caught, and
  completion offers the picked schema's keys. Only an option written in
  place is still a loss.
- OpenAPI: only the named schema and what it refers to are compiled, so a
  schema it never reaches neither fails the document (Swagger's
  `type: file`) nor adds losses. A reached `type: file` is checked as a
  string (a loss); Swagger's `x-nullable: true` allows `null`. The error
  for a document without a version reads "not an OpenAPI or Swagger
  document".
- XSD: the order of an `xs:sequence` holding an `xs:choice` is checked in
  XML text (it was skipped whenever the choice had an option not taken).
  A choice taken twice is one problem, naming the elements.
- XSD: an attribute named like a child element (`<dependency
classifier="x">`) is a problem (new string `xsd_element_as_attribute`),
  and a start tag offers only declared attributes, nothing for an element
  declaring none. A key refused next to an element of its name reads
  `"type"`, not `"@type"`, in XML text.
- XSD: a loss read from several places (a group several types use) is told
  once.
- Resources: a location resolves against a JSON Schema's `$id` when there
  is no location, `./` and backslashes name the same file, and a missing
  external `$ref` names the resource key to pass.

### Find uses

- `findUses()` from a button of the host (the editor's and the documents
  view's handle, every framework component) moves the focus to the text, as
  `Shift+F12` leaves it: the list's arrows, `Enter` and `Escape` work at
  once.
- A failed host search (`onFindUses` throws, rejects or answers no list of
  uses; Rust `FindUses::fail`, new) says "The other files could not be
  searched" (new `usesSearchFailed` string), under the open documents' uses
  or alone, and announces it; it never says "No uses". The console names
  the host's own error, and the React, Vue and Svelte components hand a
  throw or a rejection to `onError`.
- The list starts on the use at the caret (on the definition: the first
  use).
- A row quoting a line that holds a secret (a value the schema marks
  secret, a string a placeholder is glued into) hides it as problems do
  (`…`), in the row and its accessible name.
- Wrapping to the first row (or `Home`) shows its group's title; the active
  row stays visible in forced colours; under `prefers-contrast: more` the
  list gets the strong edge the other popups have. Every popup now draws
  its edge with one variable (`--config-editor-popup-edge`) that more
  contrast sets, so none can miss it.

### Locks

- A `readOnlyRanges` range that touches a key, even part of its name, locks
  that whole field, its key name included: typing in the key is refused at
  once with the read-only reason, as for `readOnlyPaths` (before, each
  keystroke was taken and held the field under a half-typed name). A range
  over a value alone leaves its key free.
- Ranges given with a text of the host's own and nothing naming their text
  (no `rangesFor`, no `valueRevision`), one of which starts or ends inside
  a word, log a console warning naming the range and pointing to
  `rangesFor` / `valueRevision`; they are read as before.
- `Ctrl+Space` on a read-only value or a secret shows why at the caret, as
  a refused edit does, and the request ends there: moving to another line
  opens no list.

### Accessibility

- A second `Ctrl+K Ctrl+I` says "Tab reaches its links." when the card or
  problem tip holds links (new `hoverLinks` string).
- The Edit-as notice's live region is its message alone: its dismiss button
  ("×") is no longer read with it.

### Fixed

- The hover card, go to definition (`F12`) and Find uses work on a key
  name inside an item written on one line (a YAML flow map or list, a TOML
  inline table or array, an HCL or JSON object on one line, XML elements on
  one line) as in block form: all three find the field under the pointer
  or caret with one lookup, by where the parsed document writes each
  member.

### Hosts that echo

- An editor shown again by a conditional render (or in another element)
  with no `echoKey`, whose parent passes each late answer back with its
  `valueRevision`, ends on the newest text the gone editor sent. Before, it
  took the first late answer as the parent's own text, read the newer ones
  as a step back and sent that older text back over the newest.
- An editor that takes over the record of the one that is gone (`echoKey`,
  the same element, a `ConfigDocuments` document listed again with its id,
  a Leptos `echo_key`) shows the newest text that editor sent at once, in
  place of the older value the parent still renders: what is typed next
  goes on from the gone editor's typing, which a late answer no longer
  loses. The record keeps that one text beside the prints.
- `diagnosticsFor` naming a revision no editor sent throws (JS `update`) or
  reaches `onError` (React, Vue, Svelte), as `rangesFor` does; it was a
  console warning.
- An out-of-date list's console warning is logged once per update (it was
  logged twice).

### Host changes

- `applyChanges`'s `revision` also takes the text the changes were made on
  (as `rangesFor` does): a text the editor sent or was given, such as the
  store's text while its echo is still to come, or the document. Left out,
  the indices are read in the text `onChange` gave last (or the host gave
  last), not in a store that has not heard the typing yet. A text the
  editor cannot name throws. Rust: `EditorHandle::apply_changes` takes
  `Option<RangesFor>` (`Some(revision.into())`, `Some(text.into())`).
- `ConfigDocuments.applyChanges` on a document listed again and not shown
  since says so (it said "it was never shown"), and the document editor's
  `editor(id).applyChanges` throws the same instead of returning `null`.

### Docs

- The React, Vue and Svelte props tables list `valueRevision`, `keyNames`,
  `onGoToDefinition`, `onFindUses` and `onGoToUse` (Vue: `ranges-for`,
  `diagnostics-for`, and why `on-find-uses` is a prop while
  `@go-to-definition` and `@go-to-use` are events), and type `onChange` as
  `(value, revision)`. The React Ref section shows `applyChanges`; the
  React snippets and the Svelte "Saving" example type-check as written.

### Known limits

- Text committed by an input method (IME: Chinese, Japanese, Korean) in a
  document of several thousand lines takes longer per insertion the further
  down the caret is: Chrome's own work for an insertion into a large text
  area grows with the lines above it (a plain `<textarea>` shows the same).
  The editor's own work per key stays linear. Ordinary typing is not
  affected.

## 0.2.2 — 2026-11-22

### Find uses

- `Shift+F12` on a key name (a value or `propertyNames` key whose schema
  has `x-config-editor-keys`) or on the key defining it lists the name's
  uses under it, grouped by document (this one, then the other open
  documents of a documents view, then the host's files), each with its
  line: arrows, `Enter` or a click goes there (switching tabs in
  `ConfigDocuments`), `Escape` closes; the count is announced. A use is a
  value (an item, an object's field, an `anyOf` option) or a key of a field
  whose keyword names the same map (and schema, `from` included); free
  text, `examples` words and other maps never count. A name defined nowhere
  still lists its uses; none says "No uses" ("No uses in open files"
  without a host search). Searched on demand only: one walk per open
  document per request, nothing while typing.
- `onFindUses({ schema, pointer, name })` (Rust `on_find_uses` with
  `FindUses::reply`) answers with the uses in files that are not open, at
  once or as a promise; they join the list when they come, a stale answer
  is dropped. `onGoToUse({ source, path, name })` (Rust `on_go_to_use`)
  opens one the user chose. `usesAt(text, language, schema, pointer, name,
of?)` in `@pngwasi/config-editor/tools` (Rust `tools::uses_at`) finds a
  file's uses with the same engine.
- The hover card on a name ends with **Definition · Uses** links (mouse,
  tap and keyboard; each a 24×24 px target, named in full for screen
  readers: "Go to definition (F12)", "Find uses (Shift+F12)"); Definition
  is left out on the definition itself. A key defining a name now shows a
  card for them even when the schema documents nothing there.
- `goToDefinition()` and `findUses()` (Rust `go_to_definition()`,
  `find_uses()`) on the editor and documents handles and every framework
  component act at the caret, for a host's button or menu.
- New `EditorStrings`: `definitionLink`, `definitionLinkLabel`, `usesLink`,
  `usesLinkLabel`, `usesTitle`, `usesOne`, `usesOther`, `noUses`,
  `noUsesInOpenFiles`, `usesSearching`.

### Hover

- The hover card and the problem tip show once the pointer rests on a key
  or an underline (300 ms by default, the new setting `hoverDelay`, Rust
  `hover_delay`), not a fixed time after it came onto it: a move of more
  than a few pixels starts the wait again, so a hand on its way across the
  text opens nothing. The problem tip waits too, instead of showing at
  once; on a documented field the card listing its problems shows
  directly, without the tip before it. While one shows, or within 300 ms after it closed, the next shows
  after 100 ms. `Ctrl+K Ctrl+I`, `F8` and taps still show them at once,
  and a card still stays 350 ms after the pointer leaves its key, so it
  can be reached whatever the delay.
- The card and the tip fade in over 100 ms (none with
  `prefers-reduced-motion`); they take the pointer from the first frame.
- With `prefers-contrast: more`, the popups' borders take their text's
  color and the focus rings on them and on the footer's buttons are
  thicker.

## 0.2.1 — 2026-11-22

### Key names

- `x-config-editor-keys` in an object's `propertyNames` names its keys: a
  new key, or one typed over, is offered the document's own keys at the
  pointer, then the host's names, then the `propertyNames` `enum` and
  `examples`, without the keys the object has. Hover, `F12` and
  `Ctrl`/`Cmd`+click work on such a key as on a value. `propertyNames` is
  found through `$ref`, `allOf`, `anyOf` and `oneOf`, and its `enum` and
  `examples` are offered as keys without the keyword too. The keyword
  never makes a key a problem; the rest of `propertyNames` is still
  checked.

## 0.2.0 — 2026-11-22

### Host lists name their text

- Read-only ranges and host diagnostics name the text their numbers are
  in: `rangesFor` and `diagnosticsFor` (JavaScript, every framework
  component, each `ConfigDocuments` entry) and `read_only_ranges_for`,
  `host_diagnostics_for` (Leptos `ConfigEditor` and `Document`) take a
  revision `onChange` gave (a server check of an older text) or a text
  (the file as loaded). Left out: the `value` they come with, else your
  text now. The editor keeps one record of how each text it sent became
  the document and maps the lists through it exactly; a list given again
  as it is in force is no news.
- `applyChanges(changes, { revision })` (JavaScript, `ConfigDocuments`,
  every framework handle) and `EditorHandle::apply_changes` (Leptos): send
  your change instead of a whole text. Made on the text it names, rebased
  over what the user typed since; locks, folds and the caret follow it
  exactly, and Undo does not undo it.
- `editor.sentRevision` (JavaScript); the constructor takes
  `valueRevision`; `ConfigDocuments` entries take `valueRevision`, and its
  `onChange` gets `(id, value, revision)` and `view.sentRevision(id)` gives
  a document's last one. `DocumentsHandle::sent_revision`
  and `Document::value_revision` in Leptos.
- A framework's `setValue` reports the reset with its revision, and its
  handle's `applyChanges` the result, through `onChange`.
- `echoKey` (JavaScript `ConfigEditor`, every framework component; Leptos
  `ConfigEditor`'s `echo_key`): a name for the place you show a document
  in. Hidden and shown again by a conditional render (`{shown && …}`,
  `v-if`, `{#if}`, `<Show>`), the new editor with the same name takes over
  the texts the one that is gone sent and you have not handed back yet, so
  a late answer for one of them keeps what is typed in it, without
  `valueRevision`. The editor keeps one record per name (a print of each
  text, never the text) from `destroy` until an editor with the name is
  made. The Leptos `ConfigDocuments` hands a document's record on by its
  id, as the JavaScript one does: listed again after a list left it out,
  or re-mounted for a new language or schema.

### Key names

- `x-config-editor-keys` on a string schema: the field names a key of the
  map at a JSON pointer (`"/clients"`, a list, or `{ pointer, from }` with
  other schemas' `$id`s). Its suggestions are the document's own keys
  there, live, then names the host gives, then the schema's `enum` and
  `examples`, each once with its source; a list's item skips names the
  list holds. Never a problem for an unknown name.
- `keyNames` (JavaScript `ConfigEditor`, updatable with `update`) and
  `key_names` (Leptos `ConfigEditor`): names of files that are not open,
  `[{ schema?, pointer, names }]`.
- Hover on a known name says where it is defined (`keyNameDefinedIn`);
  `F12` and `Ctrl`/`Cmd`+click go to its key, or call `onGoToDefinition`
  (Leptos `on_go_to_definition`) with `{ schema, pointer, name, source }`.
- `keysAt(text, language, pointer)` in `@pngwasi/config-editor/tools` and
  `leptos_config_editor::tools::keys_at`: a file's keys at a pointer.

### Theming the chrome

- Added: the tabs, the toolbar and the footer take their fonts, spacing,
  borders and radius from `--config-editor-*` variables, set with the new
  `chrome` setting (`ThemeChrome` and `ChromeToken` in Rust, `chrome` in
  JavaScript with the `ThemeChromeToken` type) or plain host CSS, and new
  `colors` tokens (`toolbar-bg`, `toolbar-border`, `footer-bg`,
  `footer-border`, `footer-text`, `tab-close-bg`, `tab-close-hover-bg`,
  `tab-add-*`). Their layout stays protected from a page's resets
  (Tailwind's preflight, modern-normalize), and the hover effects are in
  `@layer config-editor.chrome`, which a host's own rule overrides. With
  none set, the chrome looks as before.

### Changed

- **Breaking (Rust):** `Document::read_only_ranges_version` is removed:
  give `read_only_ranges_for` (`RangesFor::Text(current)` or the
  document's `value_revision`) instead.
- A list naming a revision the editor never sent is refused: `update`
  throws (a framework component reports it to `onError`), nothing changes.
  One naming a revision the editor can no longer name (it keeps the 64
  newest texts it sent, none from before a text of your own, and none an
  editor a hide and show replaced sent) is out of date:
  a console warning, the locks stay.
- Ranges that do not fit the text they name throw (they were read in the
  text the last ones came with before): give `rangesFor`.
- A text of your own with the same ranges (no `rangesFor`) reads them in
  that text, exactly: no whole-line locks around them any more.
- A text of your own with no ranges (`replaceValue`, `update({ value })`)
  carries the locks into it by the safe diff (whole changed lines where it
  cannot tell), until you give ranges again (the same list rendered again
  with numbers past the new text's end is no news); while editing as
  another language too (they no longer stay at their numbers).
- An Edit-as switch, and the switch back, are faster on large files: on
  10,000-line files they take 14–24% less time than in 0.1.1 in every
  language (switch 379–404 ms against 439–510 ms, back 289–397 ms against
  345–468 ms, the demo's release build). The switch no longer parses the
  host's text again, and each check of a text finds the secret
  placeholders written in it in one pass, not one pass for each.
- An open editor keeps one copy of its text in memory, not 16: the
  document, the host's text, the value `value` answers with, the textarea's
  value, the baseline, the locks and the parses kept share it. Typed in, it
  keeps two (the text and the baseline it is compared with), not 19. On a
  400 KB file that is about 0.8 MB of wasm memory instead of 7.6 MB.
  Editing as another language keeps one copy of each of the two texts.

### Syntax messages say what they quote

- `ParseError` has a new field, `quotes`: the parts of its message that
  quote the text (`language::Quote`: a range of the message and the text
  range it quotes). Where a secret may be written, the editor hides exactly
  those parts and shows the rest of the message as it is. Every built-in
  language gives them. A custom language sets `quotes: None` (or uses
  `ParseError::new` for a message quoting nothing); without quotes the
  editor finds them by reading the text again with its letters changed, as
  before.

### Schema formats

- Schemas in other formats than JSON Schema, compiled into JSON Schema by a
  `SchemaCompiler` wrapped in a `SchemaFormat` (Rust) or a compiler
  registered with `registerSchemaFormat` (JavaScript); the format is always
  named, never guessed. Built in: OpenAPI and XSD; `json-schema` takes JSON
  or YAML text.
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

- XSD 1.0 as a schema format (`SchemaFormat::XSD`, the opt-in cargo
  feature `xsd`; `schemaFormat: "xsd"` in JavaScript, in a package built
  with `npm run build:xsd`: the published one leaves it out, as it adds
  about 71 KB gzip): compiled into JSON Schema in the XML
  mapping, it validates and completes every view of the document. Options
  `root`, `configRoot` and `lax`. Content models (element order, choices,
  substitution groups, occurrences, wildcards' namespaces) and `xs:key`,
  `xs:keyref` and `xs:unique` are checked beside the JSON Schema, their
  problems worded by the `xsd_*` texts of `EditorStrings` and hiding the
  secrets they quote; what is not checked is a reported loss with its
  line. `xs:include`, `xs:import` and `xs:redefine` read their files from
  the host's resources.
- A JSON Schema's `$ref`s to other files are followed when the host passes
  those files as resources: they are bundled into the schema.

- OpenAPI 3.x and Swagger 2.0 documents as a schema format
  (`SchemaFormat::OPENAPI`, `schemaFormat: "openapi"`): one named schema is
  the document's, 3.0 keywords (`nullable`, boolean `exclusive*`) read as
  2020-12.

### Reading XML by the schema

- The spellings a schema lists for a boolean (`"x-true"`, `"x-false"`, as XSD
  has `1` and `0`) are read as booleans, and `"x-whitespace"` (replace,
  collapse) is applied to the value that is checked. Saves keep the text as
  written.
- A custom language whose values are all text gets the same reading
  (`LanguageTraits::scalars_are_text`; `scalars: "text"` in JavaScript).
- A property the schema marks `"xml": {"attribute": true}` is written as an
  attribute where the text does not write it yet (a conversion to XML, a key
  added in another view), and completion offers attributes in a start tag
  and elements in the content. A required one is filled as an attribute
  too, before the other required children (`RequiredChild::attribute`,
  for a custom language's `write_insert`).
- A list key written by repeating its element is suggested again when the
  items written read as valid by the schema (an XSD boolean written `1`).

### Key names across documents

- `ConfigDocuments` (JavaScript, every framework component, Leptos)
  offers a field marked `x-config-editor-keys` the keys of every other
  open document of the schemas it names, live: a key typed in one tab,
  unsaved, is suggested in another at once. The same schema is the same
  `$id`, else the same schema text; `from` names other schemas by `$id`.
  A keystroke reads again only the document typed in; a document that
  does not parse keeps its last good names; a closed one's names go.
- `keyNames` on the view and on each document (`key_names` on the Leptos
  `ConfigDocuments` and `Document::key_names`): the host's names for the
  files that are not open. A name whose `source` is an open document's id
  is replaced by that document's live names, so a host gives every file's
  names once.
- Go to definition of a name another open document defines shows that
  document with the caret on its key; one no open document defines calls
  `onGoToDefinition({ schema, pointer, name, source })` (Leptos
  `on_go_to_definition`).
- `keyNames` and `onGoToDefinition` props on the React, Vue (`:key-names`,
  `@go-to-definition`) and Svelte `ConfigEditor` and `ConfigDocuments`.

### Fixes

- A React, Vue or Svelte component whose mount failed on a prop the editor
  rejects (a `hostDiagnostics` or `readOnlyRanges` list naming places its
  text lacks, the documents of `ConfigDocuments`) now mounts again once
  any prop changes, so a fixed list brings the editor back. Before, only a
  changed setting or `editAs` tried again, and the editor stayed gone.
- An Edit-as save no longer guesses how the host's reader will read what
  it writes: it reads the text it would write once, with the host's own
  reader under the schema, and compares it value by value with what the
  view reads. A value may differ only where the user left it as the view
  showed it, or where XML writes two values alike (one written list:
  a list of one as its item, `8` and `"8"`, a number typed in another
  spelling, an emptied element, nested lists shown flat, a TOML
  datetime in its own form). Any other difference makes the save wait
  and say why ("cannot keep this value exactly"). So a save that would
  read otherwise now waits, for example two `<d>0</d>` elements the view
  reads as numbers where the schema wants strings. And a value left alone
  inside a list of one the schema reads as a list (a YAML `!!str 3` under
  `containers`), through a line copied in an XML view and a switch to
  TOML, now keeps its host form instead of being saved as the number.
- A host with revisions that goes back to a text it had moved past (it
  forgot the newest revision with a text of its own, then took a late
  answer) no longer stays there: a `valueRevision` confirmed already, or
  given before a text of the host's own replaced the document, is a known
  fact that the host is behind, so `onChange` gives it the document again
  with a new revision (at once and, if it is still behind after the
  250 ms pause, once more; nothing more until it takes the document and
  goes back again), and the host catches up. Such a value never replaces
  the document. Before, the editor kept the document and the host the
  older text, or a late answer from before the host's own text replaced
  that text.
- An Edit-as save after typing does the work of what was typed, not of
  every item of the document: an XML host with attributes no longer pairs
  each item with its link or checks each item and element's markup again
  when only a value's text changed (a 10 000-line XML host edited as JSON,
  measured natively with no parses kept between steps, saves a typed value
  in about 1.1 s under its schema, from 2.2 s, and in 0.6 s without one,
  from 1.5 s; the next line gives a mounted editor's times).
- A typed Edit-as save over an XML host walks the schema for the values
  it writes as attributes only where the document holds new values, and
  reads the host's markup only up to its first element holding some; a
  held secret placeholder's tokens are looked for in one pass over the
  text, not one pass each. The same 10 000-line XML host edited as JSON,
  in a mounted editor (which keeps its last parses, as in the browser),
  saves a typed value natively in about 0.47 s under its
  schema, from 0.57 s, and in 0.43 s without one, from 0.46 s.
- In an XML view under a schema, a copy of a list item pasted into the
  list the host already holds is a new item: it saves as the view reads
  it where the schema takes only that reading (a `<d>0</d>` copied where
  the schema wants a string saves `"0"`), and keeps its source's type
  elsewhere. The item it was copied from keeps the host's.
- A copied list item stays a copy across an Edit-as switch made while
  its save waits: switched to a TOML view (which writes the list on one
  line) and back to XML, it still saves as the schema reads it, not as
  the item it was copied from.
- With `echoes`, a reset to the text the document already holds
  (`replaceValue` or `setValue` with the same text, which reports nothing)
  no longer makes the editor take the host for holding it: a host left on
  an older text by a late answer still hears the document once the pause
  is over, so the two agree again.
- With `valueRevision`, an old revision given back after a newer one came
  (an old render) is the text the host gave last: the host's own text
  given next is taken as news, not as that text given again.
- Hidden and shown again with `valueRevision`, a late answer for a text
  the editor that is gone sent keeps what is typed in the new editor when
  it is older than a text the new editor sent (that text still comes);
  it brought that typing back over the new typing before. Without
  `valueRevision` the new editor cannot tell such an answer from your own
  text ("Hosts that echo" in the README says what to do).
- Hidden and shown again without `valueRevision`, a late answer for a text
  the editor that is gone sent no longer replaces what is typed in the new
  editor, where the editor knows it shows the same document again: an
  editor made in the element one was destroyed in (a framework component
  unmounted and mounted again, as React's `<Activity>` does), and a
  `ConfigDocuments` document listed again with its id. That editor takes
  over the texts still on their way; such an answer taking the place of
  the same text the new editor sent made the new one's own answer read as
  your text. An editor made anywhere else (a component a conditional
  render made new) needs `echoKey` or `valueRevision`.
- In XML, a second element of a secret's name (an empty one too) written
  beside a lone secret's element is taken and held, typed or pasted, as a
  list made beside a placeholder is in every language; typing it no longer
  stops at its last `>`.
- A save waiting because the host cannot hold the top level (a list or a
  scalar where TOML, HCL or XML need an object) says what to do, and a
  null TOML has no value for is worded as XML's.
- An XML Edit-as view's save tells the values the user wrote from the
  editor's record of the view's edits, not by comparing the values with
  the ones the view showed. A text the editor has no record of (an opening
  `value` given in place of the view) waits, saying why, where only that
  record could decide a value's type. Switching from an XML view with
  unsaved edits to another view writes each value the user left as the
  host holds it (a copied `edition = "2021"` stays the string `"2021"`).
- In an XML view under a schema: a host string typed in another spelling
  the schema reads alike (`8` over `"08"` where it reads numbers) saves as
  typed instead of keeping the old text; and a part of the schema the
  document fails whichever way a value is read no longer hides the part
  that takes only the view's reading.
- In an XML view under a schema, a host scalar written as several elements
  (its element typed again or pasted) saves each item as the view reads
  it where the schema takes only that reading (two `<d>0</d>` over the
  host's `d: 0` where it wants strings save `["0", "0"]`, not `[0, 0]`).
- Switching from an XML view of nested lists with unsaved edits keeps each
  value left alone: the view's flat list is paired with the host's values
  in order, not by index (which wrote another value, or none, in its
  place).
- An empty XML element reads as `null` where the schema refuses the empty
  text and an option takes `null` (`anyOf: [{type: null}, {enum:
[auto]}]`), instead of the empty text it then marks as a problem.
- An Edit-as view (XML, HCL) shows the host's schema problems where it
  shows a list of one as its item, which the save keeps as a list: a
  problem only the list has (an option wanting it) shows in the view too.
- A secret's field is named by its language's reading of indexes: only
  XML and HCL, which write a list by repeating a key, read `/l/0` and
  `/l` as the same field.
- A required list filled with the items its `minItems` asks for writes
  each item as a single value of its schema is written: an object item's
  required children in the same order, a required XML attribute in the
  item's start tag (it was written as a child element). A custom
  language's `write_insert` gets such a list as `Fill::List`, its value
  `Fill::value`.
- A required list of text a `pattern` asks for (`^x`, `^[0-9]{2}-[A-F]+$`)
  is filled with the items its `minItems` asks for, each a text read off
  the pattern, where it was written empty when no plain letters or digits
  met the pattern (with `schema-validation`, whose regex parser this uses).
- A required child whose property says nothing of its shape, while an
  `allOf` member beside it makes it a list or an object, is filled as that
  list (with its `minItems`) or object (with its required children); it
  was left to type, and XML wrote one empty element.
- An edit lines away from a secret's placeholder that makes the list or
  map holding it one string (`=` typed into a YAML item's `- `, or ` |`
  typed after the parent's key) is refused, or held with the save
  waiting, in every view; the host heard the placeholder glued to the
  other items' text before.
- A value left as written in an XML view, inside an element the
  schema reads as a list of one (`<k3>3</k3>` where `k3` is a list),
  saves and switches views as that list (`"k3": [3]`); it was saved as
  the lone value, which the schema then marked as a problem the view did
  not show, and a host string inside such an element lost its type.
- A value typed in an XML view over a host string, where the schema
  reads the element as a list of one list (`<d>null</d>` read as
  `[[null]]`), saves as the view reads it; the save kept the host's
  string before.
- A number typed in another view of an XML host, where the schema reads
  that text as a string (`3` would match two options of a `oneOf`),
  is saved in a spelling the host reads as the number, or the save
  waits; an HCL view saved the text `3`, which the host read as the
  string `"3"`.
- A YAML merge (`<<: *common`) or alias whose map an Edit-as edit parts
  from its anchor is written out with each of its scalars as the host
  wrote it (`driver: "json-file"`), and each map or list in it in the
  form its anchor writes it: a block on lines of its own, a flow as its
  line holds it. The quotes were dropped, a block map was written on one
  line (`logging: {driver: json-file}`), and a value left alone in the
  anchor that the same save moved values next to lost its quotes too
  (`driver: "json-file"` saved as `driver: json-file` from a TOML view).
- A value an Edit-as edit leaves alone keeps the form the last save wrote
  it in when an undo takes the view back past that save: a map a TOML
  dotted key made (`n.ode = ">=18"`), which the save had put on lines of
  its own in a JSON host, was written on one line by the next save of a
  line deleted elsewhere.

## 0.1.2 — 2026-11-21

### Fixes

- An edit far from a secret's placeholder that turns the list or map
  holding it into one string (`=` typed into a YAML item's `- `, or ` |`
  typed after the parent's key) is now refused, or held with the save
  waiting. Before, the host got the placeholder glued to the other text,
  which overwrote the real secret.

## 0.1.1 — 2026-11-21

- The Rust crate is renamed from `config-editor-leptos` to
  `leptos-config-editor`, following the `leptos-*` naming of the Leptos
  ecosystem. Change the dependency to `leptos-config-editor = "0.1"` and the
  imports from `config_editor_leptos::` to `leptos_config_editor::`. Nothing
  else changes.
- The npm packages move to 0.1.1 with the same code, so all versions stay
  together.

## 0.1.0 — 2026-11-21

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
- Line numbers select whole lines, as in VS Code: a click selects its line, a
  drag over the numbers the lines it passes, and `Shift`+click extends the
  selection to the line clicked.
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

- `leptos-config-editor`: the Leptos component and the headless API.
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
- Reset a framework component one way at a time: with its `setValue`, or by
  setting your state, not both. `setValue` reports the text through
  `onChange`, so with both your state lists it twice, and without
  `valueRevision` the late echo of that report can undo the next typing.
  With `valueRevision` the editor tells them apart exactly.
- Without `valueRevision`, when your store still holds an older text after an
  echo, the editor waits 250 ms and then sends the document again. With
  `valueRevision` there is no pause, but a store that loses a text (a failed
  save, a dropped render) must send it again.
- Give `readOnlyRanges` with the text they were worked out for (the `value`,
  an echo, or `rangesFor`); the editor moves them to the document. A range
  whose text was changed before it was locked locks nothing, and the save
  waits until that text is back; so does a field named in `readOnlyPaths`
  after it was changed.
- `readOnlyRanges` name places in the text they come with. When your app
  sends a new text of its own with the same ranges, the editor keeps the
  locks where the edits moved them, and may lock the whole changed lines
  around them until you send new ranges. Work the ranges out again for a new
  text, or use `readOnlyPaths`, which do not depend on places in the text.
  Hiding and showing the editor (an unmount) keeps the locks only through
  the ranges your app gives for its current text.
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
