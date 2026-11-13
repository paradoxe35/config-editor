# Config Editor for JavaScript

A fast, schema-aware config editor for the browser: YAML, JSON, TOML, HCL and XML, or a language of your own,
with completion from a JSON Schema, hover docs, validation, a diff gutter,
editing one language as another, and eleven themes. It is written in Rust
([`config-editor-leptos`](https://crates.io/crates/config-editor-leptos))
and ships as WebAssembly; this package is its framework-free JavaScript API.

It has three entries:

| Entry                            | For                                                                                                                                                                |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `@pngwasi/config-editor`         | An app: `loadConfigEditor`, `ConfigEditor`, `ConfigDocuments`, and the language and theme catalogs (`languages`, `themes`, `languageForFile`, `registerLanguage`). |
| `@pngwasi/config-editor/tools`   | Text without an editor (servers, build steps, review screens): validation, conversion, parsing, diffs, paths, folds; see [Tools](#pngwasiconfig-editortools).      |
| `@pngwasi/config-editor/binding` | Writing a package for another framework; see [Binding](#pngwasiconfig-editorbinding).                                                                              |

For React, Vue and Svelte, use [`@pngwasi/config-editor-react`](https://www.npmjs.com/package/@pngwasi/config-editor-react),
[`@pngwasi/config-editor-vue`](https://www.npmjs.com/package/@pngwasi/config-editor-vue)
or [`@pngwasi/config-editor-svelte`](https://www.npmjs.com/package/@pngwasi/config-editor-svelte),
which build on this package.

<!-- toc -->

## Contents

- [Install](#install)
- [Quick start](#quick-start)
  - [Loading the WebAssembly](#loading-the-webassembly)
- [`new ConfigEditor(element, options)`](#new-configeditorelement-options)
  - [Settings](#settings)
  - [Built-in texts](#built-in-texts)
  - [`update(options)`](#updateoptions)
  - [Instance members](#instance-members)
  - [Host diagnostics](#host-diagnostics)
  - [Read-only fields](#read-only-fields)
  - [Secrets](#secrets)
  - [Paths](#paths)
- [Languages and themes](#languages-and-themes)
  - [XML mapping](#xml-mapping)
- [`@pngwasi/config-editor/tools`](#pngwasiconfig-editortools)
- [`@pngwasi/config-editor/binding`](#pngwasiconfig-editorbinding)
- [Keyboard](#keyboard)
  - [Leaving the editor and screen readers](#leaving-the-editor-and-screen-readers)
  - [Folding](#folding)
- [Saving and unsaved edits](#saving-and-unsaved-edits)
- [Several documents](#several-documents)
- [Reviewing changes](#reviewing-changes)
- [Custom languages](#custom-languages)
  - [Schema completion and hover](#schema-completion-and-hover)
  - [Strings and comments over lines](#strings-and-comments-over-lines)
  - [Edit-as with a custom language](#edit-as-with-a-custom-language)
- [Themes](#themes)
- [Styling](#styling)
  - [Size](#size)
  - [Strict Content-Security-Policy](#strict-content-security-policy)
- [License](#license)
<!-- /toc -->

## Install

```sh
npm install @pngwasi/config-editor
```

Browsers only (ES modules + WebAssembly). The `.wasm` file is about 2.3 MB
gzipped (1.5 MB with brotli) and loads once per page. It is built with every Cargo feature of
the Rust crate (all five languages, schema validation, find and replace,
the diff, folding, read-only fields and secrets); a Rust app can leave out
what it does not use ([Cargo features](https://crates.io/crates/config-editor-leptos#cargo-features)).

## Quick start

```ts
import { ConfigEditor, loadConfigEditor } from "@pngwasi/config-editor";

await loadConfigEditor();

const editor = new ConfigEditor(document.getElementById("config")!, {
  language: "yaml",
  value: "listen: 7835\n",
  schema: await (await fetch("/schema.json")).text(),
  showToolbar: true,
  showFooter: true,
  onChange: (text) => save(text),
});
```

### Loading the WebAssembly

`loadConfigEditor()` loads the WebAssembly module, once for the whole page
however many callers ask (the framework components call it themselves).
Await it before creating an editor or calling a [tool](#pngwasiconfig-editortools).
It fetches `config_editor_bg.wasm` next to the package's JavaScript, which
works with bundlers that copy it (Vite, webpack 5, Parcel). When yours does
not, pass its URL:

```js
import { loadConfigEditor } from "@pngwasi/config-editor";

// Vite
import wasmUrl from "@pngwasi/config-editor/config_editor_bg.wasm?url";
await loadConfigEditor({ module_or_path: wasmUrl });

// Any server: copy node_modules/@pngwasi/config-editor/pkg/config_editor_bg.wasm
await loadConfigEditor({ module_or_path: "/assets/config_editor_bg.wasm" });
```

A failed load is not kept: calling it again (with the right URL) retries,
and framework components whose own load failed mount then.

## `new ConfigEditor(element, options)`

Mounts an editor into `element` (an `HTMLElement`; its previous content is
replaced, editors included). Options are type-checked: a value of the wrong type
throws, and an unknown option name is reported in the console. They are the
document and its callbacks (below) and every [setting](#settings) (`theme`,
`showFooter`, `folding`, …).

| Option               | Type                                        | Default          | Purpose                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| -------------------- | ------------------------------------------- | ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `language`           | `string`                                    | `"yaml"`         | Language of the document: `"yaml"`, `"json"`, `"toml"`, `"hcl"`, `"xml"` (any case, or a file extension such as `"yml"` or `"tf"`), or a registered one. `value`, `baseline` and `onChange` are always in it.                                                                                                                                                                                                                                                                                                                                                                                                  |
| `value`              | `string`                                    | `""`             | The document.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `editAs`             | `string`                                    | —                | Show and edit the document in another language; it is saved back in `language`, keeping its comments.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `schema`             | `string`                                    | —                | JSON Schema as JSON text: completion, hover docs and validation.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `baseline`           | `string`                                    | `value`          | Text the diff gutter compares against: the version last saved or loaded. See [Saving](#saving-and-unsaved-edits).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `readOnly`           | `boolean`                                   | `false`          | View mode: navigation, selection, diagnostics and hover keep working.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `readOnlyReason`     | `string`                                    | —                | Why the editor is read-only, while `readOnly` is on: the "Cannot edit in a read-only editor" notice says it when the user tries to edit, and so does the text area's accessible description. `update` changes it (`null`: no reason).                                                                                                                                                                                                                                                                                                                                                                          |
| `fileName`           | `string`                                    | `untitled.<ext>` | Shown in the toolbar.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `className`          | `string`                                    | —                | Extra class on the editor root.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `echoes`             | `boolean`                                   | `false`          | Set it when you hand every `onChange` text back through `update({ value })`, possibly late and in any order; see [Hosts that echo](#hosts-that-echo). Fixed for the editor's life.                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `onChange`           | `(value: string) => void`                   | —                | Called with the document after every edit (after each typing pause while editing as another language).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `onEditAsChange`     | `(language: string) => void`                | —                | Called with the language on screen: once on mount, then each time an `editAs` switch completes and when undo or redo steps back across one (a switch is an undo step). A switch waits while the document cannot convert yet (TOML has no `null`): until then the host's language stays on screen, edited and sent to `onChange` as it is, with the reason on the value in the way. Label the language shown from this, not from `editAs`.                                                                                                                                                                      |
| `onSyncChange`       | `(inSync: boolean) => void`                 | —                | Called with whether the text on screen is written back to the document: once on mount, then each time that changes. `false` while an `editAs` edit cannot convert back (a `null` for TOML, or a value `language` would read back differently, such as a list typed over an XML string; `value` stays the last document, `onChange` waits and `save()` sends nothing; a list the document held, left with one item or none, is written as `language` writes it), whatever brought it there (typing, paste, undo, a switch, a reload, a new `value`); read after each typing pause. Always `true` in `language`. |
| `onChangeSummary`    | `(summary: ChangeSummary) => void`          | —                | Called with `{ added, removed, modified, hunks }`: the lines changed against `baseline` and the number of separate changes, on mount and whenever it changes.                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `onProblems`         | `(problems: Diagnostic[]) => void`          | —                | Called with the editor's own problems (syntax, schema, secret placeholders, values Edit-as cannot convert; `origin: "editor"`), never `hostDiagnostics`: once on mount, then each time they change after a pause in typing, not while they stay the same. With `showDiagnostics` off it finds none. Gate a Save button on it instead of calling `validate` again.                                                                                                                                                                                                                                              |
| `hostDiagnostics`    | `HostDiagnostic[]`                          | —                | Problems the host found (its own checks: a service, a build step), shown with the editor's own; see [Host diagnostics](#host-diagnostics).                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `onPathChange`       | `(path: EditorPath) => void`                | —                | Called with the path of what the caret is on each time it changes (and once on mount), for a breadcrumb; see [Paths](#paths). The path is computed only while this is set.                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `onSchemaWarning`    | `(message: string) => void`                 | console warning  | Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile (keys it matches get no completions or hover docs). Called a microtask after `new ConfigEditor` returns, never inside it, so the instance is yours by then.                                                                                                                                                                                                                                                                                                                             |
| `readOnlyPaths`      | `FieldPath[]`                               | —                | Fields the user cannot change (each with everything inside it), on top of the schema's `readOnly` ones; see [Read-only fields](#read-only-fields).                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `readOnlyRanges`     | `TextRangeInput[]`                          | —                | Stretches of the document (in `language`) the user cannot change: `{ start, end }` string indices or one-based `{ startLine, startColumn, endLine?, endColumn? }`.                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `secrets`            | `SecretOptions`                             | defaults         | `{ placeholder?, isPlaceholder?, hint?, referencePrefixes? }`: how secret placeholders are matched, described and completed; see [Secrets](#secrets).                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `onSave`             | `(value: string, language: string) => void` | —                | Called on `Ctrl+S` / `Cmd+S` (the browser's dialog prevented) and `save()`, with the document in `language`. Nothing saves by itself. Without it, `Ctrl+S` is the browser's.                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `onDirtyChange`      | `(dirty: boolean) => void`                  | —                | Called with whether the document differs from `baseline`: once on mount, then each time that changes.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `onBaselineConflict` | `(event: BaselineConflict) => void`         | —                | Called when a new `baseline` arrives under unsaved edits (`"opened"`) and when that is settled (`"reloaded"`, `"kept-mine"`, `"settled"`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

An unknown language, theme or color token, or an invalid schema, throws an
`Error` saying which.

`value`, `baseline`, `editAs`, `readOnly`, `hostDiagnostics`, the read-only
fields, the callbacks and every setting change live through
[`update`](#updateoptions). The other options (`language`, `schema`,
`fileName`, `className`, `secrets`, `echoes`) are what the editor is built from:
create a new one to change them (the framework components do it for you).

### Settings

The settings are the theme, the colors and every on/off switch: the
`EditorSettings` type, generated from the Rust crate's struct, each with a
default. Give them with the other options, and switch any of them live with
`update`: the caret, the undo history and the folds stay. `update` merges
them key by key (a setting left out stays as it is, one set to `null`
returns to its default); `settings` returns them, every key set.

```ts
// A plain viewer: no colors, no diff, no suggestions, no popups.
const viewer = new ConfigEditor(element, {
  value: text,
  readOnly: true,
  highlighting: false,
  showDiff: false,
  completions: false,
  hoverDocs: false,
  showDiagnostics: false,
});

// Later: one setting changed, the rest kept.
viewer.update({ showDiagnostics: true });
```

<!-- settings:start -->

<!-- Generated from crates/config-editor-leptos/src/settings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`. -->

| Setting                  | Default             |                                                                                                                                                                                                                                                                                    |
| ------------------------ | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `theme`                  | `"vscode-dark"`     | The bundled color theme.                                                                                                                                                                                                                                                           |
| `colors`                 | none                | Colors overriding the theme's, by token (`bg`, `key`, `string`, `selection`, …), for a custom palette.                                                                                                                                                                             |
| `styled`                 | `true`              | Inject the preset stylesheet. Off, serve it as a file (`preset.css` in the crate and the npm package, as a strict Content-Security-Policy needs) or style the `config-editor__*` markup yourself (the preset is `PRESET_CSS`, `presetCss()` in JavaScript).                        |
| `showToolbar`            | `false`             | Show the built-in toolbar: the file name, its language and the schema's state.                                                                                                                                                                                                     |
| `showFooter`             | `false`             | Show the built-in status footer: the suggestions hint, problem and change counts, and the line count.                                                                                                                                                                              |
| `showDiagnostics`        | `true`              | Show syntax and schema problems inline, in the footer and on hover (`F8` moves between them).                                                                                                                                                                                      |
| `showDiff`               | `true`              | Show line changes against the baseline in the gutter (`Alt+F5` / `Alt+F3` move between them). Off, the change summary stays at zero. In Rust, needs the `diff` Cargo feature.                                                                                                      |
| `completions`            | `true`              | Schema completions (`Ctrl+Space`).                                                                                                                                                                                                                                                 |
| `autoTriggerCompletions` | `true`              | Open suggestions while typing and after accepting a key. Off, only `Ctrl+Space` opens them.                                                                                                                                                                                        |
| `hoverDocs`              | `true`              | Show the schema's title, description, type, default and examples when hovering a key, at the caret with `Ctrl+K Ctrl+I` (read to screen readers), or on a tap on a touch screen.                                                                                                   |
| `highlighting`           | `true`              | Syntax colors.                                                                                                                                                                                                                                                                     |
| `autoClosing`            | `true`              | Auto-close quotes and brackets, wrap selections, and delete empty pairs on Backspace.                                                                                                                                                                                              |
| `search`                 | `true`              | Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`). Off, those keys are left to the browser. In Rust, needs the `search` Cargo feature.                                                                                                                                                   |
| `goToLine`               | `true`              | Go to line (`Ctrl+G`). Off, the key is left to the browser. In Rust, needs the `search` Cargo feature.                                                                                                                                                                             |
| `folding`                | `true`              | Folding by structure: chevrons in the gutter, `Ctrl+Shift+[` and `Ctrl+Shift+]` at the caret, `Ctrl+K Ctrl+0` and `Ctrl+K Ctrl+J` for everything (`Cmd` on macOS). View only: the text stays whole. Switching it off opens every fold. In Rust, needs the `folding` Cargo feature. |
| `saveShortcut`           | `true`              | Whether `Ctrl+S` / `Cmd+S` saves, when a save callback is set. Off, the key is left to the browser and only the handle's `save` saves.                                                                                                                                             |
| `strings`                | English (see below) | Every built-in text (hints, tooltips, notices and their buttons, footer counts, accessible names and announcements), English by default: give only the texts to change. See `EditorStrings`.                                                                                       |

<!-- settings:end -->

### Built-in texts

`strings` rewords or translates every text the editor writes itself
(hints, tooltips, notices, footer counts, accessible names, the messages of
the problems it words). Give only the texts to change; the others keep their
English defaults, and an unknown key is warned about in the console:

```js
const editor = new ConfigEditor(element, {
  value,
  strings: {
    noProblems: "Keine Fehler",
    errorsOne: "{count} Fehler",
    errorsOther: "{count} Fehler",
  },
});
// Live, like every setting: the texts given change, the others stay.
editor.update({ strings: { find: "Suchen" } });
// `null` returns one text, or all of them, to the English default.
editor.update({ strings: { find: null } });
```

Placeholders are written in braces (`{count}`, `{line}`); a text takes the
placeholders of its default, and the editor warns in the console about any
other. Counts come in pairs: `…One` for 1, `…Other` for any other count.

While editing as another language (`editAs`), a notice under the toolbar says
what that language does not show or keep: the host file's comments and
formatting are not shown there and saving keeps them, and comments typed there
are never saved. Its texts are `editAsHostComments` (`editAsHostInstructions`
for an XML file with processing instructions), `editAsTypedComments` and
`editAsTypedCommentsNone`.

YAML tags (`!Sub 'x-${y}'`, `!GetAtt [a, b]`, `!include`, `!secret`,
`!reference`) are valid YAML: the data (what the schema validates, `parse`
returns and other languages show) is the value under the tag, a tagged
scalar as its text, and the tag stays in the YAML through every edit. An
edit the tag cannot follow (a tagged map replaced by a number) waits, naming
the tag. Several YAML documents in one text (`---` between them) are one per
file here: the problem shows at the second `---`, and `editAs` is refused
with that reason.

<!-- strings:start -->

<!-- Generated from crates/config-editor-leptos/src/strings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`. -->

<details>
<summary>Every text and its default (180 texts)</summary>

| Key                         | Default                                                                                                                                     |                                                                                                                                                                                                                                                |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `suggestionsHint`           | `"{key} for suggestions"`                                                                                                                   | The footer's suggestions hint; `{key}` is where the chord goes.                                                                                                                                                                                |
| `suggestionsKey`            | `"Ctrl+Space"`                                                                                                                              | The chord that opens suggestions, as the footer draws it: keys between `+`.                                                                                                                                                                    |
| `noProblems`                | `"No errors"`                                                                                                                               | The footer's problem count when there are none.                                                                                                                                                                                                |
| `errorsOne`                 | `"{count} error"`                                                                                                                           | One error, in the footer's problem count.                                                                                                                                                                                                      |
| `errorsOther`               | `"{count} errors"`                                                                                                                          | Several errors, in the footer's problem count.                                                                                                                                                                                                 |
| `warningsOne`               | `"{count} warning"`                                                                                                                         | One warning, in the footer's problem count.                                                                                                                                                                                                    |
| `warningsOther`             | `"{count} warnings"`                                                                                                                        | Several warnings, in the footer's problem count.                                                                                                                                                                                               |
| `infosOne`                  | `"{count} info"`                                                                                                                            | One info, in the footer's problem count.                                                                                                                                                                                                       |
| `infosOther`                | `"{count} info"`                                                                                                                            | Several infos, in the footer's problem count.                                                                                                                                                                                                  |
| `hintsOne`                  | `"{count} hint"`                                                                                                                            | One hint, in the footer's problem count.                                                                                                                                                                                                       |
| `hintsOther`                | `"{count} hints"`                                                                                                                           | Several hints, in the footer's problem count.                                                                                                                                                                                                  |
| `problemsHint`              | `"hover or focus underlined text"`                                                                                                          | After the problem count when every problem has its place.                                                                                                                                                                                      |
| `problemsHintTouch`         | `"tap underlined text"`                                                                                                                     | `problemsHint` on a touch screen, which has no hover.                                                                                                                                                                                          |
| `problemsCut`               | `"only the editor's first {count} are shown"`                                                                                               | After the problem count when the editor's own problems were cut; `{count}` is how many of them are shown (the host's are all shown).                                                                                                           |
| `problemsMore`              | `"Only the first {count} problems are shown; more follow."`                                                                                 | The problem that says the list was cut, after `{count}` problems.                                                                                                                                                                              |
| `underlinesCut`             | `"only {count} underlines are drawn, the most serious first"`                                                                               | After the problem count when the problems have more underlines than are drawn; `{count}` is how many are (the most serious first).                                                                                                             |
| `changesOne`                | `"{count} change"`                                                                                                                          | The footer's change count against the baseline, for one change.                                                                                                                                                                                |
| `changesOther`              | `"{count} changes"`                                                                                                                         | The footer's change count, for several changes.                                                                                                                                                                                                |
| `changeDetailsOne`          | `"{lines} line · Alt+F5 for the next"`                                                                                                      | The change count's tooltip when one line changed; `{lines}` lists the added, modified and removed counts.                                                                                                                                      |
| `changeDetailsOther`        | `"{lines} lines · Alt+F5 for the next"`                                                                                                     | The change count's tooltip when several lines changed.                                                                                                                                                                                         |
| `changedAdded`              | `"{count} added"`                                                                                                                           | Added lines, in the change count's tooltip.                                                                                                                                                                                                    |
| `changedModified`           | `"{count} modified"`                                                                                                                        | Modified lines, in the change count's tooltip.                                                                                                                                                                                                 |
| `changedRemoved`            | `"{count} removed"`                                                                                                                         | Removed lines, in the change count's tooltip.                                                                                                                                                                                                  |
| `linesOne`                  | `"{count} line"`                                                                                                                            | The footer's line count, for one line.                                                                                                                                                                                                         |
| `linesOther`                | `"{count} lines"`                                                                                                                           | The footer's line count, for several lines.                                                                                                                                                                                                    |
| `tabFocus`                  | `"Tab moves focus"`                                                                                                                         | The footer's indicator while Tab moves the focus.                                                                                                                                                                                              |
| `tabFocusTitle`             | `"Tab moves focus (Ctrl+M to indent with Tab)"`                                                                                             | Its tooltip; clicking it turns the mode off.                                                                                                                                                                                                   |
| `tabFocusOn`                | `"Tab moves focus: Tab and Shift+Tab now leave the editor. Ctrl+M to indent with Tab again."`                                               | Announced when `Ctrl+M` makes Tab move the focus.                                                                                                                                                                                              |
| `tabFocusOff`               | `"Tab indents: Tab and Shift+Tab indent again. Ctrl+M to move focus with Tab."`                                                             | Announced when `Ctrl+M` makes Tab indent again.                                                                                                                                                                                                |
| `schemaConnected`           | `"Schema connected"`                                                                                                                        | The toolbar's schema state.                                                                                                                                                                                                                    |
| `editAsHostComments`        | `"The {host} file's comments and formatting are not shown while editing as {edit}; saving keeps them."`                                     | The Edit-as notice when the host's file has comments, which the edited language does not show but saving keeps.                                                                                                                                |
| `editAsHostInstructions`    | `"The {host} file's comments, processing instructions and formatting are not shown while editing as {edit}; saving keeps them."`            | The same when the host's file has processing instructions too (XML `<?…?>`), which the edited language does not show either.                                                                                                                   |
| `editAsTypedComments`       | `"Comments typed in {edit} are not saved: the {host} file keeps only its own comments."`                                                    | The Edit-as notice when comments are typed in the edited language, which saving drops, and the host's language has comments.                                                                                                                   |
| `editAsTypedCommentsNone`   | `"Comments typed in {edit} are not saved: {host} has no comments."`                                                                         | The same when the host's language has no comments (JSON).                                                                                                                                                                                      |
| `editAsDismiss`             | `"Dismiss"`                                                                                                                                 | The Edit-as notice's close button.                                                                                                                                                                                                             |
| `baselineChanged`           | `"This file changed outside the editor."`                                                                                                   | The notice shown when the host's baseline changes under unsaved edits.                                                                                                                                                                         |
| `baselineChangedDetail`     | `"Reload it, losing your changes, or keep your text and compare it with the new version."`                                                  | What the notice's choices do, after its message.                                                                                                                                                                                               |
| `reload`                    | `"Reload"`                                                                                                                                  | The notice's button that takes the new baseline as the text.                                                                                                                                                                                   |
| `reloadTitle`               | `"Replace your text with the new version (Ctrl+Z brings yours back)"`                                                                       | Its tooltip.                                                                                                                                                                                                                                   |
| `keepMine`                  | `"Keep mine"`                                                                                                                               | The notice's button that keeps the text against the new baseline.                                                                                                                                                                              |
| `keepMineTitle`             | `"Keep your text; the changes are now shown against the new version (Escape)"`                                                              | Its tooltip.                                                                                                                                                                                                                                   |
| `baselineChangedKeys`       | `"F6 moves to the choices and back."`                                                                                                       | How to reach the notice's choices from the keyboard, shown on it (and announced with it); not on touch screens.                                                                                                                                |
| `findWidget`                | `"Find and replace"`                                                                                                                        | The find widget's accessible name.                                                                                                                                                                                                             |
| `toggleReplace`             | `"Toggle replace"`                                                                                                                          | The button showing the replace row.                                                                                                                                                                                                            |
| `toggleReplaceTitle`        | `"Toggle replace (Ctrl+H)"`                                                                                                                 | Its tooltip.                                                                                                                                                                                                                                   |
| `find`                      | `"Find"`                                                                                                                                    | The query field's accessible name and placeholder.                                                                                                                                                                                             |
| `matchCase`                 | `"Match case"`                                                                                                                              | The match-case toggle.                                                                                                                                                                                                                         |
| `matchCaseTitle`            | `"Match case (Alt+C)"`                                                                                                                      | Its tooltip.                                                                                                                                                                                                                                   |
| `wholeWord`                 | `"Match whole word"`                                                                                                                        | The whole-word toggle.                                                                                                                                                                                                                         |
| `wholeWordTitle`            | `"Match whole word (Alt+W)"`                                                                                                                | Its tooltip.                                                                                                                                                                                                                                   |
| `useRegex`                  | `"Use regular expression"`                                                                                                                  | The regular-expression toggle.                                                                                                                                                                                                                 |
| `useRegexTitle`             | `"Use regular expression (Alt+R)"`                                                                                                          | Its tooltip.                                                                                                                                                                                                                                   |
| `previousMatch`             | `"Previous match"`                                                                                                                          | The previous-match button.                                                                                                                                                                                                                     |
| `previousMatchTitle`        | `"Previous match (Shift+Enter)"`                                                                                                            | Its tooltip.                                                                                                                                                                                                                                   |
| `nextMatch`                 | `"Next match"`                                                                                                                              | The next-match button.                                                                                                                                                                                                                         |
| `nextMatchTitle`            | `"Next match (Enter)"`                                                                                                                      | Its tooltip.                                                                                                                                                                                                                                   |
| `close`                     | `"Close"`                                                                                                                                   | The find widget's close button.                                                                                                                                                                                                                |
| `closeTitle`                | `"Close (Escape)"`                                                                                                                          | Its tooltip.                                                                                                                                                                                                                                   |
| `replace`                   | `"Replace"`                                                                                                                                 | The replacement field's accessible name and placeholder, and the button replacing the current match.                                                                                                                                           |
| `replaceTitle`              | `"Replace (Enter)"`                                                                                                                         | The replace button's tooltip.                                                                                                                                                                                                                  |
| `replaceAll`                | `"All"`                                                                                                                                     | The replace-all button.                                                                                                                                                                                                                        |
| `replaceAllLabel`           | `"Replace all"`                                                                                                                             | Its accessible name.                                                                                                                                                                                                                           |
| `replaceAllTitle`           | `"Replace all (Ctrl+Alt+Enter)"`                                                                                                            | Its tooltip.                                                                                                                                                                                                                                   |
| `matchPosition`             | `"{current} of {total}"`                                                                                                                    | The match count: match `{current}` of `{total}`.                                                                                                                                                                                               |
| `noResults`                 | `"No results"`                                                                                                                              | The match count when nothing matches.                                                                                                                                                                                                          |
| `invalidPattern`            | `"Invalid pattern"`                                                                                                                         | The match count when the regular expression is invalid.                                                                                                                                                                                        |
| `replacedOne`               | `"Replaced {count} occurrence"`                                                                                                             | After Replace All, for one match.                                                                                                                                                                                                              |
| `replacedOther`             | `"Replaced {count} occurrences"`                                                                                                            | After Replace All, for several.                                                                                                                                                                                                                |
| `replaceSkipped`            | `"; skipped {count} in read-only text or secret placeholders"`                                                                              | Added when Replace All left matches in protected text alone.                                                                                                                                                                                   |
| `nothingReplaced`           | `"Every match is in read-only text or secret placeholders; nothing was replaced"`                                                           | After Replace All when every match is in read-only text or a secret placeholder.                                                                                                                                                               |
| `goToLine`                  | `"Go to line"`                                                                                                                              | The go-to-line field's accessible name.                                                                                                                                                                                                        |
| `goToLinePlaceholder`       | `"line or line:column"`                                                                                                                     | Its placeholder.                                                                                                                                                                                                                               |
| `goToLineHint`              | `"Line 1–{lines}, or line:column"`                                                                                                          | Its hint: the range of lines.                                                                                                                                                                                                                  |
| `goToLineInvalid`           | `"Type a line number between 1 and {lines}"`                                                                                                | Its hint when the text is no position.                                                                                                                                                                                                         |
| `addedLine`                 | `"Added line"`                                                                                                                              | The change preview's title for an added line.                                                                                                                                                                                                  |
| `modifiedLine`              | `"Modified line"`                                                                                                                           | For a modified line.                                                                                                                                                                                                                           |
| `removedLines`              | `"Removed lines"`                                                                                                                           | For removed lines.                                                                                                                                                                                                                             |
| `removedLinesAtEnd`         | `"Removed lines at end of document"`                                                                                                        | For lines removed after the last one.                                                                                                                                                                                                          |
| `showAdded`                 | `"Show added line {line}"`                                                                                                                  | The gutter marker of an added line.                                                                                                                                                                                                            |
| `showAddedTitle`            | `"Show added line"`                                                                                                                         | Its tooltip.                                                                                                                                                                                                                                   |
| `showModified`              | `"Show before and after for modified line {line}"`                                                                                          | The gutter marker of a modified line.                                                                                                                                                                                                          |
| `showModifiedTitle`         | `"Show before and after"`                                                                                                                   | Its tooltip.                                                                                                                                                                                                                                   |
| `showRemoved`               | `"Show removed lines before line {line}"`                                                                                                   | The gutter marker of removed lines.                                                                                                                                                                                                            |
| `showRemovedTitle`          | `"Show removed lines"`                                                                                                                      | Its tooltip, and that of lines removed at the end.                                                                                                                                                                                             |
| `showRemovedAtEnd`          | `"Show removed lines at end of document"`                                                                                                   | The gutter marker of lines removed after the last one.                                                                                                                                                                                         |
| `previewHeading`            | `"{title} · line {line}"`                                                                                                                   | The change preview's heading: its `{title}` and `{line}`.                                                                                                                                                                                      |
| `previewLabel`              | `"{title}, line {line}"`                                                                                                                    | The change preview's accessible name.                                                                                                                                                                                                          |
| `changePosition`            | `"{title}, line {line}: change {position} of {total}"`                                                                                      | Announced on `Alt+F5` / `Shift+Alt+F5`: the change's `{title}` (as the preview's), its `{line}`, and its `{position}` of `{total}`.                                                                                                            |
| `revert`                    | `"Revert"`                                                                                                                                  | The preview's revert button.                                                                                                                                                                                                                   |
| `revertTitle`               | `"Revert this change (R)"`                                                                                                                  | Its tooltip, for a one-line change.                                                                                                                                                                                                            |
| `revertLinesTitle`          | `"Revert this change (lines {first}–{last}) (R)"`                                                                                           | Its tooltip, for a change over lines `{first}` to `{last}`.                                                                                                                                                                                    |
| `revertLineAndRemovedTitle` | `"Revert this change (line {line} and the removed lines) (R)"`                                                                              | Its tooltip, for a changed line with removed lines.                                                                                                                                                                                            |
| `closePreview`              | `"Close change preview"`                                                                                                                    | The preview's close button.                                                                                                                                                                                                                    |
| `before`                    | `"Before"`                                                                                                                                  | The preview's column of the baseline's lines.                                                                                                                                                                                                  |
| `current`                   | `"Current"`                                                                                                                                 | The preview's column of the current lines.                                                                                                                                                                                                     |
| `fold`                      | `"Fold line {line}"`                                                                                                                        | The chevron of an open range.                                                                                                                                                                                                                  |
| `unfold`                    | `"Unfold lines {first}–{last}"`                                                                                                             | The chevron or chip of a closed fold.                                                                                                                                                                                                          |
| `foldedChanges`             | `"Folded lines have changes"`                                                                                                               | A closed fold's line number when it hides changes or problems.                                                                                                                                                                                 |
| `foldedProblemOne`          | `"hides a problem: {severity}"`                                                                                                             | After a closed fold's chip label when it hides one problem; `{severity}` names it.                                                                                                                                                             |
| `foldedProblemOther`        | `"hides {count} problems, the most serious: {severity}"`                                                                                    | After a closed fold's chip label when it hides `{count}` problems; `{severity}` names the most serious of them.                                                                                                                                |
| `foldedLines`               | `"Folded lines {first}–{last}"`                                                                                                             | Announced when the range of lines `{first}` to `{last}` folds.                                                                                                                                                                                 |
| `unfoldedLines`             | `"Unfolded lines {first}–{last}"`                                                                                                           | Announced when the fold of lines `{first}` to `{last}` opens.                                                                                                                                                                                  |
| `foldedRanges`              | `"Folded {count} ranges"`                                                                                                                   | Announced when `{count}` ranges (two or more) fold at once.                                                                                                                                                                                    |
| `unfoldedRanges`            | `"Unfolded {count} ranges"`                                                                                                                 | Announced when `{count}` folds (two or more) open at once.                                                                                                                                                                                     |
| `required`                  | `"required"`                                                                                                                                | The hover card's mark of a required key.                                                                                                                                                                                                       |
| `deprecated`                | `"deprecated"`                                                                                                                              | The hover card's mark of a deprecated key.                                                                                                                                                                                                     |
| `defaultValue`              | `"Default"`                                                                                                                                 | The hover card's default row.                                                                                                                                                                                                                  |
| `examples`                  | `"Examples"`                                                                                                                                | The hover card's examples row.                                                                                                                                                                                                                 |
| `oneOf`                     | `"One of"`                                                                                                                                  | The hover card's row listing the titles of the options (`oneOf`, `anyOf`) a field's object may take, while none matches it yet.                                                                                                                |
| `documentation`             | `"Documentation"`                                                                                                                           | The hover card's link to the schema's documentation.                                                                                                                                                                                           |
| `hoverFocused`              | `"In the docs: arrows, Page Up, Page Down, Home and End scroll them. Escape returns to the text."`                                          | Announced when a second `Ctrl+K Ctrl+I` moves the focus into the hover card, to scroll it.                                                                                                                                                     |
| `hoverLabel`                | `"About {key}"`                                                                                                                             | The focused hover card's accessible name: the `{key}` it documents.                                                                                                                                                                            |
| `problemTipLabel`           | `"Problems here"`                                                                                                                           | The problem tip's accessible name while it has the focus (a second `Ctrl+K Ctrl+I`).                                                                                                                                                           |
| `openDocs`                  | `"Open docs ↗ (Alt+Enter)"`                                                                                                                 | The link under a host problem's message.                                                                                                                                                                                                       |
| `openDocsTouch`             | `"Open docs ↗"`                                                                                                                             | `openDocs` on a touch screen, which has no keys to name.                                                                                                                                                                                       |
| `openDocsLabel`             | `"{link}: {message}"`                                                                                                                       | A problem's docs link as a screen reader names it, told apart from the others in a tip: its `{link}` text (`openDocs`) and its problem's `{message}`.                                                                                          |
| `error`                     | `"Error"`                                                                                                                                   | An error, as a screen reader hears it.                                                                                                                                                                                                         |
| `warning`                   | `"Warning"`                                                                                                                                 | A warning, as a screen reader hears it.                                                                                                                                                                                                        |
| `info`                      | `"Info"`                                                                                                                                    | An info, as a screen reader hears it.                                                                                                                                                                                                          |
| `hint`                      | `"Hint"`                                                                                                                                    | A hint, as a screen reader hears it.                                                                                                                                                                                                           |
| `problemSource`             | `" from {source}"`                                                                                                                          | Who reported a host problem: the `{source}` of the problem texts below (empty for the editor's own problems).                                                                                                                                  |
| `problem`                   | `"{severity}{source}: {message}"`                                                                                                           | A problem at the caret, for screen readers.                                                                                                                                                                                                    |
| `problemPosition`           | `"{severity} {position} of {total}{source}: {message}"`                                                                                     | Announced on `F8`: problem `{position}` of `{total}`.                                                                                                                                                                                          |
| `problemCode`               | `" Code {code}."`                                                                                                                           | After a problem's text for screen readers: its `{code}` (from the host).                                                                                                                                                                       |
| `problemDocs`               | `" Documentation: {url} (Alt+Enter opens it)."`                                                                                             | After a problem's text (or the hover card's docs) for screen readers: its documentation's `{url}`, and the key that opens it.                                                                                                                  |
| `problemMarker`             | `"{language} {severity}{source} on line {line}: {message}"`                                                                                 | An underlined problem's accessible name.                                                                                                                                                                                                       |
| `protectedNote`             | `"{label}: {message}"`                                                                                                                      | What protected text at the caret is, for screen readers: its `{label}` and `{message}`.                                                                                                                                                        |
| `sourceLabel`               | `"{language} source"`                                                                                                                       | The text area.                                                                                                                                                                                                                                 |
| `suggestionsLabel`          | `"{language} suggestions"`                                                                                                                  | The suggestions list.                                                                                                                                                                                                                          |
| `problemsLabel`             | `"Inline {language} diagnostics"`                                                                                                           | The layer of underlined problems.                                                                                                                                                                                                              |
| `editorLabel`               | `"{language} editor"`                                                                                                                       | The editor's frame.                                                                                                                                                                                                                            |
| `readOnlyEditor`            | `"Cannot edit in a read-only editor"`                                                                                                       | Said, and shown at the caret, when typing in a read-only editor.                                                                                                                                                                               |
| `readOnlyEditorReason`      | `"Cannot edit in a read-only editor: {reason}"`                                                                                             | The same, for an editor the host said why it is read-only (`readOnlyReason`): `{reason}`.                                                                                                                                                      |
| `readOnlyField`             | `"This field is read-only"`                                                                                                                 | Why a field is read-only when neither the schema nor the host says.                                                                                                                                                                            |
| `readOnlyText`              | `"This text is read-only"`                                                                                                                  | Why text in a read-only range the host gave cannot change.                                                                                                                                                                                     |
| `holdsReadOnly`             | `"Holds a read-only field: {reason}"`                                                                                                       | Why renaming or moving the key above a read-only field is refused.                                                                                                                                                                             |
| `saveRefused`               | `"{pointer} is read-only and this change would alter it"`                                                                                   | A save refused because it would change a read-only value.                                                                                                                                                                                      |
| `saveWaits`                 | `"Not saved: {pointer} is read-only, and it can be checked only once the text is valid"`                                                    | A save waiting for the text to be valid: the language cannot say where the read-only field at `{pointer}` is written, so its value is checked only in a text that parses.                                                                      |
| `readOnlyChanged`           | `"This edit makes the text valid, but {pointer} would no longer read as the host gave it ({reason}): undo the change to it first"`          | An edit refused because it makes a text that did not parse parse with the read-only field at `{pointer}` reading otherwise than in the host's text, and why the field is read-only (`{reason}`).                                               |
| `readOnlyUnchecked`         | `"Too many lines were deleted while the text was invalid to check {pointer} ({reason}): undo deleting the lines that held it first"`        | An edit refused because it makes a text that did not parse parse after more lines that may hold the read-only field at `{pointer}` were deleted from the host's text than can be checked through, and why the field is read-only (`{reason}`). |
| `readOnlyLabel`             | `"Read-only"`                                                                                                                               | The gutter lock's label, before the reason.                                                                                                                                                                                                    |
| `secretLabel`               | `"Secret placeholder"`                                                                                                                      | The gutter key's label, before the hint.                                                                                                                                                                                                       |
| `secretHint`                | `"Keeps the current secret unless you replace it"`                                                                                          | The hover text of a secret placeholder, unless the host gives one.                                                                                                                                                                             |
| `secretValue`               | `"the secret value"`                                                                                                                        | What a schema problem says instead of a secret field's value.                                                                                                                                                                                  |
| `unknownPlaceholder`        | `"{pointer}: unknown secret placeholder; it stands for no current secret, so write the new value instead"`                                  | A placeholder the host did not give, in a secret field.                                                                                                                                                                                        |
| `movedPlaceholder`          | `"{pointer}: this placeholder stands for the secret of {owner}, not this field's; here it is plain text"`                                   | One of the host's placeholders outside the field it stands in.                                                                                                                                                                                 |
| `placeholderInComment`      | `"This placeholder stands for the secret of {owner}; in a comment it is plain text"`                                                        | One of the host's field placeholders written in a comment.                                                                                                                                                                                     |
| `commentPlaceholder`        | `"{pointer}: this placeholder stands for a secret in a comment, not this field's; here it is plain text"`                                   | A placeholder the host wrote in a comment, in a field.                                                                                                                                                                                         |
| `gluedPlaceholder`          | `"{pointer}: a secret placeholder stands for the whole secret, so nothing can be written beside it: replace it whole to write a new value"` | Text written beside a placeholder the host gave as a field's whole value: the edit refused, the field flagged, a save waiting.                                                                                                                 |
| `envReference`              | `"Read the secret from an environment variable"`                                                                                            | The detail of the `${…}` reference suggestion.                                                                                                                                                                                                 |
| `fileReference`             | `"Read the secret from a file"`                                                                                                             | The detail of the `file:` reference suggestion.                                                                                                                                                                                                |
| `otherReference`            | `"A reference starting with {prefix}"`                                                                                                      | The detail of another reference suggestion.                                                                                                                                                                                                    |
| `referenceType`             | `"reference"`                                                                                                                               | The type column of a reference suggestion.                                                                                                                                                                                                     |
| `closesTag`                 | `"Closes <{name}>"`                                                                                                                         | The detail of the end tag XML suggests.                                                                                                                                                                                                        |
| `endTag`                    | `"end tag"`                                                                                                                                 | Its type column.                                                                                                                                                                                                                               |
| `keyNotAllowed`             | `"\"{key}\" is not allowed here"`                                                                                                           | A key the schema does not allow (`{key}`); its object's pointer goes before it (`/web: `), as before every schema problem but at the root.                                                                                                     |
| `textNotAllowed`            | `"text is not allowed here, only elements"`                                                                                                 | Text in an element the schema wants elements in (XML); the element's pointer goes before it.                                                                                                                                                   |
| `keyDeprecated`             | `"{pointer}: \"{key}\" is deprecated"`                                                                                                      | A deprecated key.                                                                                                                                                                                                                              |
| `keyDeprecatedInstead`      | `"{pointer}: \"{key}\" is deprecated. {message}"`                                                                                           | A deprecated key whose schema says what to use instead.                                                                                                                                                                                        |
| `notOneOf`                  | `"{value} is not one of {allowed}"`                                                                                                         | A value not among the allowed ones.                                                                                                                                                                                                            |
| `wrongType`                 | `"{value} is not of type {types}"`                                                                                                          | A value of none of the accepted types.                                                                                                                                                                                                         |
| `invalidSchema`             | `"Invalid JSON Schema: {error}"`                                                                                                            | A schema that does not compile.                                                                                                                                                                                                                |
| `invalidPlaceholder`        | `"secrets.placeholder: {error}"`                                                                                                            | A secret placeholder pattern the editor cannot use (it does not compile, or matches empty text): no token is taken for a placeholder.                                                                                                          |
| `problemAtPath`             | `"{path}: {message}"`                                                                                                                       | A host's problem with a field the editor cannot find in the text, listed with the field's path.                                                                                                                                                |
| `hostCannotKeep`            | `"{pointer}: {host} cannot keep this value exactly; it reads back differently"`                                                             | A value the host's language reads back differently (Edit-as).                                                                                                                                                                                  |
| `editCannotShow`            | `"{pointer}: {edit} cannot show this value exactly; editing it here may change it"`                                                         | A value the edited language cannot show exactly (Edit-as).                                                                                                                                                                                     |
| `notSaved`                  | `"Not saved as {host}: {error}"`                                                                                                            | An edit the host's language cannot save (Edit-as).                                                                                                                                                                                             |
| `noNull`                    | `"{host} has no null value (at {pointer})"`                                                                                                 | A null the host's language cannot write (Edit-as).                                                                                                                                                                                             |
| `noNonFinite`               | `"{host} has no infinity or NaN (at {pointer})"`                                                                                            | An infinity or NaN the host's language has no number for (Edit-as).                                                                                                                                                                            |
| `cannotSwitch`              | `"Cannot switch to {edit} yet: {error}"`                                                                                                    | An Edit-as switch waiting until the document converts.                                                                                                                                                                                         |
| `switchRefused`             | `"Cannot edit as {edit}: {error}"`                                                                                                          | An Edit-as switch refused: the host's language or the other one cannot write documents.                                                                                                                                                        |
| `mixedContent`              | `"{pointer}: text beside child elements cannot be shown in another language"`                                                               | XML text beside child elements, which no other language can show (Edit-as).                                                                                                                                                                    |
| `tabList`                   | `"Open files"`                                                                                                                              | The tab list's accessible name, unless the host gives one.                                                                                                                                                                                     |
| `noDocuments`               | `"No open files"`                                                                                                                           | Shown in place of an editor while no document is open.                                                                                                                                                                                         |
| `closeTab`                  | `"Close {name}"`                                                                                                                            | A tab's close button.                                                                                                                                                                                                                          |
| `closeTabTitle`             | `"Close (Delete)"`                                                                                                                          | Its tooltip.                                                                                                                                                                                                                                   |
| `addDocument`               | `"New file"`                                                                                                                                | The "+" button after the tabs (with `onAdd`): its accessible name and tooltip.                                                                                                                                                                 |
| `unsaved`                   | `"unsaved changes"`                                                                                                                         | The dirty dot's tooltip, and what a tab's accessible name adds for it.                                                                                                                                                                         |
| `readOnlyTab`               | `"read-only"`                                                                                                                               | What a read-only document's tab adds to its accessible name.                                                                                                                                                                                   |
| `readOnlyTabTitle`          | `"{name}\nRead-only: {reason}"`                                                                                                             | The tooltip of a read-only document's tab with a reason (`readOnlyReason`): its file `{name}` and the `{reason}`.                                                                                                                              |

</details>

<!-- strings:end -->

### `update(options)`

`editor.update(options)` changes a mounted editor, live (the caret, undo
history, scroll and folds stay). It takes the constructor's options, any of
them, and is the one way to change one:

```ts
editor.update({ readOnly: true, theme: "nord" });
editor.update({ value: loaded, baseline: loaded }); // a file the host loaded
editor.update({ hostDiagnostics: problems, onSave: null });
```

<details>
<summary>How update treats left-out keys, null, settings, a new value and fixed options</summary>

- A key left out, or set to `undefined`, stays as it is.
- A callback given where there was none reporting a state (`onEditAsChange`,
  `onSyncChange`, `onProblems`, `onDirtyChange`, `onChangeSummary`) hears the
  current one at once, as it would have on mount (a `ConfigDocuments` tells
  it of each document).
- `null` removes a callback (`onSave: null` gives `Ctrl+S` back to the
  browser), clears `hostDiagnostics`, `readOnlyPaths` and `readOnlyRanges`,
  turns `editAs` and `readOnly` off, and returns a setting to its default.
- Settings merge key by key, `strings` text by text and `colors` token by
  token, as the settings do: a text or token set to `undefined` stays, one
  set to `null` returns to its default (`colors: null` clears them all).
- A new `value` replaces the document as a host update, whatever it is (a
  file switched to, a discard, a reset); a `value` that is the document
  already changes nothing. The caret stays on the same text when possible.
  `readOnlyRanges` and host `range`s given with it are in the new document.
  For a host that hands every `onChange` text back, see
  [Hosts that echo](#hosts-that-echo).
- `rangesFor` names the text an update's `readOnlyRanges` and host `range`s
  were worked out for, when it is not the document (a check of a text the
  user has typed past since): they are checked in it and moved to the
  document, a range around an edit growing over it (a lock never lets a
  locked character be edited). Ranges given with a `value` are in that
  `value`, applied or an echo.
- `language`, `schema`, `fileName`, `className`, `secrets` and `echoes` are what the
  editor is built from: they throw (`"language cannot change on a mounted
ConfigEditor …"`); create a new editor to change them. The `language`
  the editor already has changes nothing. TypeScript types them `never` in
  `EditorUpdate`.
- Every key is checked before any applies: a wrong value throws and changes
  nothing.

</details>

#### Hosts that echo

A host that only saves what `onChange` gives (`onChange: save`, as in the
[Quick start](#quick-start)) needs nothing more: every `value` it passes to
`update` applies. A host that hands every `onChange` text back through
`update({ value })` (a store that re-renders after a delay, a fetch per
edit) may do it late, after the user typed on, and in any order. Applying
such an echo would roll the document back, so tell the editor with
`echoes: true`:

```ts
const editor = new ConfigEditor(element, {
  value: store.text,
  echoes: true,
  onChange: (text) => store.save(text), // the store calls editor.update({ value }) later
});
```

Then a `value` that `onChange` gave and that has not come back yet is its
echo and keeps the document, whatever its order or delay: nothing typed is
lost. Ranges given with an echo (`readOnlyRanges`, host `range`s) are in
the echoed text: they are moved to the document. Any other `value` is yours
and applies, the text you gave last included (a reset to the text the
editor was created with). The texts `onChange` gave before it belong to the
text it replaced (another file, a discarded draft): they are yours from
then on, so going back to one (a file switched back to) applies. So once
you give a text of your own, drop the answers still to come for the texts
before it, or hand them to the file they belong to; a host that cannot
(every answer handed back, whatever happened since) resets with
`replaceValue` instead. When an echo
leaves your store holding an older text and nothing still on its way brings
it back, `onChange` gives it the document again. A store that takes
several texts in one render passes only the last, so the echo that would
bring it back may never be seen: if the store is still on an older text
250 ms after such an echo, `onChange` gives it the document again then. To go back to a text
`onChange` gave that may still be on its way (a discard to what you saved, a
revert), or to reset while the echoes of what was typed are still to come,
call `replaceValue(text)`: it always applies, and the echoes on their way
are still told apart. The promise is
yours to keep: a host that hands back only some texts (the latest of
several) leaves the others counted as echoes (every text sent is
remembered until its echo comes, a text of your own replaces the
document, or 5 seconds after a text sent later came back: an echo
handed back out of order more than 5 seconds after a later one is taken
for your own text), so set those with `replaceValue` too.

### Instance members

`ConfigEditor` is an `EditorView`, the interface a document's editor in a
[`ConfigDocuments`](#several-documents) (`view.editor(id)`) implements too:

| `EditorView` member                                                    | Description                                                                                                                                                                            |
| ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `value` (getter)                                                       | The current document, in `language`, including an edit made in `editAs` that is still waiting out the typing pause (it is sent to `onChange` first). Still readable after `destroy()`. |
| `text` (getter)                                                        | The text on screen: in the `editAs` language while editing as another one. `rangeOf` and `pathAt` index into it; `value` is the document in `language`.                                |
| `isDirty` (getter)                                                     | Whether the document differs from the baseline.                                                                                                                                        |
| `currentPath` (getter)                                                 | The path of what the caret is on; see [Paths](#paths).                                                                                                                                 |
| `foldedLines` (getter)                                                 | The folded blocks, one-based `{ startLine, endLine }[]`: the line that stays visible and the last hidden one.                                                                          |
| `settings` (getter)                                                    | The settings in effect, every key set; see [Settings](#settings).                                                                                                                      |
| `reveal(path)`, `revealRange(range)`, `rangeOf(path)`, `pathAt(index)` | Paths and positions; see [Paths](#paths).                                                                                                                                              |
| `fold(path)`, `unfold(path)`, `foldAll()`, `unfoldAll()`               | Fold the block a field's key opens / open the folds around a field (`false` when there was nothing to do); fold or open everything. See [Folding](#folding).                           |
| `focus()`                                                              | Focus the editor, keeping its selection.                                                                                                                                               |
| `save()`                                                               | Save as `Ctrl+S` does (calls `onSave`); `false` when nothing was sent (no `onSave`, text that does not parse, or an `editAs` edit that cannot convert back yet).                       |
| `unifiedDiff(context?)`                                                | The `diff -u` of the baseline against the document, with `context` unchanged lines around each change (default 3); `""` when they are the same. Its lines end with LF.                 |

`ConfigEditor` adds:

| Member               | Description                                                                                                                                                                                                                                                                                                                                           |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `update(options)`    | Change the editor live; see [`update`](#updateoptions).                                                                                                                                                                                                                                                                                               |
| `replaceValue(text)` | Replace the document with a text of your own, whatever it is: with `echoes`, `update({ value })` takes a text `onChange` gave that has not come back yet for its echo and keeps the document; this never does (a revert to such a text). The caret stays on the same text when possible; `onChange` does not hear it.                                 |
| `destroy()`          | Unmount. An edit still converting (while editing as another language) is sent to `onChange` first. Safe to call inside any callback (`onChange`, `onSave`, …).                                                                                                                                                                                        |
| `free()`             | Release the WebAssembly object after `destroy()`. Optional; the garbage collector does it too. An editor stays on the page until `destroy()`, whether or not you keep a reference to it. Call `destroy()` first: after `free()` the instance cannot be used at all (wasm-bindgen's own method). Inside a callback it frees once the callback returns. |

An exception thrown by a callback is logged to the console and does not
break the editor.

### Host diagnostics

When the host checks the document itself (a service, a build step), show its
problems with the editor's own:

```ts
editor.update({
  hostDiagnostics: [
    {
      path: "/web/listen",
      severity: "error",
      message: "port 80 is taken",
      code: "E_PORT",
      source: "validator",
      docsUrl: "https://example.com/docs/ports",
    },
    {
      range: { startLine: 4, startColumn: 1, endColumn: 19 },
      severity: "hint",
      message: "consider TLS",
    },
    { severity: "warning", message: "restart needed to apply" }, // the whole document
  ],
});
```

<details>
<summary>Paths and ranges, severities, F8 and hover, what throws, limits</summary>

- `path` (a JSON pointer or keys and indices) is marked where the field is
  written and follows it through edits and `editAs`. `range` is in the
  document (`value`, in `language`): string indices `{ start, end }` or
  one-based `{ startLine, startColumn, endLine?, endColumn? }`, underlined on
  every line it covers (up to 200; at most 200 underlines in all, the most
  serious problems first, and the footer says when some were left out;
  `F8` reaches every one). While editing as another language it
  marks the field it starts in (its key when it starts on the key). With
  neither, or when the field is missing, the problem shows in the footer,
  after the field's path (`/web/listen: port 80 is taken`).
- A `range` with no end (`{ startLine, startColumn }`, no `endColumn` and
  no `endLine` past `startLine`) is a point and never widened from the text
  around it: on a secret placeholder (a token the `secrets` pattern
  matches) it underlines the whole token, from any column inside it;
  elsewhere one character, at a line's end the place just past it, on an
  empty line its start.
- `severity` is `"error"` (default), `"warning"`, `"info"` or `"hint"`,
  written so (`"ERROR"` throws); colors `error`, `warning`, `info` and
  `hint` in `colors`. `message` is required and not empty.
- `F8` stops at them, the footer counts them, and the hover message adds
  `source` (default `"host"`), `code` and "Open docs ↗": a click on the
  message opens `docsUrl` (an `http:` or `https:` URL with a host), as `Alt+Enter`
  does with the caret on the problem; screen readers hear the code and the
  link with the problem.
- `path` and `range` are exclusive; a range past the text's end, ending
  before it starts, `{ start }` without `end`, or a pointer without its
  leading `/` throws. A range may give both styles (as `rangeOf` returns
  it) when they name the same stretch, its indices in `value` or in the
  text on screen (the editor's own `rangeOf`, whose lines end with LF: in a
  CRLF file its indices alone name another stretch of `value`, so give its
  range whole or its lines). `start` and `end` are string indices
  (UTF-16 units, as `selectionStart` counts); `startColumn` and `endColumn`
  count characters (code points, as problems' columns do): `😀` is two
  indices and one column. A column past its line's end is read as the
  line's end, with a console warning (the rest of the update applies).
- The editor lists and underlines at most 20 problems of each source (the
  schema's, the secret placeholders'), and the footer says how many were
  cut ("20+ errors · only the editor's first 20 are shown"); a problem with no place
  (the whole document's) stays in the footer and `F8` reaches it.
- Where the host marks the same text as one of the editor's own problems
  with the same message (with or without its JSON pointer), it shows once:
  the more serious of the two, the host's when as serious (a host warning
  repeating a schema error leaves the error);
  a different message shows beside it. A new list replaces the previous one (`null` or
  `[]` clears it); only the markers update. An invalid entry throws, naming
  its index.

</details>

### Read-only fields

A field is read-only when the schema marks it `readOnly: true` (with its
reason in `x-readonly-reason`), or when the host names it:

```ts
const editor = new ConfigEditor(element, {
  language: "yaml",
  value,
  schema, // {"properties": {"id": {"readOnly": true, "x-readonly-reason": "Assigned when created"}}}
  readOnlyPaths: ["/limits"],
  readOnlyRanges: [{ startLine: 1, startColumn: 1, endLine: 2, endColumn: 1 }],
});
editor.update({ readOnlyPaths: ["/limits", "/tls/cert"] }); // at runtime
```

<details>
<summary>How locks behave: refused edits, formatting, syntax errors, suggestions, Edit-as</summary>

- A lock in the gutter marks each locked line, the value is muted (color
  token `locked`), and hovering it shows the reason.
- Typing, deleting, pasting, cutting, completions, Replace, `Ctrl+/`,
  Revert and undo that would change a read-only value, delete its key (also
  inside a deleted selection or parent block) or add to a read-only block
  are refused: the text stays, and a tooltip at the caret says why. Replace
  All skips matches in read-only text and says how many. The line commands
  are judged the same way: a locked line moves among its siblings
  (`Alt+↑` / `Alt+↓`, or cut and pasted past several), not out of its
  parent or into a list.
- Whitespace, comments and punctuation around a locked field stay free
  (also whitespace inside it, such as a space after `:`); in YAML a locked
  line's indent cannot change (it is structure). A syntax error anywhere
  keeps every lock until it is repaired, also in a `value` that does not
  parse and for `readOnlyPaths` given while the text does not parse.
- Suggestions leave read-only keys out (inside a read-only block they are
  listed with the reason).
- With `editAs`, the same fields are locked in the other language, and a
  save that would change one is refused (shown as an error, not sent).
- In a custom language without `spans`, a field is checked by value: while
  the text does not parse, an edit that would change a read-only value in
  the last text that parsed is refused, and `save()` waits (returns
  `false`, saying which field) until the text parses again.

</details>

### Secrets

Fields the schema marks `writeOnly: true` or `x-secret: true` are secrets.
The host sends a placeholder instead of the value, and the editor keeps it
unless the user replaces it:

```ts
const editor = new ConfigEditor(element, {
  language: "yaml",
  value: "tls:\n  key: «secret 3f2a»\n",
  schema,
  secrets: {
    placeholder: "«secret [0-9a-f]+»", // the default (Rust regex syntax)
    isPlaceholder: (token) => knownTokens.has(token), // optional
    hint: "Keeps the current secret unless you replace it",
    referencePrefixes: ["${", "file:"],
  },
});
```

<details>
<summary>How placeholders behave: where they count, what throws, atomic editing, chips, references, validation</summary>

- A placeholder is real only in the field of the host's own `value` it
  was given in (and accepted by `isPlaceholder`). Anywhere else it is plain
  text, so it never stands for another field's secret: one of the host's
  tokens pasted into another field, or its field's own key renamed, is
  flagged, and so is a token the host did not give in a secret field. Put
  back into its own field it is real again. It follows its field when the
  path to it changes but not the field: a list item deleted, inserted or
  moved before it, or a key above it renamed (`db` made `database`). A token the host wrote in a comment is real
  in that text's comments only: pasted into a field it is flagged, as a
  field's token is in a comment, each where it is written.
- A token is opaque: whatever `placeholder` matches, kept whole as written;
  nothing is read from inside it, so the same token in two fields stands
  for each field's own secret. Make the pattern match your tokens exactly
  (`«secret [0-9a-f]{8}»`, not `«.*»`): text it does not match (`«hidden»`)
  is plain text. `placeholder` must be a regular expression
  that matches no empty text (`.*` throws). An exception `isPlaceholder`
  throws is logged, and the token is not real. `referencePrefixes` must be
  non-empty strings.
- A placeholder is atomic: the caret never lands inside, `Backspace` and
  `Delete` remove it whole, an edit cutting into it replaces it whole (a
  placeholder pasted over another, or by a bare `insertFromPaste` input,
  stays whole), and
  copying keeps its text. It shows as a chip (color tokens `secret-chip`,
  `secret-chip-border`) with a key in the gutter; hovering shows `hint`.
- In a secret field, suggestions offer `${VAR}` and `file:` references.
- The schema check accepts an untouched placeholder (no `pattern`,
  `format`, length or type problem), and `editAs` and saving keep it
  byte-for-byte. Nothing the editor shows, says or hands back (problems,
  hover, announcements, the diff preview, copies, `onChange`, console
  messages) holds a token the host did not send.
- Editing in `language`, `onChange` hands back exactly the text on screen
  with the host's line endings, byte order mark, final newline, tabs and
  quoting (placeholders included); a text that did not change is not
  reported. In `editAs`, the value handed back is the conversion back to
  `language`: a host that writes the text out as is must write that value.

</details>

### Paths

A path names a field the same way in every language: a JSON pointer
(`"/web/hosts/1"`, `""` for the whole document; as RFC 6901 has it, `"/"`
is the key `""`) or its keys and indices (`["web", "hosts", 1]`). A pointer starts with `/`: `"web/hosts"` throws,
everywhere a path is taken (the editor's methods, the tools,
`readOnlyPaths`, host diagnostics). Each segment is verbatim (`"/ a"` is
the key `" a"`; `~1` is `/` and `~0` is `~`), and only `0` or digits
without a leading zero index a list, where the value is one (in an object
`"/1"` is the key `"1"`; `"/01"` is always a key). A path the document does
not have (a missing parent, list index or leaf) is found nowhere: `rangeOf`
gives `null`, `reveal` `false`, a lock there locks nothing, and a host
problem there is shown for the whole document, naming its path. An index names an item only where there is a list: a list written as
one, repeated XML elements or HCL blocks, and the element or block
written once, which XML and HCL read as a one-item list too (`/web/0` is
`<web>` or `web { }` written once). An index on anything else is found
nowhere: on a scalar, an object, an attribute, HCL blocks keyed by their
labels (`/service/0` beside `service "api" { }`), another index
(`/web/0/0`) or the document (`/0`).

```ts
editor.reveal("/web/listen"); // scroll to it, select it, focus; false when absent
editor.revealRange({ start: 0, end: 12 }); // any stretch of the text on screen, a comment included
editor.rangeOf(["web", "listen"]);
// { start: 31, end: 43, startLine: 3, startColumn: 3, endLine: 3, endColumn: 15 } or null
editor.pathAt(35); // { pointer: "/web/listen", segments: ["web", "listen"] } or null
editor.currentPath; // the caret's path
new ConfigEditor(element, {
  onPathChange: (path) => (crumb.textContent = path.pointer || "/"),
});
```

`start`/`end` and `pathAt`'s index are string indices into the text on screen
(the `editAs` language while editing as another one), as `selectionStart`
counts; lines and columns are one-based. `reveal` and `rangeOf` cover the key
and its value when they share a line (`listen: 7835`, `<listen>7835</listen>`),
the key alone when the value is a block below it, the value alone for a list
item, and the dash or header of an item holding a block (`- name: web`, a
repeated HCL block, a TOML `[[array]]` entry). `pathAt` gives the key under or just before the index, else the
value or container holding it (`""` at the top level).
`revealRange({ start, end })` does what `reveal` does for any stretch of the
text on screen (string indices, as `rangeOf` gives them, so
`editor.revealRange(editor.rangeOf(path))` is `reveal(path)`): it selects
it, scrolls it into view inside the editor and focuses the editor, leaving
the page where it is. A range past the end is clamped to it; `false` before
the editor mounts and for an empty document.

## Languages and themes

The main entry also lists what the editor accepts, for pickers and file
handling (after `loadConfigEditor()`):

| Function                       | Returns          | Description                                                                                                                                     |
| ------------------------------ | ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `languages()`                  | `LanguageInfo[]` | The built-in and registered languages: `{ name, label, icon, extensions, defaultFileName }` each (`name` is what `language` and `editAs` take). |
| `themes()`                     | `ThemeInfo[]`    | The bundled themes: `{ name, label, dark }` each (`name` is what `theme` takes, `label` its display name, `dark` whether it is a dark theme).   |
| `languageForFile(path)`        | `string \| null` | The language of a file from its extension (`"main.tf"` gives `"hcl"`), registered languages included.                                           |
| `registerLanguage(definition)` | `string`         | Add a custom language (written in JavaScript); returns its name. See [Custom languages](#custom-languages).                                     |

```js
import { languageForFile, languages, themes } from "@pngwasi/config-editor";

languageForFile("deploy/main.tf"); // "hcl"
languages().find((language) => language.extensions.includes("yml"));
// { name: "yaml", label: "YAML", icon: "Y", extensions: ["yaml", "yml"], defaultFileName: "untitled.yaml" }
for (const theme of themes())
  picker.append(new Option(theme.label, theme.name));
```

### XML mapping

Child elements and attributes are object properties, element text is a typed
scalar (`<listen>7835</listen>` is the number 7835), and repeated siblings are
an array. An attribute sharing its name with a child element is the property
`@name` (`<r a="1"><a>2</a></r>` is `{"r": {"@a": 1, "a": 2}}`), so the two
stay apart. Another host's `@name` key holding a value (an npm `@scope`) is
shown as the attribute `name` in an XML view and saved back as `@name`
(converted alone, it is a loss: it reads back `name`); one holding a map or
list, and a `#text` beside child keys, make the switch to XML wait. A top-level element marked `config-root="true"` is a transparent
wrapper; without it, every top-level element is a document property, so a
file may hold one top-level element or several (a fragment). The editor never
adds a root: `{"listen": 4343, "port": 80}` writes `<listen>4343</listen>` and
`<port>80</port>`, and a key added at the top in another view is one more
top-level element.

## `@pngwasi/config-editor/tools`

The editor's answers about a text, without an editor: for a server, a
build step or a review screen. They need the WebAssembly module loaded:
`loadConfigEditor()`, which this entry exports too.

| Function                                   | Returns                   | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------------------------------------------ | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `validate(language, text, schema?)`        | `Diagnostic[]`            | Syntax problems, plus schema problems when a schema is given.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `convert(from, text, to, schema?)`         | `{ text, losses }`        | Convert between languages. `losses` lists JSON pointers of values the target reads back differently (an infinity or `NaN` is written as the text `inf` or `nan` in JSON, HCL and XML, and listed). Throws when the text does not parse or the target cannot express it at all (TOML has no `null`). With a schema, a lone XML element where it expects a list converts as a one-item list.                                                                                                                                                                                    |
| `unifiedDiff(baseline, current, context?)` | `string`                  | The `diff -u` of two texts: `--- baseline` / `+++ current` headers, then `@@` hunks with `context` lines around each change (default 3). `""` when they have the same lines; line endings are compared as LF.                                                                                                                                                                                                                                                                                                                                                                 |
| `changeSummary(baseline, current)`         | `ChangeSummary`           | `{ added, removed, modified, hunks }` between two texts, as the diff gutter and `onChangeSummary` count them: a final line break added (or removed) is the empty last line added (or removed), where `unifiedDiff` writes it as `diff -u` does (the last line changed, `\ No newline at end of file`).                                                                                                                                                                                                                                                                        |
| `parse(language, text)`                    | `unknown`                 | The document as plain data (objects, arrays, strings, numbers, booleans, `null`): the model schema validation sees, the same in every language. YAML's `.inf`/`.nan` and TOML's `inf`/`nan` are `Infinity`, `-Infinity` and `NaN`. An integer past 2^53 (TOML and YAML hold 64-bit ones) is a `bigint`, exact, and `serialize` takes one back; one past 64 bits, or a JSON number past a float's range (`1e400`), is a syntax error, in the editor too. A YAML number with a leading zero (`012`) is the string it is written as (YAML 1.1 read it as octal, 1.2 as decimal). |
| `serialize(language, value)`               | `string`                  | Plain data as a document, only when it reads back as exactly that value (the check `convert`'s `losses` make). Throws when the value is not plain data (a `Date`, `Map`, `Set` or class instance throws: pass a date's `toISOString()`) or the language cannot express it exactly (TOML has no `null`; XML has no empty, nested or one-item list). In every language, as `JSON.stringify` does, an object member set to `undefined` is left out, and an `undefined` or a hole in an array throws.                                                                             |
| `rangeOf(language, text, path)`            | `EditorRange \| null`     | Where a field is written in a text, as the editor's `rangeOf` answers; see [Paths](#paths).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `rangesOf(language, text, paths)`          | `(EditorRange \| null)[]` | `rangeOf` for each of many paths, in order, the text read once for all of them (thousands of paths one by one each read it again).                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `pathAt(language, text, index)`            | `EditorPath \| null`      | The path at a string index of a text, as the editor's `pathAt` answers.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `schemaWarnings(schema)`                   | `string[]`                | What the editor cannot use in a schema (the messages `onSchemaWarning` receives). Throws on an invalid schema.                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `presetCss()`                              | `string`                  | The stylesheet the editor injects (layout and every theme), for `styled: false`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |

```ts
interface Diagnostic {
  line?: number; // one-based; absent when the problem has no position
  column?: number;
  endColumn?: number; // just past the marker, on endLine when it is set
  endLine?: number; // only when the marker covers several lines
  message: string;
  severity: "error" | "warning" | "info" | "hint";
  origin: "editor" | "host"; // who found it (`validate` and `onProblems`: always "editor")
  deprecated?: boolean; // true on a deprecated key's warning (the marker is the key)
}
```

```js
import {
  convert,
  loadConfigEditor,
  parse,
  rangeOf,
  rangesOf,
  serialize,
  validate,
} from "@pngwasi/config-editor/tools";

await loadConfigEditor();

validate("json", '{"insecure_plaintext": "yes"}', schemaText);
// [{ line: 1, column: 24, endColumn: 29, severity: "error", origin: "editor",
//    message: '/insecure_plaintext: "yes" is not of type "boolean"' }]

// At most 20 problems are listed; past that, a 21st without a position says
// more follow (the footer shows it too).

convert("yaml", "listen: 7835\n", "toml");
// { text: "listen = 7835\n", losses: [] }

parse("toml", "[web]\nhttp = 80\n"); // { web: { http: 80 } }
serialize("hcl", { web: { http: 80 } }); // "web {\n  http = 80\n}\n"
rangeOf("yaml", "web:\n  http: 80\n", "/web/http");
// { start: 7, end: 15, startLine: 2, startColumn: 3, endLine: 2, endColumn: 11 }
rangesOf("yaml", "web:\n  http: 80\n", ["/web/http", "/web/https"]);
// [{ start: 7, end: 15, … }, null]
```

## `@pngwasi/config-editor/binding`

React, Vue and Svelte have packages of their own, built on what this
entry exports (`EditorBinding`, `DocumentsBinding`, `SETTING_TYPES`):

| Package                                                                                        | Component                                                     |
| ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| [`@pngwasi/config-editor-react`](https://www.npmjs.com/package/@pngwasi/config-editor-react)   | `<ConfigEditor value onChange />`, a ref for the instance API |
| [`@pngwasi/config-editor-vue`](https://www.npmjs.com/package/@pngwasi/config-editor-vue)       | `<ConfigEditor v-model @save />`, a template ref              |
| [`@pngwasi/config-editor-svelte`](https://www.npmjs.com/package/@pngwasi/config-editor-svelte) | `<ConfigEditor bind:value onSave />`, exported functions      |

For another framework (Angular, Solid, Lit, …), `EditorBinding` is what
those components do, written once: feed it the component's props and it
decides what each change means. Settings and most props apply live (the
caret, undo history and folds stay); `language`, `schema`, `fileName`,
`editorClassName` and the `secrets` patterns re-mount the editor, carrying
the document, its baseline and the caret over; the bound value is kept in step so a late echo never rolls the
document back (an edit the framework passes back in late is told apart
from a value the app sets, such as a reset or a loaded file); a mount
that failed on a bad prop is retried when a prop changes; the WebAssembly
loads once (`loadConfigEditor`); and `handle` is the instance API
(`ConfigEditorHandle`: `value`, `setValue`, `reveal`, `save`, `isDirty`, …).
Each prop that changes reaches the editor through `update`: a setting no
longer passed returns to its default.

```ts
import { EditorBinding } from "@pngwasi/config-editor/binding";

const binding = new EditorBinding("my-framework-config-editor");
// When the component's element exists (props named as React's:
// `value`, `onChange`, `editorClassName`, every setting, …):
binding.attach(element, {
  value,
  onChange: (text) => emit(text),
  language: "yaml",
});
// After every change of the props (unchanged ones cost nothing):
binding.update({
  value,
  onChange: (text) => emit(text),
  language: "yaml",
  theme,
});
// The instance API, for the component's users:
binding.handle.reveal("/web/listen");
// On unmount: the editor is destroyed and freed.
binding.detach();
```

<details>
<summary>DocumentsBinding, unmounting from a callback, SETTING_TYPES, and binding from a vanilla app</summary>

`DocumentsBinding` does the same for a `ConfigDocuments` view (`label`,
`viewClassName` and whether `onClose` is set re-mount; a new `activeId`
shows that document). A component unmounted from inside a callback (state
set in `onChange` that hides it) is destroyed when the framework commits
that, after the callback returns: React and Svelte do so asynchronously,
so the callbacks the same edit fires next (`onChangeSummary`,
`onDirtyChange`) still arrive first; after `detach()` none does. `SETTING_TYPES` is every setting's key and the
JavaScript type of its values, generated from the Rust struct: plain data,
readable before the WebAssembly loads (Vue declares its props from it, and
`Object.keys(SETTING_TYPES)` picks the settings out of a component's props).

A vanilla app can use `EditorBinding` too: it is plain JavaScript. It
pays off when the app re-renders from state and passes every prop each
time (a web component, Lit, a store of your own); an app that creates an
editor and changes an option now and then is simpler with `new
ConfigEditor` and `update()`.

</details>

## Keyboard

**Writing**

- `Ctrl+Space`: suggestions; `Enter` or `Tab` accepts one
- `Tab` / `Shift+Tab`: indent / outdent
- `Ctrl+/`: comment or uncomment the lines
- `Ctrl+Enter` / `Ctrl+Shift+Enter`: new line below / above
- `Alt+↑` / `Alt+↓`: move the line or selected lines up / down (a folded
  line with its hidden lines, staying folded; past a folded block, over all
  of it)
- `Shift+Alt+↑` / `Shift+Alt+↓`: copy the line or selected lines up / down
- `Ctrl+Shift+K`: delete the line or selected lines (the caret keeps its column)
- `Ctrl+]` / `Ctrl+[`: indent / outdent the line or selected lines
- `Ctrl+C` / `Ctrl+X` with nothing selected: copy / cut the whole line
  (pasting it inserts above)
- `Ctrl+Z` / `Ctrl+Y`: undo / redo (the last 500 steps, fewer in a large text: they keep up to 64 MB of earlier texts)
- `Ctrl+S`: save (with `onSave`)
- `Escape`: close what is open (suggestions, a preview, a widget)

**Finding your way**

- `Ctrl+F` / `Ctrl+H`: find / find and replace; `Enter` and `Shift+Enter` (or `F3` / `Shift+F3`) next / previous match
- `Ctrl+G`: go to a line (`line` or `line:column`)
- `F8` / `Shift+F8`: next / previous problem
- `Alt+Enter`: open the documentation of the problem at the caret (a problem with a docs link), else the hover card's Documentation link
- `F6`: while the "file changed outside the editor" notice is open, move to its choices and back
- `Ctrl+K Ctrl+I`: show the schema docs of the key at the caret (read to
  screen readers, its Documentation link too; `Escape` closes them; in
  `Ctrl+M` mode `Tab` moves into its links); on a touch screen, a tap on a
  key shows them
- `Ctrl+K Ctrl+I` again: focus those docs (or the problems' tip shown at
  the caret): the arrows, `PageUp` / `PageDown`, `Home` and `End` scroll
  them, `Tab` reaches their links, `Enter` or `Alt+Enter` opens their docs
  link, `Escape` returns to the text
- `PageDown` / `PageUp` (with `Shift`, selecting): move the caret a page in
  the text; the host page never scrolls

**Changes against the baseline**

- `Alt+F5` / `Shift+Alt+F5`: next / previous change (announced, as `F8` is)
- `Alt+F3` / `Shift+Alt+F3`: the same, opening its preview, where `Enter` or `R` reverts the change

**Folding**

- `Ctrl+Shift+[` / `Ctrl+Shift+]`: fold / unfold at the caret
- `Ctrl+K Ctrl+0` / `Ctrl+K Ctrl+J`: fold / unfold everything

**Documents** (a `ConfigDocuments` view)

- `Alt+PageDown` / `Alt+PageUp`: next / previous document

**Leaving the editor**

- `Ctrl+M`: toggle "Tab moves focus"
- `Escape`, then `Tab` / `Shift+Tab`: leave the editor once

`Cmd` works in place of `Ctrl` on macOS.

### Leaving the editor and screen readers

`Tab` indents while editing. `Ctrl+M` switches `Tab` and `Shift+Tab` to
moving the focus out of the editor (announced, and shown in the footer as
"Tab moves focus" until `Ctrl+M` again or a click on it); right after an
`Escape` that had nothing to close, one `Tab` or `Shift+Tab` leaves. `Tab`
still accepts an open suggestion. The textarea's `aria-describedby` holds
what is at the caret: each problem's message (host ones with their source),
a read-only value's reason, a secret placeholder's hint; `F8` announces
"Error 2 of 5: …". On a touch screen a tap on underlined or read-only text
shows its message, and selections made with a long press or the handles are
followed. Nothing to configure:

```ts
const editor = new ConfigEditor(element, { value, language: "yaml" });
// Tab out with Ctrl+M or Escape, Tab; screen readers hear the caret's problem.
```

### Folding

Lines that start a block (a YAML mapping, a JSON object, a TOML table, an
HCL block, an XML element, a run of comments, a multi-line string) get a
chevron in the gutter, shown while the pointer is over it and always on a
folded line, as in VS Code. A folded line shows `…`; clicking it or the
chevron opens it.

```ts
editor.foldAll();
editor.unfold("/web/listen"); // show one field; false when nothing was folded around it
editor.fold(["clients"]); // fold the block its key opens; false when there is none
```

<details>
<summary>How folds behave through edits and caret moves</summary>

Folding only changes the view: `value`, `onChange`, undo and saving see the
whole text, and copying a selection across a fold copies the hidden lines.
An edit that touches a fold (typing at the end of a folded line, Backspace
at the start of the next one, a paste over a selection reaching into it)
opens it first, and whatever moves the caret into a fold (find, go to line,
`F8`, `reveal`, a click on a problem) opens it. A folded block hiding an
error or changes marks its line. The textarea holds the text on screen, so
read the document from `value`, not from the element.

</details>

## Saving and unsaved edits

Nothing is saved by itself: `Ctrl+S` / `Cmd+S` in the editor (and
`editor.save()`) calls `onSave` with the document in `language`. Text that
does not parse is never saved, in any language: nothing is sent, `save()`
returns `false`, and the caret goes to the syntax error (`onChange` still
hears every edit). Text written beside a secret placeholder the host gave as
a field's whole value waits the same way, its message shown. While editing as another language, an edit still waiting
out the typing pause is converted first; when it cannot be written in
`language` (invalid, or a `null` for TOML), the save waits the same way.
The document is dirty while it differs from `baseline` (the document at
mount when no `baseline` is given). Embedding notes (what is fixed at mount,
which text to send, flushing before your own Save button) are in the crate's
[Embedding](https://crates.io/crates/config-editor-leptos#embedding)
section: `editor.value` is the document in `language`, `editor.text` the
text on screen.

<details>
<summary>Line endings (CRLF, LF and CR, mixed in one text)</summary>

Line endings are kept: when the host's text ends its lines with CRLF,
`onChange` and `onSave` receive CRLF too (the editor works on LF text inside,
and dirty state and the diff compare line endings as LF). Each line keeps its
own: in a text mixing CRLF, LF and old Mac CR endings, an untouched line ends
as it did, an edited line keeps its ending, and a new line takes the ending
of the line above (the text's most common one when there is none). A lone CR
that the language does not read as a line break (TOML, HCL) is shown as the
syntax error its reader gives (as `parse` reports it), until that line is
edited. Every position the tools give or take (`validate`, `rangeOf`,
`pathAt`, `parse` errors) counts LF, CRLF and a lone CR each as one line
break, as the editor does.

</details>

```js
const editor = new ConfigEditor(element, {
  value: loaded,
  baseline: loaded,
  onSave: async (text) => {
    await putConfig(text);
    editor.update({ baseline: text }); // saved: nothing is unsaved any more
  },
  onDirtyChange: (dirty) => (saveButton.disabled = !dirty),
  onBaselineConflict: (event) => console.log("baseline conflict:", event),
});
// Saved outside the editor: the baseline moves on.
events.on("config-changed", (text) => editor.update({ baseline: text }));
```

A new baseline while the user has unsaved edits opens a notice in the editor,
"This file changed outside the editor." (announced, `role="alert"`; it leaves the
focus in the text, and `F6` moves it to **Keep mine** and back): **Reload** makes the new baseline the document (one undo step,
reported through `onChange`), **Keep mine** (or `Escape`) keeps the document
and diffs it against the new baseline. Without unsaved edits, or when the new
baseline is the document or a text the editor sent (`onChange`, `onSave`: a
save coming back while the user typed on), it is taken silently. A new
`value` keeps the caret and the scroll position.

## Several documents

`ConfigDocuments` shows several files, a main config and its drop-ins, as
VS Code-style tabs over one editor per document, each with its own language
(from `fileName` unless given, registered languages included, as
`languageForFile` reads it; a file no language claims opens in the default
language, with a console warning), baseline, read-only state, `editAs`,
schema (documents with the same schema text share one compiled schema; one
that does not compile throws, as it does for a `ConfigEditor`) and,
optionally, settings, host problems and read-only fields.

```js
import { ConfigDocuments, loadConfigEditor } from "@pngwasi/config-editor";

await loadConfigEditor();
let files = [
  // The main config stays open: no close button.
  {
    id: "main",
    fileName: "config.yaml",
    value: mainText,
    schema,
    closable: false,
  },
  { id: "limits", fileName: "conf.d/limits.toml", value: limitsText, schema },
  {
    id: "clients",
    fileName: "conf.d/clients.json",
    value: clientsText,
    schema,
    readOnly: true,
  },
];
const update = (id, change) => {
  files = files.map((file) => (file.id === id ? { ...file, ...change } : file));
  view.update({ documents: files });
};
const view = new ConfigDocuments(document.getElementById("files"), {
  documents: files,
  onChange: (id, value) => update(id, { value }),
  onSave: async (id, value, language) => {
    await putFile(id, value);
    update(id, { baseline: value }); // nothing unsaved any more
  },
  onClose: ({ id, dirty }) => {
    if (dirty && !confirm(`Discard unsaved changes to ${id}?`)) return;
    files = files.filter((file) => file.id !== id);
    view.update({ documents: files });
  },
  // The "+" after the tabs: you decide what a new file is.
  onAdd: () => {
    files = [...files, newDropIn(files)];
    view.update({ documents: files });
  },
  onSelect: (id) => console.log("showing", id),
});
view.saveAll(); // saves the dirty documents; returns their ids
view.select("limits"); // shows it and focuses its editor; false for an id not listed
view.activeId; // "limits"
view.isDirty("main");
```

- You own the list: `update({ documents })` adds, removes, renames and reorders. A
  document keeps its editor while its `id` stays listed (a new `value`,
  `baseline`, `fileName`, `readOnly` or `editAs` updates it in place; a new
  `language` or `schema` mounts a new editor). Handing the whole list back
  after every edit is cheap: a `value`, `baseline` or `schema` that is the
  same string as last time (the one `onChange` gave you included) is not
  read again, so only the edited document costs anything. A listed `value`
  always applies (a discard, a reload); one that is the document already
  changes nothing. A host that lists every `onChange` text back late, in
  any order (a store that re-renders after a delay), creates the view with
  `echoes: true`: a document's `value` its editor gave that has not come
  back yet is then its echo and keeps what the editor holds, as in
  [Hosts that echo](#hosts-that-echo) (the ranges listed with it are moved
  to the document; a `value` listed as it was listed last is no news, as
  an unchanged prop); `view.replaceValue(id, text)` goes back to such a
  text (list the document with it from then on; `false` for an id not
  listed); it changes only that document, the others keep their edits.
- `update` and `replaceValue` (on both classes) first apply input the
  browser sent in this task, which calls `onChange`: what you set from
  there (a list, a value, locks, callbacks) is newer than the call that
  started, and that call leaves those keys as you set them, applying only
  its others.
- `onProblems(id, problems)` gives each document's editor's own problems
  (as `ConfigEditor`'s `onProblems`): once its editor mounts, when the
  document is first shown, then each time they change.
- Nothing closes by itself: `onClose({ id, dirty })` asks you (close button,
  middle click, `Delete` on a tab). Only closable documents ask: a document
  with `closable: false` (switches live) stays open, with no close button,
  and a middle click or `Delete` on its tab does nothing. Without `onClose`,
  no tab has a close button.
- Adding is yours too: with `onAdd`, a "+" button follows the last tab
  (outside the tab list, after it in the `Tab` order; named and titled by
  the `addDocument` text, "New file"). It only calls `onAdd()`: you decide
  what a new document is and list it, and the document you add in answer is
  selected, its editor focused without scrolling the page. In answer: the
  first one added within 5 seconds of the click, before the focus moves to
  something outside the view (a dialog of yours: select the document with
  `select(id)` then), before another tab is picked and before the user
  clicks or types anywhere else in the view (in a document's editor). One
  added later (by you on your own) is listed, not selected. When the tabs
  overflow, the "+" stays at the end of the strip. No key adds a document
  (`Ctrl+T` and `Ctrl+N` belong to the browser). `onClose` and `onAdd`
  cannot be added by `update` to a view mounted without them.
- Switching keeps each document's caret, selection, scroll position, undo
  history, folds and find widget: an editor mounts the first time its
  document is shown, then stays (hidden) while it is open. With ten
  ~2000-line documents all shown, the view used about 21 MB of wasm memory
  (1.5 to 1.9 MB per editor) and 4 MB of JS heap. Closing a document gives
  its editor's memory back, as `destroy()` does for a `ConfigEditor` (all
  but under 1 KB).
- Keys: `←`/`→`, `Home`/`End` in the tab list (a WAI-ARIA `tablist`),
  `Enter` into the editor (a typed character goes there too, typed at its
  caret), `Delete` to close a closable one, `Tab` on to
  the "+"; `Alt+PageDown`/`Alt+PageUp`
  switch documents from anywhere in the view (browsers keep `Ctrl+Tab` and
  `Ctrl+PageDown`). `Ctrl+S` saves the document being edited.
- Settings: the options take every [setting](#settings), for all the
  editors and the tabs (`theme`, `colors` with the `tab-*` tokens, see
  [Styling](#styling); `styled` injects the stylesheet once for the view);
  `settings` reads them and `update` switches them live, merging key by
  key. A document's own
  `settings` go over them for its editor: the keys it gives replace the
  view's, the rest follow the view's.
- Per document, as on `ConfigEditor`: `hostDiagnostics`, `readOnlyPaths`,
  `readOnlyRanges` (updated in place by `update({ documents })`) and `secrets` (read
  when its editor mounts).
- What each document shows: `onEditAsChange(id, language)` says the
  language on screen, once when its editor mounts, then each time it changes
  (a switch made, also one that waited until the text could convert; undo
  or redo across a switch). A switch still waiting changes nothing on screen
  and is not reported. `onSyncChange(id, inSync)` says whether the text on
  screen is written back to the document's `value` (as `ConfigEditor`'s
  `onSyncChange`): once when its editor mounts, then on each change. Neither
  repeats what it said last.
- Asking for a language again: a list with the same `editAs` changes
  nothing (and frameworks do not send an unchanged prop), so after the user
  undid a switch, `view.editAs(id, language)` asks for it again (the
  document's own language goes back; a switch the text cannot make yet
  waits). Keep the document's `editAs` the same in your list, or the next
  `update({ documents })` switches back.
- Opening from the text the user had: `editAsText` (with `editAs`) opens the
  document's editor with that text on screen in the `editAs` language, in
  place of `value` converted: what you kept from `view.editor(id).text`
  before the view went away. `value` and `baseline` stay yours; the text
  converts back like an edit, so `onChange` hears it once it converts
  (comments of `value` kept), and until then `onSyncChange` says `false`.
  Undo starts at it. Read when the editor mounts.
- A reason for a read-only document: `readOnlyReason` (with `readOnly`,
  updates live) shows in the tab's tooltip and accessible description, in
  the "Cannot edit in a read-only editor" notice when the user tries to type
  and in the text area's accessible description.

```js
const view = new ConfigDocuments(element, {
  documents: [
    // Back on the page: the TOML the user had, not valid yet.
    {
      id: "main",
      fileName: "config.yaml",
      value: saved,
      editAs: "toml",
      editAsText: kept,
    },
    {
      id: "clients",
      fileName: "conf.d/clients.json",
      value: clients,
      readOnly: true,
      readOnlyReason: "Managed by the cluster operator",
    },
  ],
  onEditAsChange: (id, language) => showLanguage(id, language),
  onSyncChange: (id, inSync) => (saveButton.disabled = !inSync),
});
view.editAs("main", "toml"); // after an undo across the switch: ask again
```

- `editor(id)` returns that document's editor, an
  [`EditorView`](#instance-members) like a `ConfigEditor` (`null` for an id
  not listed): its `value` (the host's, then each edit), `settings` (its
  own over the view's) and `unifiedDiff` against its baseline, paths,
  folds, `focus`, `save` and `isDirty`. An editor mounts the first time
  its document is shown: until then its queries answer `null` or `false`
  (`text` answers the document's `value`). Once its document is removed,
  it stays with it and answers `null` (`value`, `text`, `currentPath`) or
  `false` (`save()`, `isDirty`, `reveal`, …), also when a new document
  with the same id is listed: ask `editor(id)` again for that one.
- `update(options)` changes the view live, with `ConfigEditor`'s rules
  (left out or `undefined` stays, `null` removes a callback or returns a
  setting to its default, every key checked before any applies): the
  `documents`, `activeId` (shows that document, leaving the focus where it
  is), the settings and the callbacks. `className`, `label` (the tab
  list's name) and `echoes` are fixed and throw, and so does an `onClose` for a view
  created without one (its tabs have no close buttons); `activeId` given to
  the constructor is the document shown first. An `activeId` not listed
  throws there, as a wrong option does; `select(id)` is a request and
  answers `false` for an id not listed. Also `destroy()` and `free()`, as
  for a `ConfigEditor`: an edit still converting is sent to `onChange` first.

```js
files = files.map((file) =>
  file.id === "limits"
    ? {
        ...file,
        settings: { folding: false },
        readOnlyPaths: ["/limits/max_pending"],
      }
    : file,
);
view.update({ documents: files, showFooter: true }); // every editor; limits keeps folding off
view.update({ activeId: "limits" }); // shows it, the focus stays where it is
view.editor("main").reveal("/public_address");
```

## Reviewing changes

The gutter marks added, modified and removed lines against `baseline`; a
marker opens the lines before and now with **Revert** (one undo step,
reported through `onChange`). For your own review screen:

```js
const editor = new ConfigEditor(element, {
  value: saved,
  onChangeSummary: ({ hunks }) => (reviewButton.disabled = hunks === 0),
});
reviewButton.onclick = () => showPatch(editor.unifiedDiff());
// After saving: editor.update({ baseline: editor.value }) clears the changes.
```

The change summary is read live from `onChangeSummary`, or computed for any
two texts with the free `changeSummary(baseline, current)` of
`@pngwasi/config-editor/tools`; the editor has no method for it, by design.

## Custom languages

`registerLanguage` adds a language written in JavaScript. A `.env` file
(`KEY=value` lines) needs only `parse`; its `separators` also give it schema
completion and hover:

```js
import { registerLanguage, ConfigEditor } from "@pngwasi/config-editor";
import { convert } from "@pngwasi/config-editor/tools";

const env = registerLanguage({
  label: "ENV", // its name is the label lowercased: "env"
  icon: "E", // optional, toolbar icon: one or two letters or digits
  extensions: ["env"], // optional: also names the language (language: "env"), as "yml" does
  parse(text) {
    // Return a JSON value, or throw { message, line?, column? }.
    return Object.fromEntries(
      text
        .split("\n")
        .filter((line) => line.trim() && !line.startsWith("#"))
        .map((line) => line.split("=")),
    );
  },
  serialize(value) {
    // Optional: text for a value, for conversion and editAs.
    return (
      Object.entries(value)
        .map(([key, item]) => `${key}=${item}`)
        .join("\n") + "\n"
    );
  },
  keySpan(text, pointer) {
    // Optional: { line, startColumn, endColumn } (one-based) of the key at a
    // JSON pointer, so schema problems are underlined. Without it, or where
    // it returns null or undefined, a flat language (separators, no
    // contextAt) finds keys on their lines.
    return null;
  },
  valueSpan(text, pointer) {
    return null; // optional, like keySpan for the value; Edit-as saves edit values in place with it
  },
  highlight(line) {
    // Optional: [{ start, end, kind }] ranges of one line, as JavaScript
    // string indices (what `indexOf` gives); kind is
    // "key" | "string" | "value" | "literal" | "comment" | "punctuation" |
    // "reference" (an anchor, an alias) | "interpolation" (`${…}` in a string).
  },
  // Optional syntax, as in VS Code's language configuration:
  comments: { lineComment: "#" }, // Ctrl+/ (a list: the first is written); blockComment: ["/*", "*/"] is unwrapped
  brackets: [
    ["{", "}"],
    ["[", "]"],
  ], // they auto-close, and Enter between them opens a line
  separators: ["="], // key renames, Tab through empty values, schema completion and hover
});

new ConfigEditor(element, { language: env, schema });
convert("env", "listen=7835\n", "yaml").text; // 'listen: "7835"\n'
```

A registered language gets schema validation (placed with `keySpan` and
`valueSpan`), conversion to and from every other language, and Edit-as;
`comments`, `brackets` and `separators` add comment toggling,
auto-closed brackets and bracket-aware Enter, and key/value editing. A language without `serialize`
reads documents but cannot write them, so it is never edited as another
language nor another as it: `editAs` either way throws
`editAs: <label> cannot write documents`.

Read-only fields (`readOnly` in the schema, `readOnlyPaths`) hold in every
registered language. Where the language says where a field is written
(`keySpan` / `valueSpan`, or a flat language's lines), its value is muted,
the gutter shows a lock, and an edit on it is refused as it is made. A
language that cannot say still keeps the field: an edit after which the text
parses with the field's value changed is refused (while the text does not
parse, the edit that makes it parse again is checked). Secret placeholders
are found by their text and need nothing from the language. Built-in names cannot be
replaced, and a registered language cannot take another's name or extension
(`registerLanguage` throws); registering the same label again replaces its
definition, in the editors already open too, and takes no more memory (a
component may register its language each time it mounts). Its name is the label lowercased (`"Énv"` → `"énv"`).

### Schema completion and hover

Suggestions and hover docs run through the same engine as the built-in
languages', in one of two ways.

**Flat languages (`separators` only).** A language with one `KEY=value`
entry per line and no nesting, like the `.env` one above, gets them from its
`separators` with no more code:

- before a line's first separator, the schema's top-level keys are
  suggested, those already written on other lines left out;
- after it, the key's values (an `enum`, `true` / `false`, a `const`), a
  string written bare when it reads back as written (`LOG_LEVEL=debug`);
- accepting a key writes it and the first separator, the caret after it:
  `KEY=`, or with the spacing `writtenSeparator` gives (`writtenSeparator:
": "` writes `key: `);
- hovering a key shows its schema docs;
- nothing is suggested on a line comment.

**Nested languages (callbacks).** A language with objects inside objects
says where the caret is with three optional callbacks. Each one replaces its
part of the flat default; the rest (the schema at a path, filtering and
ranking, the popup, value docs, required fields, `oneOf` choices) is the
editor's, as for the built-in languages.

| Callback                  | When it is called                                                                                           | Returns                                                                                                                                                                                                                                                                         | `null` / missing                                                         |
| ------------------------- | ----------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| `contextAt(text, offset)` | As suggestions open or update, and to place a hovered key; `offset` is the caret's string index             | `{ path, at, prefix }`. `at: "key"`: a key is typed in the object at `path`. `at: "value"`: the caret is in the value of the key that ends `path` (a path ending in an index: an array item). `prefix`: what is typed before the caret, which suggestions filter by and replace | `null`: nothing to suggest there. Missing: the flat default              |
| `insertKey(key, kind)`    | For each key suggested; `kind` is what the schema expects as its value: `"scalar"`, `"object"` or `"array"` | `{ text, caret }`: the text written, and the caret's string index in it. Lines after the first are indented as the caret's line                                                                                                                                                 | `null`: the key is not suggested. Missing: the key and first separator   |
| `keyAt(text, offset)`     | As the mouse rests on the text; `offset` is the string index under it                                       | `{ key, path, start }`: the key's name, its full path (ending with it), and the string index where it starts                                                                                                                                                                    | `null`: no key there. Missing: the key before the line's first separator |

Paths are keys and array indices, as `["server", "tls"]`. Keys already
written in the object at the caret are left out: the editor reads the
document through `parse` without the caret's line (while that does not
parse, none are left out). Values are written as JSON. A result of the wrong
shape is warned about in the console (once) and read as `null`; a callback
that throws is logged and read as `null` too.

<details>
<summary>A sketch: completion and hover for an HJSON-like language</summary>

This is what lets formats the editor does not ship (HJSON, KDL, UCL, …) get
suggestions: `parse` comes from the host's own parser, and the callbacks
from what it knows of positions. A sketch for an HJSON-like language
(`key: value` lines, `key: {` … `}` blocks), where `scan(text)` stands for
the host's line scanner (for each line: the keys of the objects open there,
its key and the key's column):

```js
registerLanguage({
  label: "HJSON",
  parse: (text) => Hjson.parse(text),
  separators: [":"],
  writtenSeparator: ": ",
  contextAt(text, offset) {
    const line = scan(text).at(text.slice(0, offset).split("\n").length - 1);
    const before = text.slice(text.lastIndexOf("\n", offset - 1) + 1, offset);
    const colon = before.indexOf(":");
    if (colon < 0) return { path: line.path, at: "key", prefix: before.trim() };
    const key = before.slice(0, colon).trim();
    return {
      path: [...line.path, key],
      at: "value",
      prefix: before.slice(colon + 1).trimStart(),
    };
  },
  insertKey(key, kind) {
    if (kind === "object")
      return { text: `${key}: {\n  \n}`, caret: key.length + 6 };
    return { text: `${key}: `, caret: key.length + 2 };
  },
  keyAt(text, offset) {
    const line = scan(text).at(text.slice(0, offset).split("\n").length - 1);
    const start = text.lastIndexOf("\n", offset - 1) + 1 + line.keyColumn;
    if (!line.key || offset < start || offset >= start + line.key.length)
      return null;
    return { key: line.key, path: [...line.path, line.key], start };
  },
});
```

</details>

### Strings and comments over lines

`highlight` colors one line at a time. For what spans lines (HJSON's `'''`
strings, `/* */` comments), `multilineRanges(text)` returns
`[{ start, end, kind }]`: string indices, and `kind` `"string"` or
`"comment"`. The lines after a range's first are colored as its kind up to
`end`, and the rest of its last line by `highlight`; nothing is suggested
inside, a string's lines stay text for Enter, and a line comment marker
inside a range starts no comment.

```js
multilineRanges(text) {
  return [...text.matchAll(/'''[\s\S]*?(?:'''|$)|\/\*[\s\S]*?(?:\*\/|$)/g)].map((match) => ({
    start: match.index,
    end: match.index + match[0].length,
    kind: match[0].startsWith("/*") ? "comment" : "string",
  }));
},
```

### Edit-as with a custom language

<details>
<summary>How a save edits the stored text in place</summary>

A document stored in a custom language and edited as another (`editAs`)
is saved back by editing its own text, not by writing it anew: a changed
value is rewritten where `valueSpan` places it, and a key added, removed or
renamed changes just its lines (compared line by line with what `serialize`
writes, so this works without spans too). Comments, blank lines and the
entries left alone stay as written. The whole document comes from
`serialize` only when the stored text did not parse before the edit, or no
such patch reads back (through `parse`) as the edited data. Highlighting
stays per line (`highlight`), with `multilineRanges` for what spans lines.
The save reads your syntax only from what you declare (`comments`,
`brackets`, `separators`, strings in `"…"` or `'…'`): no built-in
language's characters are assumed. Declare how maps and lists are written
on one line, and a save edits them where they are, keeping the line's
spacing and comment:

| Member             | Example      | Says                                                                                  |
| ------------------ | ------------ | ------------------------------------------------------------------------------------- |
| `mapBrackets`      | `["{", "}"]` | the brackets around a map written on one line                                         |
| `listBrackets`     | `["[", "]"]` | the brackets around a list written on one line                                        |
| `itemSeparator`    | `","`        | what separates their items, one character                                             |
| `itemMarker`       | `"-"`        | what starts a list item on lines of its own: a save moves it with its lines, comments |
| `literalQuotes`    | `["'"]`      | the quotes (of `"` and `'`) a backslash does not escape in                            |
| `writtenSeparator` | `": "`       | one of `separators` with the spacing a new entry writes                               |

A member of the wrong shape makes `registerLanguage` throw.

</details>

## Themes

The `theme` setting takes a slug; `themes()` returns them all, each with
its display name and whether it is dark.

<details>
<summary>The eleven themes</summary>

| Slug              | Name                       | Look  |
| ----------------- | -------------------------- | ----- |
| `vscode-dark`     | VS Code Dark (the default) | dark  |
| `vscode-light`    | VS Code Light              | light |
| `obsidian`        | Obsidian Dark              | dark  |
| `obsidian-light`  | Obsidian Light             | light |
| `dracula`         | Dracula                    | dark  |
| `nord`            | Nord                       | dark  |
| `atom-one-dark`   | Atom One Dark              | dark  |
| `github-light`    | GitHub Light               | light |
| `github-dark`     | GitHub Dark                | dark  |
| `solarized-light` | Solarized Light            | light |
| `vira-inspired`   | Vira-inspired (original)   | dark  |

</details>

## Styling

The editor injects its own stylesheet. Every theme color is a CSS variable
on the `.config-editor` root, and the bundled themes set them with zero
specificity, so a plain rule in your stylesheet overrides any theme:

```css
.config-editor {
  --config-editor-bg: #101418;
  --config-editor-key: #ffcc66;
}
```

With `styled: false` the editor injects nothing, and its markup
(`config-editor__*` classes) is yours to style; the package ships the preset
as `@pngwasi/config-editor/preset.css` to serve or adapt (`presetCss()`
returns the same text).

### Size

The editor sizes itself, not by its element's height: its text area grows
with the text from `--config-editor-min-height` (240px, less in a window too
short for it) to `--config-editor-max-height` (80% of the window, less in a
short one), then scrolls; the toolbar, tab bar and footer come on top.
`--config-editor-height` (a length, such as `400px`) fixes the text area's
height. To fill an element of a set height, give the editor `height: 100%`:
the text area takes what the toolbar and footer leave, however short or tall
(neither the minimum nor the 80% applies, unless you set the custom
property).

```css
#config {
  height: 60vh;
}
#config .config-editor {
  height: 100%;
}
/* Or a fixed text area: */
.config-editor {
  --config-editor-height: 400px;
}
```

`ConfigDocuments` sizes itself the same way: the tab strip, then the shown
editor, as tall as its text. To fill an element of a set height, give the
view `height: 100%`: the strip keeps its height and the shown editor (its
text area, between its toolbar and footer) takes the rest, with one
document or many.

```css
#files {
  height: 60vh;
}
#files .config-editor-documents {
  height: 100%;
}
```

### Strict Content-Security-Policy

The editor runs under a policy without `'unsafe-inline'`:

```text
Content-Security-Policy: default-src 'self'; style-src 'self'; script-src 'self' 'wasm-unsafe-eval'
```

- Set `styled: false` (on `ConfigEditor`, or once on `ConfigDocuments`): the
  injected `<style>` is inline, so the policy refuses it.
- Serve `preset.css` as a file and link it: one file with the base styles,
  every theme and the tabs of `ConfigDocuments`, so themes and `colors`
  switch as they do with the injected stylesheet. With a bundler, import it
  (`import "@pngwasi/config-editor/preset.css";`) so it lands in your CSS
  bundle; without one, copy `node_modules/@pngwasi/config-editor/preset.css`
  next to your page and add `<link rel="stylesheet" href="/preset.css">`.
- `'wasm-unsafe-eval'` lets the page compile the WebAssembly; nothing else
  in the editor evaluates code.

Everything else holds as it is: the editor sets positions, sizes and
`colors` through the CSSOM (`style.setProperty`), which the policy allows,
never as `style` attributes, and its icons are inline SVG, not `data:`
images. The React, Vue and Svelte components work the same way (with Vue,
write render functions or precompiled templates: compiling a template in the
browser needs `'unsafe-eval'`). An end-to-end test runs every feature under
this policy, in plain JavaScript and in each framework.

<details>
<summary>The color tokens: the colors setting, the diff, the footer counts and the tabs</summary>

The `colors` setting sets the variables per editor, by token: its type is
`ThemeColorMap`, keyed by `ThemeColorToken` (both exported), which lists
every token. The diff markers use `added`,
`modified` and `removed` (which follows `error` unless set), and the
footer's change count `status-changes`; its problem counts `status-error`,
`status-warning`, `status-info` and `status-hint`, each in its own color.

The tabs of `ConfigDocuments` use `tab-bar` (the strip), `tab-bg` /
`tab-active-bg`, `tab-text` / `tab-active-text`, `tab-border`,
`tab-active-border` (the line over the active tab) and `tab-dirty` (the
unsaved-changes dot); they follow `panel`, `bg`, `muted`, `text`, `border`
and `accent` unless set, as variables on the `.config-editor-documents` root
or through its `colors` setting.

</details>

## License

MIT. See [LICENSE](LICENSE).
