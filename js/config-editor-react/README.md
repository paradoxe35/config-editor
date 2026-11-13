# Config Editor for React

React component for [`@pngwasi/config-editor`](https://www.npmjs.com/package/@pngwasi/config-editor):
a fast, schema-aware YAML, JSON, TOML, HCL and XML (or your own language) editor running as WebAssembly,
with completion from a JSON Schema, hover docs, validation, a diff gutter,
editing one language as another, and eleven themes.

<!-- toc -->

## Contents

- [Install](#install)
- [Quick start](#quick-start)
- [`<ConfigEditor>` props](#configeditor-props)
  - [Settings](#settings)
- [Ref](#ref)
  - [Saving](#saving)
  - [Several documents](#several-documents)
- [Keyboard and screen readers](#keyboard-and-screen-readers)
- [Exports](#exports)
- [License](#license)
<!-- /toc -->

## Install

```sh
npm install @pngwasi/config-editor-react @pngwasi/config-editor
```

`@pngwasi/config-editor` is a peer dependency. React 18 or 19. Its
WebAssembly is built with every feature of the Rust crate.

## Quick start

```tsx
import { useState } from "react";
import { ConfigEditor } from "@pngwasi/config-editor-react";

function Settings({ schema }: { schema: string }) {
  const [text, setText] = useState("listen: 7835\n");
  return (
    <ConfigEditor
      language="yaml"
      schema={schema}
      value={text}
      onChange={setText}
      showToolbar
      showFooter
    />
  );
}
```

The component loads the WebAssembly itself on first mount. To start the
download earlier, or when your bundler does not serve the `.wasm` next to
the package's JavaScript, call `loadConfigEditor` (from `@pngwasi/config-editor`) first:

```ts
import { loadConfigEditor } from "@pngwasi/config-editor";
// Vite
import wasmUrl from "@pngwasi/config-editor/config_editor_bg.wasm?url";

loadConfigEditor({ module_or_path: wasmUrl });
```

## `<ConfigEditor>` props

The props are the document and its callbacks (below) and every
[setting](#settings) (`theme`, `showFooter`, `folding`, …): `ConfigEditorProps`
extends the generated `EditorSettings` type.

| Prop                 | Type                                        | Default          | Purpose                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------------- | ------------------------------------------- | ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `value`              | `string`                                    | —                | The document (controlled).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `defaultValue`       | `string`                                    | `""`             | The initial document when uncontrolled.                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `onChange`           | `(value: string) => void`                   | —                | Called with the document after every edit (after each typing pause while editing as another language).                                                                                                                                                                                                                                                                                                                                                                                                     |
| `language`           | `string`                                    | `"yaml"`         | `"yaml"`, `"json"`, `"toml"`, `"hcl"`, `"xml"`, or a registered one. `value`, `baseline` and `onChange` are always in it.                                                                                                                                                                                                                                                                                                                                                                                  |
| `editAs`             | `string \| null`                            | —                | Show and edit the document in another language; it is saved back in `language`, keeping its comments.                                                                                                                                                                                                                                                                                                                                                                                                      |
| `onEditAsChange`     | `(language: string) => void`                | —                | Called with the language on screen: once on mount, then each time an `editAs` switch completes and when undo or redo steps back across one (a switch is an undo step). A switch waits while the document cannot convert (e.g. a `null` to TOML): until then the host's language stays on screen and reaches `onChange` as it is, with the reason on the value in the way. Label the language shown from this, not from `editAs`.                                                                           |
| `onSyncChange`       | `(inSync: boolean) => void`                 | —                | Called with whether the text on screen is written back to the document: once on mount, then each time that changes. `false` while an `editAs` edit cannot convert back (`onChange` waits), whatever brought it there (typing, paste, undo, a switch, a reload, a new value). Always `true` in `language`.                                                                                                                                                                                                  |
| `onChangeSummary`    | `(summary: ChangeSummary) => void`          | —                | Called with `{ added, removed, modified, hunks }` against `baseline`, on mount and whenever it changes.                                                                                                                                                                                                                                                                                                                                                                                                    |
| `onProblems`         | `(problems: Diagnostic[]) => void`          | —                | Called with the editor's own problems (syntax, schema, secret placeholders, values Edit-as cannot convert; `origin: "editor"`), never `hostDiagnostics`: once on mount, then each time they change after a pause in typing, not while they stay the same. With `showDiagnostics` off it finds none. Gate a Save button on it instead of calling `validate` again.                                                                                                                                          |
| `onPathChange`       | `(path: EditorPath) => void`                | —                | Called with the path of what the caret is on (`{ pointer: "/web/listen", segments: ["web", "listen"] }`) each time it changes, for a breadcrumb. The path is computed only while this is set.                                                                                                                                                                                                                                                                                                              |
| `hostDiagnostics`    | `HostDiagnostic[]`                          | —                | Problems the host found (its own checks: a service, a build step): `{ path \| range, severity?, message, code?, source?, docsUrl? }`, shown with the editor's own; see the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#host-diagnostics). Updates live; only the markers change.                                                                                                                                                                                |
| `readOnlyPaths`      | `FieldPath[]`                               | —                | Fields the user cannot change (each a whole subtree), on top of the schema's `readOnly` ones; they hold with `editAs` too. See the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#read-only-fields). Updates live.                                                                                                                                                                                                                                                 |
| `readOnlyRanges`     | `TextRangeInput[]`                          | —                | Stretches of the document (in `language`) the user cannot change: `{ start, end }` or `{ startLine, startColumn, endLine?, endColumn? }`. Updates live.                                                                                                                                                                                                                                                                                                                                                    |
| `secrets`            | `SecretOptions`                             | defaults         | `{ placeholder?, isPlaceholder?, hint?, referencePrefixes? }`: secret placeholders (`«secret 3f2a»`) in `writeOnly` / `x-secret` fields stay atomic and are kept unless replaced, each real only in the field the host gave it in; see the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#secrets). Changing `placeholder`, `hint` or `referencePrefixes`, or adding or removing `isPlaceholder`, re-mounts; a new `isPlaceholder` function in its place does not. |
| `schema`             | `string`                                    | —                | JSON Schema as JSON text: completion, hover docs and validation.                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `onSchemaWarning`    | `(message: string) => void`                 | console warning  | Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile.                                                                                                                                                                                                                                                                                                                                                                                    |
| `baseline`           | `string`                                    | initial value    | Text the diff gutter compares against: the version last saved or loaded. A new one under unsaved edits asks the user (Reload / Keep mine).                                                                                                                                                                                                                                                                                                                                                                 |
| `onSave`             | `(value: string, language: string) => void` | —                | Called on `Ctrl+S` / `Cmd+S` (the browser's dialog prevented) and `ref.save()`, with the document in `language`. Nothing saves by itself. Without it, `Ctrl+S` is the browser's.                                                                                                                                                                                                                                                                                                                           |
| `onDirtyChange`      | `(dirty: boolean) => void`                  | —                | Called with whether the document differs from `baseline`: once on mount, then each time that changes.                                                                                                                                                                                                                                                                                                                                                                                                      |
| `onBaselineConflict` | `(event: BaselineConflict) => void`         | —                | Called when a new `baseline` arrives under unsaved edits (`"opened"`) and when that is settled (`"reloaded"`, `"kept-mine"`, `"settled"`).                                                                                                                                                                                                                                                                                                                                                                 |
| `readOnly`           | `boolean`                                   | `false`          | View mode.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `readOnlyReason`     | `string`                                    | —                | Why the editor is read-only, while `readOnly` is on: the "Cannot edit in a read-only editor" notice says it, and so does the text area's accessible description. Updates live.                                                                                                                                                                                                                                                                                                                             |
| `fileName`           | `string`                                    | `untitled.<ext>` | Shown in the toolbar.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `editorClassName`    | `string`                                    | —                | Class on the editor root.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `className`, `style` |                                             | —                | Class and style of the container `<div>`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `onError`            | `(error: unknown) => void`                  | `console.error`  | Errors from loading or from a prop the editor rejects (unknown language, theme or color token, invalid schema). They never unmount your tree.                                                                                                                                                                                                                                                                                                                                                              |

`value`, `editAs`, `readOnly`, `baseline`, `hostDiagnostics`,
`readOnlyPaths`, `readOnlyRanges`, every setting, `onPathChange`, `onSave` and
the other callbacks update the live editor (the caret, undo history and folds
stay). `language`, `schema`, `fileName`, `editorClassName` and the `secrets`
patterns re-create it; the document and its diff baseline carry over (and
the caret, when the editor had focus), undo history starts over. The
component is a thin wrapper around `EditorBinding` from
`@pngwasi/config-editor`, shared with the
[Vue](https://www.npmjs.com/package/@pngwasi/config-editor-vue) and
[Svelte](https://www.npmjs.com/package/@pngwasi/config-editor-svelte)
packages: the three behave the same.

### Settings

The theme, the colors and every on/off switch, each a prop with a default;
the type (`EditorSettings`) is generated from the Rust crate's struct.
Changing one applies live, and one no longer passed returns to its
default.

```tsx
// A plain viewer: no colors, no diff, no suggestions, no popups.
<ConfigEditor
  value={text}
  readOnly
  highlighting={false}
  showDiff={false}
  completions={false}
  hoverDocs={false}
  showDiagnostics={false}
/>
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

`strings` rewords or translates every built-in text; give only the texts to
change (the others stay English), and new texts apply live:

```tsx
<ConfigEditor
  value={yaml}
  onChange={setYaml}
  editAs="json"
  strings={{ noProblems: "Keine Fehler", editAsDismiss: "Schließen" }}
/>
```

Placeholders are written in braces (`{count}`, `{line}`); counts come in
pairs, `…One` for 1 and `…Other` otherwise. While editing as another language,
a dismissible notice says the host file's comments are not shown there (saving
keeps them) and that comments typed there are not saved.

<!-- strings:start -->

<!-- Generated from crates/config-editor-leptos/src/strings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`. -->

<details>
<summary>Every text and its default (188 texts)</summary>

| Key                         | Default                                                                                                                                     |                                                                                                                                                                                                                                                |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `suggestionsHint`           | `"{key} for suggestions"`                                                                                                                   | The footer's suggestions hint; `{key}` is where the chord goes.                                                                                                                                                                                |
| `suggestionsHintTouch`      | `"suggestions appear as you type"`                                                                                                          | `suggestionsHint` on a touch screen, which has no `suggestionsKey`; shown only while suggestions open as you type.                                                                                                                             |
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
| `reloadTitle`               | `"Replace your text with the new version ({cmd}+Z brings yours back)"`                                                                      | Its tooltip.                                                                                                                                                                                                                                   |
| `keepMine`                  | `"Keep mine"`                                                                                                                               | The notice's button that keeps the text against the new baseline.                                                                                                                                                                              |
| `keepMineTitle`             | `"Keep your text; the changes are now shown against the new version (Escape)"`                                                              | Its tooltip.                                                                                                                                                                                                                                   |
| `baselineChangedKeys`       | `"In the editor, F6 moves to the choices and back."`                                                                                        | How to reach the notice's choices from the keyboard, shown on it (and announced with it); not on touch screens.                                                                                                                                |
| `findWidget`                | `"Find and replace"`                                                                                                                        | The find widget's accessible name.                                                                                                                                                                                                             |
| `toggleReplace`             | `"Toggle replace"`                                                                                                                          | The button showing the replace row.                                                                                                                                                                                                            |
| `toggleReplaceTitle`        | `"Toggle replace ({cmd}+H)"`                                                                                                                | Its tooltip.                                                                                                                                                                                                                                   |
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
| `replaceAllTitle`           | `"Replace all ({cmd}+Alt+Enter)"`                                                                                                           | Its tooltip.                                                                                                                                                                                                                                   |
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
| `noSuggestions`             | `"No suggestions"`                                                                                                                          | Said when typing leaves the open list with nothing to suggest.                                                                                                                                                                                 |
| `keyNotAllowed`             | `"\"{key}\" is not allowed here"`                                                                                                           | A key the schema does not allow (`{key}`); its object's pointer goes before it (`/web: `), as before every schema problem but at the root.                                                                                                     |
| `textNotAllowed`            | `"text is not allowed here, only elements"`                                                                                                 | Text in an element the schema wants elements in (XML); the element's pointer goes before it.                                                                                                                                                   |
| `keyDeprecated`             | `"{pointer}: \"{key}\" is deprecated"`                                                                                                      | A deprecated key.                                                                                                                                                                                                                              |
| `keyDeprecatedInstead`      | `"{pointer}: \"{key}\" is deprecated. {message}"`                                                                                           | A deprecated key whose schema says what to use instead.                                                                                                                                                                                        |
| `notOneOf`                  | `"{value} is not one of {allowed}"`                                                                                                         | A value not among the allowed ones.                                                                                                                                                                                                            |
| `wrongType`                 | `"{value} is not of type {types}"`                                                                                                          | A value of none of the accepted types.                                                                                                                                                                                                         |
| `noOptionMatches`           | `"matches none of the {keyword} options: {reasons}"`                                                                                        | A value that matches none of a schema's options (`{keyword}`: `oneOf` or `anyOf`), with why each fails (`{reasons}`, each worded by `optionNeeds`, `optionRefuses` or `optionProblem`, joined by `; `).                                        |
| `severalOptionsMatch`       | `"matches more than one of the oneOf options ({options}); only one may match"`                                                              | A value that matches more than one of a `oneOf`'s options (`{options}`, their names joined by `, `).                                                                                                                                           |
| `optionNeeds`               | `"{option} needs \"{key}\""`                                                                                                                | An option (`{option}`) that needs a key the value does not have.                                                                                                                                                                               |
| `optionRefuses`             | `"{option} does not allow \"{key}\""`                                                                                                       | An option (`{option}`) that does not allow a key the value has.                                                                                                                                                                                |
| `optionProblem`             | `"{option}: {problem}"`                                                                                                                     | An option (`{option}`) and another problem it has with the value.                                                                                                                                                                              |
| `optionNumber`              | `"option {number}"`                                                                                                                         | The name of an option with no title or description (`{number}`, from 1).                                                                                                                                                                       |
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

<details>
<summary>Parents that apply onChange late: echoes and resetting with setValue</summary>

A parent that applies `onChange` values late, even in another order (a
store, a fetch per edit), cannot roll the document back: a `value` the
editor sent that has not come back yet is taken as its echo, in any order and however
late, until 5 seconds after a value it sent later came back (one it
skipped before that is then yours). Any other `value` (a revert, a reset, a loaded file) is
applied. The values the editor sent before such a value belong to the text it replaced: switching back to a file applies its text (a parent still applying every late value after one of its own resets with `setValue`, below). `readOnlyRanges` and `hostDiagnostics` passed with an echo are in the echoed text: they are moved to the document. If an echo leaves the parent's state on an older text, with nothing
still on its way to bring it back, `onChange` reports the document again so
both end on it. React applies several values in one render, so the echo that
would bring it back may never be seen: if the state is still on an older
text 250 ms after such an echo, `onChange` reports the document again then. A parent that applies only some values (a debounce keeping
the latest) leaves the others counted as echoes: to reset to a text the
user may have passed through, call `ref.current.setValue(text)`, which
always applies (also before the editor has mounted, from an effect: it is
applied when the editor mounts) and is reported through `onChange` so the
parent's state follows.

</details>

## Ref

```tsx
import { useRef } from "react";
import {
  ConfigEditor,
  type ConfigEditorHandle,
} from "@pngwasi/config-editor-react";

const ref = useRef<ConfigEditorHandle>(null);
<ConfigEditor ref={ref} defaultValue={initial} />;
<ConfigEditor
  value={text}
  onChange={setText}
  hostDiagnostics={[
    { path: "/web/listen", message: "port 80 is taken", source: "validator" },
  ]}
/>;
<ConfigEditor
  value={text}
  onChange={setText}
  schema={schema}
  readOnlyPaths={["/limits"]}
/>; // plus the schema's `readOnly` fields
<ConfigEditor
  value={text}
  onChange={setText}
  schema={schema}
  secrets={{ hint: "Keeps the stored secret" }}
/>; // `«secret 3f2a»` stays atomic

ref.current?.value; // the current document
ref.current?.editor; // the underlying @pngwasi/config-editor instance, once mounted
ref.current?.setValue(saved); // replace the document, always; onChange reports it
ref.current?.unifiedDiff(); // the `diff -u` of the baseline against the document

// Paths: a JSON pointer or keys and indices, the same in every language.
ref.current?.reveal("/web/listen"); // scroll to it, select it, focus; false if absent
ref.current?.revealRange({ start: 0, end: 12 }); // any stretch of the text on screen, a comment included
ref.current?.rangeOf(["web", "listen"]); // { start, end, startLine, startColumn, endLine, endColumn } | null
ref.current?.pathAt(42); // { pointer, segments } | null, at a string index
ref.current?.currentPath; // the caret's path
ref.current?.text; // the text on screen (the editAs language while editing as another)
ref.current?.focus();
ref.current?.save(); // as Ctrl+S: calls onSave; false when nothing was sent
ref.current?.editAs("toml"); // ask again for the editAs prop's language (after an undo across the switch)
ref.current?.isDirty; // the document differs from baseline

// Folding (view only).
ref.current?.foldAll();
ref.current?.unfoldAll();
ref.current?.fold("/clients"); // fold the block its key opens; false if none
ref.current?.unfold("/web/listen"); // open the folds around a field
ref.current?.foldedLines; // [{ startLine, endLine }], one-based
```

### Saving

```tsx
import { useState } from "react";

const [saved, setSaved] = useState(loaded);
const [text, setText] = useState(loaded);
const [dirty, setDirty] = useState(false);
<ConfigEditor
  value={text}
  onChange={setText}
  baseline={saved}
  onSave={async (next) => {
    await putConfig(next);
    setSaved(next); // nothing unsaved any more
  }}
  onDirtyChange={setDirty}
/>;
```

When `baseline` changes while the user has unsaved edits (another client
saved), the editor shows "This file changed outside the editor." (announced; the
focus stays in the text, `F6` moves it to the notice and back) with **Reload**
(the document becomes the new baseline, reported through `onChange`) and
**Keep mine** (or `Escape` with the focus in the notice: the document stays, diffed against the new
baseline). A baseline equal to the document, or to a text the editor sent
(a save coming back while the user typed on, or a late parent), never asks.
A new `value` from the parent keeps the caret and the scroll position.

Ranges and indices refer to the text on screen (the `editAs` language while
editing as another one). `reveal` selects the key and its value when they
share a line, the key alone when the value is a block below it, the value
alone for a list item, and the dash or header of an item holding a block.

### Several documents

```tsx
import { useRef, useState } from "react";
import {
  ConfigDocuments,
  type ConfigDocumentsHandle,
  type DocumentSpec,
} from "@pngwasi/config-editor-react";

const [files, setFiles] = useState<DocumentSpec[]>([
  { id: "main", fileName: "config.yaml", value: mainText, schema },
  {
    id: "tls",
    fileName: "conf.d/tls.toml",
    value: tlsText,
    schema,
    readOnly: true,
  },
]);
const update = (id: string, change: Partial<DocumentSpec>) =>
  setFiles((list) =>
    list.map((file) => (file.id === id ? { ...file, ...change } : file)),
  );
const tabs = useRef<ConfigDocumentsHandle>(null);
<ConfigDocuments
  ref={tabs}
  documents={files}
  onChange={(id, value) => update(id, { value })}
  onSave={async (id, value) => {
    await putFile(id, value);
    update(id, { baseline: value });
  }}
  onClose={({ id, dirty }) => {
    if (dirty && !confirm(`Discard unsaved changes to ${id}?`)) return;
    setFiles((list) => list.filter((file) => file.id !== id));
  }}
/>;
tabs.current?.saveAll(); // the dirty documents; returns their ids
tabs.current?.select("tls");
tabs.current?.activeId;
```

Tabs over one editor per document, each with its own language, baseline,
read-only state, schema and, optionally, settings; switching keeps each document's caret, undo
history, scroll and folds. The parent owns the list (a late parent never
rolls a document back); nothing closes by itself (`onClose` gets
`{ id, dirty }`). `Alt+PageDown` / `Alt+PageUp` switch documents; the tab list
takes arrows, `Home`, `End`, `Enter` and `Delete`. Props: `documents`,
`activeId`, `onSelect`, `onClose` (a document with `closable: false` has no
close button), `onAdd` (the "+" after the tabs, shown only with it: add the
new document, which opens selected), `onChange`, `onSave`, `onDirtyChange`,
`onEditAsChange`, `onSyncChange`, `onProblems` (a document's id and its
editor's own problems), every [setting](#settings) (for all the editors and
the tabs: `colors` takes
the `tab-*` tokens too; live), `label`, `viewClassName`, `className`,
`style`, `onError`. A document's own `settings` go over the view's for its
editor; `hostDiagnostics`, `readOnlyPaths`, `readOnlyRanges` and `secrets`
work per document as on `<ConfigEditor>`. A new `activeId` shows that
document, leaving the focus where it is. `ref.current.editor(id)` returns
that document's editor, an `EditorView` (`null` for an id not listed):
`value`, `rangeOf`, `reveal`, `fold`, `currentPath`, `unifiedDiff`, …; its
its paths, folds and `reveal` answer `null` until the document has been shown once (`value`, `unifiedDiff`, `isDirty` and `save` answer for the document as listed). `ref.current.setValue(id, text)` replaces a document's text, always (a reset to a text the user passed through), and reports it through `onChange`. Per document, `onEditAsChange(id, language)` says the language on screen (once when its editor mounts, then on each change: a switch made, one that waited, undo or redo across a switch) and `onSyncChange(id, inSync)` whether the text on screen is written back to its `value`. A prop that did not change is not sent again, so after an undo stepped back across a switch, `editAs(id, language)` on the ref asks for it again. A document's `editAsText` (with `editAs`) opens its editor with the text the user had on screen (kept from `editor(id).text`), converted back once it is valid; `readOnlyReason` shows why a read-only document is read-only (its tab's tooltip, the notice when the user types, the accessible descriptions). See the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#several-documents).

For example:

```tsx
<ConfigDocuments ref={tabs} documents={files} showFooter theme="nord" />;
tabs.current?.editor("main")?.reveal("/public_address");
```

See the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#several-documents).
To fill a set height (the component's element, styled through its class), give
the view `height: 100%`
(`.config-editor-documents { height: 100%; }`): the tab strip keeps its height
and the shown editor takes the rest; see
[Size](https://www.npmjs.com/package/@pngwasi/config-editor#size).

## Keyboard and screen readers

The keys are those of [`@pngwasi/config-editor`](https://www.npmjs.com/package/@pngwasi/config-editor#keyboard).
`Tab` indents while editing; to leave the editor with the keyboard,
`Ctrl+M` switches `Tab` and `Shift+Tab` to moving the focus (announced, and
shown in the footer as "Tab moves focus" until `Ctrl+M` again), and right
after an `Escape` that had nothing to close one `Tab` or `Shift+Tab` leaves.
The text area's `aria-describedby` holds what is at the caret (each problem's
message, host ones with their source, a read-only value's reason, a secret
placeholder's hint), `F8` announces "Error 2 of 5: …", and `Ctrl+K Ctrl+I`
shows and reads the schema docs of the key at the caret. Nothing to set:

```tsx
<label htmlFor="after">Next field</label>
<ConfigEditor value={text} onChange={setText} />
<input id="after" /> {/* Ctrl+M, Tab (or Escape, Tab) lands here */}
```

## Exports

This package exports its components (`ConfigEditor`, `ConfigDocuments`),
their props and handle types (`ConfigEditorProps`, `ConfigEditorHandle`,
`ConfigDocumentsProps`, `ConfigDocumentsHandle`), and the types of what
they take and return: `EditorSettings`, `EditorStrings`, `ThemeName`,
`ThemeColorMap`, `LanguageName`, `DocumentSpec`, `CloseRequest`,
`ChangeSummary`, `BaselineConflict`, `FieldPath`, `EditorPath`,
`EditorRange`, `LineRange`, `EditorView` (what `editor(id)` returns),
`HostDiagnostic`, `Diagnostic` (a problem the editor reports), `Severity`, `SecretOptions`, `TextRangeInput`.

The helpers come from `@pngwasi/config-editor`, a peer dependency:
`loadConfigEditor`, `languages()`, `themes()`, `languageForFile` and
`registerLanguage` from its main entry, and the tools (`validate`,
`convert`, `parse`, `serialize`, `unifiedDiff`, `changeSummary`, `rangeOf`, `rangesOf`,
`pathAt`, `schemaWarnings`, `presetCss`) from
`@pngwasi/config-editor/tools`, called after `loadConfigEditor()` has
resolved; see the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#pngwasiconfig-editortools).

```ts
import { loadConfigEditor, themes } from "@pngwasi/config-editor";
import { validate } from "@pngwasi/config-editor/tools";

await loadConfigEditor();
const problems = validate("toml", text, schema);
const names = themes().map((theme) => theme.label);
```

## License

MIT. See [LICENSE](LICENSE).
