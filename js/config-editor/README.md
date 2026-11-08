# @pngwasi/config-editor

A schema-aware config editor for the browser: YAML, JSON, TOML, HCL and XML,
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
- [`@pngwasi/config-editor/tools`](#pngwasiconfig-editortools)
- [`@pngwasi/config-editor/binding`](#pngwasiconfig-editorbinding)
- [Keyboard](#keyboard)
  - [Leaving the editor and screen readers](#leaving-the-editor-and-screen-readers)
  - [Folding](#folding)
- [Saving and unsaved edits](#saving-and-unsaved-edits)
- [Several documents](#several-documents)
- [Reviewing changes](#reviewing-changes)
- [Custom languages](#custom-languages)
- [Themes](#themes)
- [Styling](#styling)
- [License](#license)
<!-- /toc -->

## Install

```sh
npm install @pngwasi/config-editor
```

Browsers only (ES modules + WebAssembly). The `.wasm` file is about 2 MB
gzipped and loads once per page. It is built with every Cargo feature of
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

| Option               | Type                                        | Default          | Purpose                                                                                                                                                                                                                                                                                                                                                                                                          |
| -------------------- | ------------------------------------------- | ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `language`           | `string`                                    | `"yaml"`         | Language of the document: `"yaml"`, `"json"`, `"toml"`, `"hcl"`, `"xml"` (any case, or a file extension such as `"yml"` or `"tf"`), or a registered one. `value`, `baseline` and `onChange` are always in it.                                                                                                                                                                                                    |
| `value`              | `string`                                    | `""`             | The document.                                                                                                                                                                                                                                                                                                                                                                                                    |
| `editAs`             | `string`                                    | —                | Show and edit the document in another language; it is saved back in `language`, keeping its comments.                                                                                                                                                                                                                                                                                                            |
| `schema`             | `string`                                    | —                | JSON Schema as JSON text: completion, hover docs and validation.                                                                                                                                                                                                                                                                                                                                                 |
| `baseline`           | `string`                                    | `value`          | Text the diff gutter compares against: the version last saved or loaded. See [Saving](#saving-and-unsaved-edits).                                                                                                                                                                                                                                                                                                |
| `readOnly`           | `boolean`                                   | `false`          | View mode: navigation, selection, diagnostics and hover keep working.                                                                                                                                                                                                                                                                                                                                            |
| `fileName`           | `string`                                    | `untitled.<ext>` | Shown in the toolbar.                                                                                                                                                                                                                                                                                                                                                                                            |
| `className`          | `string`                                    | —                | Extra class on the editor root.                                                                                                                                                                                                                                                                                                                                                                                  |
| `onChange`           | `(value: string) => void`                   | —                | Called with the document after every edit (after each typing pause while editing as another language).                                                                                                                                                                                                                                                                                                           |
| `onEditAsChange`     | `(language: string) => void`                | —                | Called with the language on screen when an `editAs` switch completes, and when undo or redo steps back across one (a switch is an undo step). A switch waits while the document cannot convert yet (TOML has no `null`): until then the host's language stays on screen, edited and sent to `onChange` as it is, with the reason on the value in the way. Label the language shown from this, not from `editAs`. |
| `onChangeSummary`    | `(summary: ChangeSummary) => void`          | —                | Called with `{ added, removed, modified, hunks }`: the lines changed against `baseline` and the number of separate changes, on mount and whenever it changes.                                                                                                                                                                                                                                                    |
| `hostDiagnostics`    | `HostDiagnostic[]`                          | —                | Problems the host found (a server's validation), shown with the editor's own; see [Host diagnostics](#host-diagnostics).                                                                                                                                                                                                                                                                                         |
| `onPathChange`       | `(path: EditorPath) => void`                | —                | Called with the path of what the caret is on each time it changes (and once on mount), for a breadcrumb; see [Paths](#paths). The path is computed only while this is set.                                                                                                                                                                                                                                       |
| `onSchemaWarning`    | `(message: string) => void`                 | console warning  | Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile (keys it matches get no completions or hover docs).                                                                                                                                                                                                                                       |
| `readOnlyPaths`      | `FieldPath[]`                               | —                | Fields the user cannot change (each with everything inside it), on top of the schema's `readOnly` ones; see [Read-only fields](#read-only-fields).                                                                                                                                                                                                                                                               |
| `readOnlyRanges`     | `TextRangeInput[]`                          | —                | Stretches of the document (in `language`) the user cannot change: `{ start, end }` string indices or one-based `{ startLine, startColumn, endLine?, endColumn? }`.                                                                                                                                                                                                                                               |
| `secrets`            | `SecretOptions`                             | defaults         | `{ placeholder?, isPlaceholder?, hint?, referencePrefixes? }`: how secret placeholders are matched, described and completed; see [Secrets](#secrets).                                                                                                                                                                                                                                                            |
| `onSave`             | `(value: string, language: string) => void` | —                | Called on `Ctrl+S` / `Cmd+S` (the browser's dialog prevented) and `save()`, with the document in `language`. Nothing saves by itself. Without it, `Ctrl+S` is the browser's.                                                                                                                                                                                                                                     |
| `onDirtyChange`      | `(dirty: boolean) => void`                  | —                | Called with whether the document differs from `baseline`: once on mount, then each time that changes.                                                                                                                                                                                                                                                                                                            |
| `onBaselineConflict` | `(event: BaselineConflict) => void`         | —                | Called when a new `baseline` arrives under unsaved edits (`"opened"`) and when that is settled (`"reloaded"`, `"kept-mine"`, `"settled"`).                                                                                                                                                                                                                                                                       |

An unknown language, theme or color token, or an invalid schema, throws an
`Error` saying which.

`value`, `baseline`, `editAs`, `readOnly`, `hostDiagnostics`, the read-only
fields, the callbacks and every setting change live through
[`update`](#updateoptions). The other options (`language`, `schema`,
`fileName`, `className`, `secrets`) are what the editor is built from:
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
| `styled`                 | `true`              | Inject the preset stylesheet. Off, style the `config-editor__*` markup yourself (the preset is `PRESET_CSS`, `presetCss()` in JavaScript).                                                                                                                                         |
| `showToolbar`            | `false`             | Show the built-in toolbar: the file name, its language and the schema's state.                                                                                                                                                                                                     |
| `showFooter`             | `false`             | Show the built-in status footer: the suggestions hint, problem and change counts, and the line count.                                                                                                                                                                              |
| `showDiagnostics`        | `true`              | Show syntax and schema problems inline, in the footer and on hover (`F8` moves between them).                                                                                                                                                                                      |
| `showDiff`               | `true`              | Show line changes against the baseline in the gutter (`Alt+F5` / `Alt+F3` move between them). Off, the change summary stays at zero. In Rust, needs the `diff` Cargo feature.                                                                                                      |
| `completions`            | `true`              | Schema completions (`Ctrl+Space`).                                                                                                                                                                                                                                                 |
| `autoTriggerCompletions` | `true`              | Open suggestions while typing and after accepting a key. Off, only `Ctrl+Space` opens them.                                                                                                                                                                                        |
| `hoverDocs`              | `true`              | Show the schema's description, type, default and examples when hovering a key.                                                                                                                                                                                                     |
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
are never saved. Its texts are `editAsHostComments`, `editAsTypedComments`
and `editAsTypedCommentsNone`.

<!-- strings:start -->

<!-- Generated from crates/config-editor-leptos/src/strings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`. -->

<details>
<summary>Every text and its default (157 texts)</summary>

| Key                         | Default                                                                                                                            |                                                                                                                                                                                                  |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `suggestionsHint`           | `"{key} for suggestions"`                                                                                                          | The footer's suggestions hint; `{key}` is where the chord goes.                                                                                                                                  |
| `suggestionsKey`            | `"Ctrl+Space"`                                                                                                                     | The chord that opens suggestions, as the footer draws it: keys between `+`.                                                                                                                      |
| `noProblems`                | `"No errors"`                                                                                                                      | The footer's problem count when there are none.                                                                                                                                                  |
| `errorsOne`                 | `"{count} error"`                                                                                                                  | One error, in the footer's problem count.                                                                                                                                                        |
| `errorsOther`               | `"{count} errors"`                                                                                                                 | Several errors, in the footer's problem count.                                                                                                                                                   |
| `warningsOne`               | `"{count} warning"`                                                                                                                | One warning, in the footer's problem count.                                                                                                                                                      |
| `warningsOther`             | `"{count} warnings"`                                                                                                               | Several warnings, in the footer's problem count.                                                                                                                                                 |
| `infosOne`                  | `"{count} info"`                                                                                                                   | One info, in the footer's problem count.                                                                                                                                                         |
| `infosOther`                | `"{count} info"`                                                                                                                   | Several infos, in the footer's problem count.                                                                                                                                                    |
| `hintsOne`                  | `"{count} hint"`                                                                                                                   | One hint, in the footer's problem count.                                                                                                                                                         |
| `hintsOther`                | `"{count} hints"`                                                                                                                  | Several hints, in the footer's problem count.                                                                                                                                                    |
| `problemsHint`              | `"hover or focus underlined text"`                                                                                                 | After the problem count when every problem has its place.                                                                                                                                        |
| `problemsCut`               | `"only the first {count} are shown"`                                                                                               | After the problem count when the list was cut; `{count}` is how many are shown.                                                                                                                  |
| `problemsMore`              | `"Only the first {count} problems are shown; more follow."`                                                                        | The problem that says the list was cut, after `{count}` problems.                                                                                                                                |
| `changesOne`                | `"{count} change"`                                                                                                                 | The footer's change count against the baseline, for one change.                                                                                                                                  |
| `changesOther`              | `"{count} changes"`                                                                                                                | The footer's change count, for several changes.                                                                                                                                                  |
| `changeDetailsOne`          | `"{lines} line · Alt+F5 for the next"`                                                                                             | The change count's tooltip when one line changed; `{lines}` lists the added, modified and removed counts.                                                                                        |
| `changeDetailsOther`        | `"{lines} lines · Alt+F5 for the next"`                                                                                            | The change count's tooltip when several lines changed.                                                                                                                                           |
| `changedAdded`              | `"{count} added"`                                                                                                                  | Added lines, in the change count's tooltip.                                                                                                                                                      |
| `changedModified`           | `"{count} modified"`                                                                                                               | Modified lines, in the change count's tooltip.                                                                                                                                                   |
| `changedRemoved`            | `"{count} removed"`                                                                                                                | Removed lines, in the change count's tooltip.                                                                                                                                                    |
| `linesOne`                  | `"{count} line"`                                                                                                                   | The footer's line count, for one line.                                                                                                                                                           |
| `linesOther`                | `"{count} lines"`                                                                                                                  | The footer's line count, for several lines.                                                                                                                                                      |
| `tabFocus`                  | `"Tab moves focus"`                                                                                                                | The footer's indicator while Tab moves the focus.                                                                                                                                                |
| `tabFocusTitle`             | `"Tab moves focus (Ctrl+M to indent with Tab)"`                                                                                    | Its tooltip; clicking it turns the mode off.                                                                                                                                                     |
| `tabFocusOn`                | `"Tab moves focus: Tab and Shift+Tab now leave the editor. Ctrl+M to indent with Tab again."`                                      | Announced when `Ctrl+M` makes Tab move the focus.                                                                                                                                                |
| `tabFocusOff`               | `"Tab indents: Tab and Shift+Tab indent again. Ctrl+M to move focus with Tab."`                                                    | Announced when `Ctrl+M` makes Tab indent again.                                                                                                                                                  |
| `schemaConnected`           | `"Schema connected"`                                                                                                               | The toolbar's schema state.                                                                                                                                                                      |
| `editAsHostComments`        | `"The {host} file's comments and formatting are not shown while editing as {edit}; saving keeps them."`                            | The Edit-as notice when the host's file has comments, which the edited language does not show but saving keeps.                                                                                  |
| `editAsTypedComments`       | `"Comments typed in {edit} are not saved: the {host} file keeps only its own comments."`                                           | The Edit-as notice when comments are typed in the edited language, which saving drops, and the host's language has comments.                                                                     |
| `editAsTypedCommentsNone`   | `"Comments typed in {edit} are not saved: {host} has no comments."`                                                                | The same when the host's language has no comments (JSON).                                                                                                                                        |
| `editAsDismiss`             | `"Dismiss"`                                                                                                                        | The Edit-as notice's close button.                                                                                                                                                               |
| `baselineChanged`           | `"The file changed on the server."`                                                                                                | The notice shown when the host's baseline changes under unsaved edits.                                                                                                                           |
| `baselineChangedDetail`     | `"Reload it, losing your changes, or keep your text and compare it with the new version."`                                         | What the notice's choices do, after its message.                                                                                                                                                 |
| `reload`                    | `"Reload"`                                                                                                                         | The notice's button that takes the new baseline as the text.                                                                                                                                     |
| `reloadTitle`               | `"Replace your text with the new version (Ctrl+Z brings yours back)"`                                                              | Its tooltip.                                                                                                                                                                                     |
| `keepMine`                  | `"Keep mine"`                                                                                                                      | The notice's button that keeps the text against the new baseline.                                                                                                                                |
| `keepMineTitle`             | `"Keep your text; the changes are now shown against the new version (Escape)"`                                                     | Its tooltip.                                                                                                                                                                                     |
| `baselineChangedKeys`       | `"F6 moves to the choices and back."`                                                                                              | How to reach the notice's choices from the keyboard, shown on it (and announced with it).                                                                                                        |
| `findWidget`                | `"Find and replace"`                                                                                                               | The find widget's accessible name.                                                                                                                                                               |
| `toggleReplace`             | `"Toggle replace"`                                                                                                                 | The button showing the replace row.                                                                                                                                                              |
| `toggleReplaceTitle`        | `"Toggle replace (Ctrl+H)"`                                                                                                        | Its tooltip.                                                                                                                                                                                     |
| `find`                      | `"Find"`                                                                                                                           | The query field's accessible name and placeholder.                                                                                                                                               |
| `matchCase`                 | `"Match case"`                                                                                                                     | The match-case toggle.                                                                                                                                                                           |
| `matchCaseTitle`            | `"Match case (Alt+C)"`                                                                                                             | Its tooltip.                                                                                                                                                                                     |
| `wholeWord`                 | `"Match whole word"`                                                                                                               | The whole-word toggle.                                                                                                                                                                           |
| `wholeWordTitle`            | `"Match whole word (Alt+W)"`                                                                                                       | Its tooltip.                                                                                                                                                                                     |
| `useRegex`                  | `"Use regular expression"`                                                                                                         | The regular-expression toggle.                                                                                                                                                                   |
| `useRegexTitle`             | `"Use regular expression (Alt+R)"`                                                                                                 | Its tooltip.                                                                                                                                                                                     |
| `previousMatch`             | `"Previous match"`                                                                                                                 | The previous-match button.                                                                                                                                                                       |
| `previousMatchTitle`        | `"Previous match (Shift+Enter)"`                                                                                                   | Its tooltip.                                                                                                                                                                                     |
| `nextMatch`                 | `"Next match"`                                                                                                                     | The next-match button.                                                                                                                                                                           |
| `nextMatchTitle`            | `"Next match (Enter)"`                                                                                                             | Its tooltip.                                                                                                                                                                                     |
| `close`                     | `"Close"`                                                                                                                          | The find widget's close button.                                                                                                                                                                  |
| `closeTitle`                | `"Close (Escape)"`                                                                                                                 | Its tooltip.                                                                                                                                                                                     |
| `replace`                   | `"Replace"`                                                                                                                        | The replacement field's accessible name and placeholder, and the button replacing the current match.                                                                                             |
| `replaceTitle`              | `"Replace (Enter)"`                                                                                                                | The replace button's tooltip.                                                                                                                                                                    |
| `replaceAll`                | `"All"`                                                                                                                            | The replace-all button.                                                                                                                                                                          |
| `replaceAllLabel`           | `"Replace all"`                                                                                                                    | Its accessible name.                                                                                                                                                                             |
| `replaceAllTitle`           | `"Replace all (Ctrl+Alt+Enter)"`                                                                                                   | Its tooltip.                                                                                                                                                                                     |
| `matchPosition`             | `"{current} of {total}"`                                                                                                           | The match count: match `{current}` of `{total}`.                                                                                                                                                 |
| `noResults`                 | `"No results"`                                                                                                                     | The match count when nothing matches.                                                                                                                                                            |
| `invalidPattern`            | `"Invalid pattern"`                                                                                                                | The match count when the regular expression is invalid.                                                                                                                                          |
| `replacedOne`               | `"Replaced {count} occurrence"`                                                                                                    | After Replace All, for one match.                                                                                                                                                                |
| `replacedOther`             | `"Replaced {count} occurrences"`                                                                                                   | After Replace All, for several.                                                                                                                                                                  |
| `replaceSkipped`            | `"; skipped {count} in read-only text or secret placeholders"`                                                                     | Added when Replace All left matches in protected text alone.                                                                                                                                     |
| `nothingReplaced`           | `"Every match is in read-only text; nothing was replaced"`                                                                         | After Replace All when every match is in read-only text.                                                                                                                                         |
| `goToLine`                  | `"Go to line"`                                                                                                                     | The go-to-line field's accessible name.                                                                                                                                                          |
| `goToLinePlaceholder`       | `"line or line:column"`                                                                                                            | Its placeholder.                                                                                                                                                                                 |
| `goToLineHint`              | `"Line 1–{lines}, or line:column"`                                                                                                 | Its hint: the range of lines.                                                                                                                                                                    |
| `goToLineInvalid`           | `"Type a line number between 1 and {lines}"`                                                                                       | Its hint when the text is no position.                                                                                                                                                           |
| `addedLine`                 | `"Added line"`                                                                                                                     | The change preview's title for an added line.                                                                                                                                                    |
| `modifiedLine`              | `"Modified line"`                                                                                                                  | For a modified line.                                                                                                                                                                             |
| `removedLines`              | `"Removed lines"`                                                                                                                  | For removed lines.                                                                                                                                                                               |
| `removedLinesAtEnd`         | `"Removed lines at end of document"`                                                                                               | For lines removed after the last one.                                                                                                                                                            |
| `showAdded`                 | `"Show added line {line}"`                                                                                                         | The gutter marker of an added line.                                                                                                                                                              |
| `showAddedTitle`            | `"Show added line"`                                                                                                                | Its tooltip.                                                                                                                                                                                     |
| `showModified`              | `"Show before and after for modified line {line}"`                                                                                 | The gutter marker of a modified line.                                                                                                                                                            |
| `showModifiedTitle`         | `"Show before and after"`                                                                                                          | Its tooltip.                                                                                                                                                                                     |
| `showRemoved`               | `"Show removed lines before line {line}"`                                                                                          | The gutter marker of removed lines.                                                                                                                                                              |
| `showRemovedTitle`          | `"Show removed lines"`                                                                                                             | Its tooltip, and that of lines removed at the end.                                                                                                                                               |
| `showRemovedAtEnd`          | `"Show removed lines at end of document"`                                                                                          | The gutter marker of lines removed after the last one.                                                                                                                                           |
| `previewHeading`            | `"{title} · line {line}"`                                                                                                          | The change preview's heading: its `{title}` and `{line}`.                                                                                                                                        |
| `previewLabel`              | `"{title}, line {line}"`                                                                                                           | The change preview's accessible name.                                                                                                                                                            |
| `revert`                    | `"Revert"`                                                                                                                         | The preview's revert button.                                                                                                                                                                     |
| `revertTitle`               | `"Revert this change (R)"`                                                                                                         | Its tooltip, for a one-line change.                                                                                                                                                              |
| `revertLinesTitle`          | `"Revert this change (lines {first}–{last}) (R)"`                                                                                  | Its tooltip, for a change over lines `{first}` to `{last}`.                                                                                                                                      |
| `revertLineAndRemovedTitle` | `"Revert this change (line {line} and the removed lines) (R)"`                                                                     | Its tooltip, for a changed line with removed lines.                                                                                                                                              |
| `closePreview`              | `"Close change preview"`                                                                                                           | The preview's close button.                                                                                                                                                                      |
| `before`                    | `"Before"`                                                                                                                         | The preview's column of the baseline's lines.                                                                                                                                                    |
| `current`                   | `"Current"`                                                                                                                        | The preview's column of the current lines.                                                                                                                                                       |
| `fold`                      | `"Fold line {line}"`                                                                                                               | The chevron of an open range.                                                                                                                                                                    |
| `unfold`                    | `"Unfold lines {first}–{last}"`                                                                                                    | The chevron or chip of a closed fold.                                                                                                                                                            |
| `foldedChanges`             | `"Folded lines have changes"`                                                                                                      | A closed fold's line number when it hides changes or problems.                                                                                                                                   |
| `required`                  | `"required"`                                                                                                                       | The hover card's mark of a required key.                                                                                                                                                         |
| `deprecated`                | `"deprecated"`                                                                                                                     | The hover card's mark of a deprecated key.                                                                                                                                                       |
| `defaultValue`              | `"Default"`                                                                                                                        | The hover card's default row.                                                                                                                                                                    |
| `examples`                  | `"Examples"`                                                                                                                       | The hover card's examples row.                                                                                                                                                                   |
| `oneOf`                     | `"One of"`                                                                                                                         | The hover card's row listing the titles of the options (`oneOf`, `anyOf`) a field's object may take, while none matches it yet.                                                                  |
| `documentation`             | `"Documentation"`                                                                                                                  | The hover card's link to the schema's documentation.                                                                                                                                             |
| `openDocs`                  | `"Open docs ↗ (Alt+Enter)"`                                                                                                        | The link under a host problem's message.                                                                                                                                                         |
| `error`                     | `"Error"`                                                                                                                          | An error, as a screen reader hears it.                                                                                                                                                           |
| `warning`                   | `"Warning"`                                                                                                                        | A warning, as a screen reader hears it.                                                                                                                                                          |
| `info`                      | `"Info"`                                                                                                                           | An info, as a screen reader hears it.                                                                                                                                                            |
| `hint`                      | `"Hint"`                                                                                                                           | A hint, as a screen reader hears it.                                                                                                                                                             |
| `problemSource`             | `" from {source}"`                                                                                                                 | Who reported a host problem: the `{source}` of the problem texts below (empty for the editor's own problems).                                                                                    |
| `problem`                   | `"{severity}{source}: {message}"`                                                                                                  | A problem at the caret, for screen readers.                                                                                                                                                      |
| `problemPosition`           | `"{severity} {position} of {total}{source}: {message}"`                                                                            | Announced on `F8`: problem `{position}` of `{total}`.                                                                                                                                            |
| `problemCode`               | `" Code {code}."`                                                                                                                  | After a problem's text for screen readers: its `{code}` (from the host).                                                                                                                         |
| `problemDocs`               | `" Documentation: {url} (Alt+Enter opens it)."`                                                                                    | After a problem's text for screen readers: its documentation's `{url}`, and the key that opens it.                                                                                               |
| `problemMarker`             | `"{language} {severity}{source} on line {line}: {message}"`                                                                        | An underlined problem's accessible name.                                                                                                                                                         |
| `protectedNote`             | `"{label}: {message}"`                                                                                                             | What protected text at the caret is, for screen readers: its `{label}` and `{message}`.                                                                                                          |
| `sourceLabel`               | `"{language} source"`                                                                                                              | The text area.                                                                                                                                                                                   |
| `suggestionsLabel`          | `"{language} suggestions"`                                                                                                         | The suggestions list.                                                                                                                                                                            |
| `problemsLabel`             | `"Inline {language} diagnostics"`                                                                                                  | The layer of underlined problems.                                                                                                                                                                |
| `editorLabel`               | `"{language} editor"`                                                                                                              | The editor's frame.                                                                                                                                                                              |
| `readOnlyField`             | `"This field is read-only"`                                                                                                        | Why a field is read-only when neither the schema nor the host says.                                                                                                                              |
| `readOnlyText`              | `"This text is read-only"`                                                                                                         | Why text in a read-only range the host gave cannot change.                                                                                                                                       |
| `holdsReadOnly`             | `"Holds a read-only field: {reason}"`                                                                                              | Why renaming or moving the key above a read-only field is refused.                                                                                                                               |
| `saveRefused`               | `"{pointer} is read-only and this change would alter it"`                                                                          | A save refused because it would change a read-only value.                                                                                                                                        |
| `readOnlyChanged`           | `"This edit makes the text valid, but {pointer} would no longer read as the host gave it ({reason}): undo the change to it first"` | An edit refused because it makes a text that did not parse parse with the read-only field at `{pointer}` reading otherwise than in the host's text, and why the field is read-only (`{reason}`). |
| `readOnlyUnchecked`         | `"Too much changed while the text was invalid to check its read-only fields: undo some changes first"`                             | An edit refused because it makes a text that did not parse parse after more changes to the host's text than the read-only fields can be checked through.                                         |
| `readOnlyLabel`             | `"Read-only"`                                                                                                                      | The gutter lock's label, before the reason.                                                                                                                                                      |
| `secretLabel`               | `"Secret placeholder"`                                                                                                             | The gutter key's label, before the hint.                                                                                                                                                         |
| `secretHint`                | `"Keeps the current secret unless you replace it"`                                                                                 | The hover text of a secret placeholder, unless the host gives one.                                                                                                                               |
| `secretValue`               | `"the secret value"`                                                                                                               | What a schema problem says instead of a secret field's value.                                                                                                                                    |
| `unknownPlaceholder`        | `"{pointer}: unknown secret placeholder; it stands for no current secret, so write the new value instead"`                         | A placeholder the host did not give, in a secret field.                                                                                                                                          |
| `movedPlaceholder`          | `"{pointer}: this placeholder stands for the secret of {owner}, not this field's; here it is plain text"`                          | One of the host's placeholders outside the field it stands in.                                                                                                                                   |
| `envReference`              | `"Read the secret from an environment variable"`                                                                                   | The detail of the `${…}` reference suggestion.                                                                                                                                                   |
| `fileReference`             | `"Read the secret from a file"`                                                                                                    | The detail of the `file:` reference suggestion.                                                                                                                                                  |
| `otherReference`            | `"A reference starting with {prefix}"`                                                                                             | The detail of another reference suggestion.                                                                                                                                                      |
| `referenceType`             | `"reference"`                                                                                                                      | The type column of a reference suggestion.                                                                                                                                                       |
| `closesTag`                 | `"Closes <{name}>"`                                                                                                                | The detail of the end tag XML suggests.                                                                                                                                                          |
| `endTag`                    | `"end tag"`                                                                                                                        | Its type column.                                                                                                                                                                                 |
| `keyNotAllowed`             | `"{pointer}: '{key}' is not allowed here"`                                                                                         | A key the schema does not allow.                                                                                                                                                                 |
| `textNotAllowed`            | `"{pointer}: text is not allowed here, only elements"`                                                                             | Text in an element the schema wants elements in (XML).                                                                                                                                           |
| `keyDeprecated`             | `"{pointer}: '{key}' is deprecated"`                                                                                               | A deprecated key.                                                                                                                                                                                |
| `keyDeprecatedInstead`      | `"{pointer}: '{key}' is deprecated. {message}"`                                                                                    | A deprecated key whose schema says what to use instead.                                                                                                                                          |
| `notOneOf`                  | `"{value} is not one of {allowed}"`                                                                                                | A value not among the allowed ones.                                                                                                                                                              |
| `wrongType`                 | `"{value} is not of type {types}"`                                                                                                 | A value of none of the accepted types.                                                                                                                                                           |
| `invalidSchema`             | `"Invalid JSON Schema: {error}"`                                                                                                   | A schema that does not compile.                                                                                                                                                                  |
| `invalidPlaceholder`        | `"secrets.placeholder: {error}"`                                                                                                   | A secret placeholder pattern the editor cannot use (it does not compile, or matches empty text): no token is taken for a placeholder.                                                            |
| `problemAtPath`             | `"{path}: {message}"`                                                                                                              | A host's problem with a field the editor cannot find in the text, listed with the field's path.                                                                                                  |
| `hostCannotKeep`            | `"{pointer}: {host} cannot keep this value exactly; it reads back differently"`                                                    | A value the host's language reads back differently (Edit-as).                                                                                                                                    |
| `editCannotShow`            | `"{pointer}: {edit} cannot show this value exactly; editing it here may change it"`                                                | A value the edited language cannot show exactly (Edit-as).                                                                                                                                       |
| `notSaved`                  | `"Not saved as {host}: {error}"`                                                                                                   | An edit the host's language cannot save (Edit-as).                                                                                                                                               |
| `noNull`                    | `"{host} has no null value (at {pointer})"`                                                                                        | A null the host's language cannot write (Edit-as).                                                                                                                                               |
| `noNonFinite`               | `"{host} has no infinity or NaN (at {pointer})"`                                                                                   | An infinity or NaN the host's language has no number for (Edit-as).                                                                                                                              |
| `cannotEditAs`              | `"Cannot edit this {host} as {edit}: {error}"`                                                                                     | A host document that does not convert for editing.                                                                                                                                               |
| `cannotSwitch`              | `"Cannot switch to {edit} yet: {error}"`                                                                                           | An Edit-as switch waiting until the document converts.                                                                                                                                           |
| `mixedContent`              | `"{pointer}: text beside child elements cannot be shown in another language"`                                                      | XML text beside child elements, which no other language can show (Edit-as).                                                                                                                      |
| `tabList`                   | `"Open files"`                                                                                                                     | The tab list's accessible name, unless the host gives one.                                                                                                                                       |
| `noDocuments`               | `"No open files"`                                                                                                                  | Shown in place of an editor while no document is open.                                                                                                                                           |
| `closeTab`                  | `"Close {name}"`                                                                                                                   | A tab's close button.                                                                                                                                                                            |
| `closeTabTitle`             | `"Close (Delete)"`                                                                                                                 | Its tooltip.                                                                                                                                                                                     |
| `unsaved`                   | `"unsaved changes"`                                                                                                                | The dirty dot's tooltip, and what a tab's accessible name adds for it.                                                                                                                           |
| `readOnlyTab`               | `"read-only"`                                                                                                                      | What a read-only document's tab adds to its accessible name.                                                                                                                                     |

</details>

<!-- strings:end -->

### `update(options)`

`editor.update(options)` changes a mounted editor, live (the caret, undo
history, scroll and folds stay). It takes the constructor's options, any of
them, and is the one way to change one:

```ts
editor.update({ readOnly: true, theme: "nord" });
editor.update({ value: loaded, baseline: loaded }); // a file loaded from the server
editor.update({ hostDiagnostics: problems, onSave: null });
```

- A key left out, or set to `undefined`, stays as it is.
- `null` removes a callback (`onSave: null` gives `Ctrl+S` back to the
  browser), clears `hostDiagnostics`, `readOnlyPaths` and `readOnlyRanges`,
  turns `editAs` and `readOnly` off, and returns a setting to its default.
- Settings merge key by key, and `strings` text by text.
- A new `value` replaces the document as a host update: the caret stays on
  the same text when possible. `readOnlyRanges` and host `range`s given with
  it are in the new document.
- `language`, `schema`, `fileName`, `className` and `secrets` are what the
  editor is built from: they throw (`"language cannot change on a mounted
ConfigEditor …"`); create a new editor to change them. TypeScript types
  them `never` in `EditorUpdate`.
- Every key is checked before any applies: a wrong value throws and changes
  nothing.

### Instance members

`ConfigEditor` is an `EditorView`, the interface a document's editor in a
[`ConfigDocuments`](#several-documents) (`view.editor(id)`) implements too:

| `EditorView` member                                      | Description                                                                                                                                                                            |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `value` (getter)                                         | The current document, in `language`, including an edit made in `editAs` that is still waiting out the typing pause (it is sent to `onChange` first). Still readable after `destroy()`. |
| `text` (getter)                                          | The text on screen: in the `editAs` language while editing as another one. `rangeOf` and `pathAt` index into it; `value` is the document in `language`.                                |
| `isDirty` (getter)                                       | Whether the document differs from the baseline.                                                                                                                                        |
| `currentPath` (getter)                                   | The path of what the caret is on; see [Paths](#paths).                                                                                                                                 |
| `foldedLines` (getter)                                   | The folded blocks, one-based `{ startLine, endLine }[]`: the line that stays visible and the last hidden one.                                                                          |
| `settings` (getter)                                      | The settings in effect, every key set; see [Settings](#settings).                                                                                                                      |
| `reveal(path)`, `rangeOf(path)`, `pathAt(index)`         | Paths and positions; see [Paths](#paths).                                                                                                                                              |
| `fold(path)`, `unfold(path)`, `foldAll()`, `unfoldAll()` | Fold the block a field's key opens / open the folds around a field (`false` when there was nothing to do); fold or open everything. See [Folding](#folding).                           |
| `focus()`                                                | Focus the editor, keeping its selection.                                                                                                                                               |
| `save()`                                                 | Save as `Ctrl+S` does (calls `onSave`); `false` when nothing was sent (no `onSave`, or an `editAs` edit that cannot convert back yet).                                                 |
| `unifiedDiff(context?)`                                  | The `diff -u` of the baseline against the document, with `context` unchanged lines around each change (default 3); `""` when they are the same. Its lines end with LF.                 |

`ConfigEditor` adds:

| Member            | Description                                                                                                                                                                                                                                                                                     |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `update(options)` | Change the editor live; see [`update`](#updateoptions).                                                                                                                                                                                                                                         |
| `destroy()`       | Unmount. An edit still converting (while editing as another language) is sent to `onChange` first. Safe to call inside `onChange`.                                                                                                                                                              |
| `free()`          | Release the WebAssembly object after `destroy()`. Optional; the garbage collector does it too. An editor stays on the page until `destroy()`, whether or not you keep a reference to it. Call `destroy()` first: after `free()` the instance cannot be used at all (wasm-bindgen's own method). |

An exception thrown by a callback is logged to the console and does not
break the editor.

### Host diagnostics

When a server validates the document, show its problems with the editor's own:

```ts
editor.update({
  hostDiagnostics: [
    {
      path: "/web/listen",
      severity: "error",
      message: "port 80 is taken",
      code: "E_PORT",
      source: "burrow",
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

- `path` (a JSON pointer or keys and indices) is marked where the field is
  written and follows it through edits and `editAs`. `range` is in the
  document (`value`, in `language`): string indices `{ start, end }` or
  one-based `{ startLine, startColumn, endLine?, endColumn? }`, underlined on
  every line it covers (up to 200). While editing as another language it
  marks the field it starts in (its key when it starts on the key). With
  neither, or when the field is missing, the problem shows in the footer,
  after the field's path (`/web/listen: port 80 is taken`).
- `severity` is `"error"` (default), `"warning"`, `"info"` or `"hint"`;
  colors `error`, `warning`, `info` and `hint` in `colors`.
- `F8` stops at them, the footer counts them, and the hover message adds
  `source` (default `"host"`), `code` and "Open docs ↗": a click on the
  message opens `docsUrl` (an `http:` or `https:` URL), as `Alt+Enter`
  does with the caret on the problem; screen readers hear the code and the
  link with the problem.
- `path` and `range` are exclusive; a range past the text's end, ending
  before it starts, or a pointer without its leading `/` throws.
- The editor lists and underlines at most 20 problems of each source (the
  schema's, the secret placeholders'), and the footer says how many were
  cut ("20+ errors · only the first 20 are shown"); a problem with no place
  (the whole document's) stays in the footer and `F8` reaches it.
- Where the host marks the same text as one of the editor's own problems,
  only the host's shows. A new list replaces the previous one (`null` or
  `[]` clears it); only the markers update. An invalid entry throws, naming
  its index.

### Read-only fields

A field is read-only when the schema marks it `readOnly: true` (with its
reason in `x-readonly-reason`), or when the host names it:

```ts
const editor = new ConfigEditor(element, {
  language: "yaml",
  value,
  schema, // {"properties": {"id": {"readOnly": true, "x-readonly-reason": "Assigned by the server"}}}
  readOnlyPaths: ["/limits"],
  readOnlyRanges: [{ startLine: 1, startColumn: 1, endLine: 2, endColumn: 1 }],
});
editor.update({ readOnlyPaths: ["/limits", "/tls/cert"] }); // at runtime
```

- A lock in the gutter marks each locked line, the value is muted (color
  token `locked`), and hovering it shows the reason.
- Typing, deleting, pasting, cutting, completions, Replace, `Ctrl+/`,
  Revert and undo that would change a read-only value, delete its key (also
  inside a deleted selection or parent block) or add to a read-only block
  are refused: the text stays, and a tooltip at the caret says why. Replace
  All skips matches in read-only text and says how many.
- Whitespace, comments and punctuation around a locked field stay free
  (also whitespace inside it, such as a space after `:`); in YAML a locked
  line's indent cannot change (it is structure). A syntax error anywhere
  keeps every lock until it is repaired, also in a `value` that does not
  parse and for `readOnlyPaths` given while the text does not parse.
- Suggestions leave read-only keys out (inside a read-only block they are
  listed with the reason).
- With `editAs`, the same fields are locked in the other language, and a
  save that would change one is refused (shown as an error, not sent).

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

- A placeholder is real only in the field of the host's own `value` it
  was given in (and accepted by `isPlaceholder`). Anywhere else it is plain
  text, so it never stands for another field's secret: one of the host's
  tokens pasted into another field, or under a renamed key, is flagged, and
  so is a token the host did not give in a secret field. Put back into its
  own field it is real again. `placeholder` must be a regular expression
  that matches no empty text (`.*` throws).
- A placeholder is atomic: the caret never lands inside, `Backspace` and
  `Delete` remove it whole, an edit cutting into it replaces it whole, and
  copying keeps its text. It shows as a chip (color tokens `secret-chip`,
  `secret-chip-border`) with a key in the gutter; hovering shows `hint`.
- In a secret field, suggestions offer `${VAR}` and `file:` references.
- The schema check accepts an untouched placeholder (no `pattern`,
  `format`, length or type problem), and `editAs` and saving keep it
  byte-for-byte.

### Paths

A path names a field the same way in every language: a JSON pointer
(`"/web/hosts/1"`, `""` for the whole document) or its keys and indices
(`["web", "hosts", 1]`). A pointer starts with `/`: `"web/hosts"` throws,
everywhere a path is taken (the editor's methods, the tools,
`readOnlyPaths`, host diagnostics).

```ts
editor.reveal("/web/listen"); // scroll to it, select it, focus; false when absent
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
the key alone when the value is a block below it, and the value alone for a
list item. `pathAt` gives the key under or just before the index, else the
value or container holding it (`""` at the top level).

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

## `@pngwasi/config-editor/tools`

The editor's answers about a text, without an editor: for a server, a
build step or a review screen. They need the WebAssembly module loaded:
`loadConfigEditor()`, which this entry exports too.

| Function                                   | Returns               | Description                                                                                                                                                                                                                                                                                                                                                                                |
| ------------------------------------------ | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `validate(language, text, schema?)`        | `Diagnostic[]`        | Syntax problems, plus schema problems when a schema is given.                                                                                                                                                                                                                                                                                                                              |
| `convert(from, text, to, schema?)`         | `{ text, losses }`    | Convert between languages. `losses` lists JSON pointers of values the target reads back differently (an infinity or `NaN` is written as the text `inf` or `nan` in JSON, HCL and XML, and listed). Throws when the text does not parse or the target cannot express it at all (TOML has no `null`). With a schema, a lone XML element where it expects a list converts as a one-item list. |
| `unifiedDiff(baseline, current, context?)` | `string`              | The `diff -u` of two texts: `--- baseline` / `+++ current` headers, then `@@` hunks with `context` lines around each change (default 3). `""` when they have the same lines; line endings are compared as LF.                                                                                                                                                                              |
| `changeSummary(baseline, current)`         | `ChangeSummary`       | `{ added, removed, modified, hunks }` between two texts, as the diff gutter and `onChangeSummary` count them.                                                                                                                                                                                                                                                                              |
| `parse(language, text)`                    | `unknown`             | The document as plain data (objects, arrays, strings, numbers, booleans, `null`): the model schema validation sees, the same in every language. YAML's `.inf`/`.nan` and TOML's `inf`/`nan` are `Infinity`, `-Infinity` and `NaN`. Integers past 2^53 (TOML holds 64-bit ones) lose precision as JavaScript numbers, as with `JSON.parse`.                                                 |
| `serialize(language, value)`               | `string`              | Plain data as a document, only when it reads back as exactly that value (the check `convert`'s `losses` make). Throws when the value is not plain data (a `Date`, `Map`, `Set` or class instance throws: pass a date's `toISOString()`) or the language cannot express it exactly (TOML has no `null`; XML has no empty, nested or one-item list).                                         |
| `rangeOf(language, text, path)`            | `EditorRange \| null` | Where a field is written in a text, as the editor's `rangeOf` answers; see [Paths](#paths).                                                                                                                                                                                                                                                                                                |
| `pathAt(language, text, index)`            | `EditorPath \| null`  | The path at a string index of a text, as the editor's `pathAt` answers.                                                                                                                                                                                                                                                                                                                    |
| `schemaWarnings(schema)`                   | `string[]`            | What the editor cannot use in a schema (the messages `onSchemaWarning` receives). Throws on an invalid schema.                                                                                                                                                                                                                                                                             |
| `presetCss()`                              | `string`              | The stylesheet the editor injects (layout and every theme), for `styled: false`.                                                                                                                                                                                                                                                                                                           |

```ts
interface Diagnostic {
  line?: number; // one-based; absent when the problem has no position
  column?: number;
  endColumn?: number; // just past the marker, on endLine when it is set
  endLine?: number; // only when the marker covers several lines
  message: string;
  severity: "error" | "warning" | "info" | "hint";
  deprecated?: boolean; // true on a deprecated key's warning (the marker is the key)
}
```

```js
import {
  convert,
  loadConfigEditor,
  parse,
  rangeOf,
  serialize,
  validate,
} from "@pngwasi/config-editor/tools";

await loadConfigEditor();

validate("json", '{"insecure_plaintext": "yes"}', schemaText);
// [{ line: 1, column: 24, endColumn: 29, severity: "error",
//    message: '/insecure_plaintext: "yes" is not of type "boolean"' }]

// At most 20 problems are listed; past that, a 21st without a position says
// more follow (the footer shows it too).

convert("yaml", "listen: 7835\n", "toml");
// { text: "listen = 7835\n", losses: [] }

parse("toml", "[web]\nhttp = 80\n"); // { web: { http: 80 } }
serialize("hcl", { web: { http: 80 } }); // "web {\n  http = 80\n}\n"
rangeOf("yaml", "web:\n  http: 80\n", "/web/http");
// { start: 7, end: 15, startLine: 2, startColumn: 3, endLine: 2, endColumn: 11 }
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

## Keyboard

**Writing**

- `Ctrl+Space`: suggestions; `Enter` or `Tab` accepts one
- `Tab` / `Shift+Tab`: indent / outdent
- `Ctrl+/`: comment or uncomment the lines
- `Ctrl+Enter` / `Ctrl+Shift+Enter`: new line below / above
- `Ctrl+Z` / `Ctrl+Y`: undo / redo
- `Ctrl+S`: save (with `onSave`)
- `Escape`: close what is open (suggestions, a preview, a widget)

**Finding your way**

- `Ctrl+F` / `Ctrl+H`: find / find and replace; `Enter` and `Shift+Enter` (or `F3` / `Shift+F3`) next / previous match
- `Ctrl+G`: go to a line (`line` or `line:column`)
- `F8` / `Shift+F8`: next / previous problem
- `Alt+Enter`: open the documentation of the problem at the caret (a host problem with a docs link)
- `F6`: while the "file changed on the server" notice is open, move to its choices and back
- `Alt+Enter` on a problem with a docs link: open its documentation
- `F6` while the baseline notice is open: to its Keep mine button and back to the text

**Changes against the baseline**

- `Alt+F5` / `Shift+Alt+F5`: next / previous change
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

Folding only changes the view: `value`, `onChange`, undo and saving see the
whole text, and copying a selection across a fold copies the hidden lines.
An edit that touches a fold (typing at the end of a folded line, Backspace
at the start of the next one, a paste over a selection reaching into it)
opens it first, and whatever moves the caret into a fold (find, go to line,
`F8`, `reveal`, a click on a problem) opens it. A folded block hiding an
error or changes marks its line. The textarea holds the text on screen, so
read the document from `value`, not from the element.

## Saving and unsaved edits

Nothing is saved by itself: `Ctrl+S` / `Cmd+S` in the editor (and
`editor.save()`) calls `onSave` with the document in `language`. While
editing as another language, an edit still waiting out the typing pause is
converted first; when it cannot be written in `language` (invalid, or a
`null` for TOML), nothing is sent, `save()` returns `false`, and the caret
goes to the problem. The document is dirty while it differs from `baseline`
(the document at mount when no `baseline` is given). Embedding notes (what
is fixed at mount, which text to send, flushing before your own Save
button) are in the crate's
[Embedding](https://crates.io/crates/config-editor-leptos#embedding)
section: `editor.value` is the document in `language`, `editor.text` the
text on screen.

Line endings are kept: when the host's text ends its lines with CRLF,
`onChange` and `onSave` receive CRLF too (the editor works on LF text inside,
and dirty state and the diff compare line endings as LF). Each line keeps its
own: in a text mixing CRLF, LF and old Mac CR endings, an untouched line ends
as it did, an edited line keeps its ending, and a new line takes the ending
of the line above (the text's most common one when there is none). A lone CR
that the language does not read as a line break (TOML, HCL) is shown as the
syntax error its reader gives (as `parse` reports it), until that line is
edited.

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
// Another client saved: the server's version moves on.
events.on("config-changed", (text) => editor.update({ baseline: text }));
```

A new baseline while the user has unsaved edits opens a notice in the editor,
"The file changed on the server." (announced, `role="alert"`; it leaves the
focus in the text, and `F6` moves it to **Keep mine** and back): **Reload** makes the new baseline the document (one undo step,
reported through `onChange`), **Keep mine** (or `Escape`) keeps the document
and diffs it against the new baseline. Without unsaved edits, or when the new
baseline is the document or a text the editor sent (`onChange`, `onSave`: a
save coming back while the user typed on), it is taken silently. A new
`value` keeps the caret and the scroll position.

## Several documents

`ConfigDocuments` shows several files, a main config and its drop-ins, as
VS Code-style tabs over one editor per document, each with its own language
(from `fileName` unless given), baseline, read-only state, `editAs`,
schema (documents with the same schema text share one compiled schema) and,
optionally, settings, host problems and read-only fields.

```js
import { ConfigDocuments, loadConfigEditor } from "@pngwasi/config-editor";

await loadConfigEditor();
let files = [
  { id: "main", fileName: "burrow.yaml", value: mainText, schema },
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
  `language` or `schema` mounts a new editor).
- Nothing closes by itself: `onClose({ id, dirty })` asks you (close button,
  middle click, `Delete` on a tab). Without `onClose`, tabs have no close
  button.
- Switching keeps each document's caret, selection, scroll position, undo
  history, folds and find widget: an editor mounts the first time its
  document is shown, then stays (hidden) while it is open. With ten
  ~2000-line documents all shown, the view used about 21 MB of wasm memory
  (1.5 to 1.9 MB per editor) and 4 MB of JS heap. Closing a document gives
  its editor's memory back, as `destroy()` does for a `ConfigEditor` (all
  but under 1 KB).
- Keys: `←`/`→`, `Home`/`End` in the tab list (a WAI-ARIA `tablist`),
  `Enter` into the editor, `Delete` to close; `Alt+PageDown`/`Alt+PageUp`
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
  is), the settings and the callbacks. `className` and `label` (the tab
  list's name) are fixed and throw, and so does an `onClose` for a view
  created without one (its tabs have no close buttons); `activeId` given to
  the constructor is the document shown first. An `activeId` not listed
  throws there, as a wrong option does; `select(id)` is a request and
  answers `false` for an id not listed. Also `destroy()`.

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
        .filter(Boolean)
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
    // JSON pointer, so schema problems are underlined.
  },
  valueSpan(text, pointer) {
    /* optional, like keySpan for the value */
  },
  highlight(line) {
    // Optional: [{ start, end, kind }] ranges of one line, as JavaScript
    // string indices (what `indexOf` gives); kind is
    // "key" | "string" | "value" | "literal" | "comment" | "punctuation".
  },
  // Optional syntax, as in VS Code's language configuration:
  comments: { lineComment: "#" }, // Ctrl+/ (a list: the first is written); blockComment: ["/*", "*/"] is unwrapped
  brackets: [
    ["{", "}"],
    ["[", "]"],
  ], // Enter between them opens a line
  separators: ["="], // key renames, Tab through empty values
});

new ConfigEditor(element, { language: env, schema });
convert("env", "listen=7835\n", "yaml").text; // 'listen: "7835"\n'
```

A registered language gets schema validation (placed with `keySpan` and
`valueSpan`), conversion to and from every other language, and Edit-as;
`comments`, `brackets` and `separators` add comment toggling,
bracket-aware Enter, and key/value editing. Schema completion is available
for the built-in languages only. Built-in names cannot be replaced, and a
registered language cannot take another's name or extension (`registerLanguage`
throws); registering the same label again replaces it for new editors. Its name is
the label lowercased (`"Énv"` → `"énv"`).

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
(`config-editor__*` classes) is yours to style; `presetCss()` returns the
preset stylesheet to serve or adapt.

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

## License

MIT. See [LICENSE](LICENSE).
