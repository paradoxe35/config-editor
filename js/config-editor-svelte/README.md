# @pngwasi/config-editor-svelte

Svelte 5 components for [`@pngwasi/config-editor`](https://www.npmjs.com/package/@pngwasi/config-editor):
a schema-aware YAML, JSON, TOML, HCL and XML editor running as WebAssembly,
with completion from a JSON Schema, hover docs, validation, a diff gutter,
editing one language as another, and eleven themes.

<!-- toc -->

## Contents

- [Install](#install)
- [Quick start](#quick-start)
- [`<ConfigEditor>` props](#configeditor-props)
  - [Settings](#settings)
  - [Late parents](#late-parents)
- [The instance](#the-instance)
  - [Saving](#saving)
  - [Several documents](#several-documents)
- [Keyboard and screen readers](#keyboard-and-screen-readers)
- [Exports](#exports)
- [License](#license)
<!-- /toc -->

## Install

```sh
npm install @pngwasi/config-editor-svelte @pngwasi/config-editor
```

`@pngwasi/config-editor` is a peer dependency. Svelte 5 (runes). The package
ships its components as `.svelte` files with TypeScript declarations
(`svelte-package`, the `svelte` export condition), compiled by your app's
Svelte, as any Svelte library.

## Quick start

```svelte
<script lang="ts">
  import { ConfigEditor } from "@pngwasi/config-editor-svelte";

  let { schema }: { schema: string } = $props();
  let text = $state("listen: 7835\n");
</script>

<ConfigEditor bind:value={text} language="yaml" {schema} showToolbar showFooter />
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

**SSR** (SvelteKit): the component renders an empty `<div>` on the server
and mounts the editor in the browser (`onMount`); nothing touches the DOM or
loads the WebAssembly during server rendering. Call `loadConfigEditor` only
in the browser (in `onMount`, or behind `browser` from `$app/environment`).

## `<ConfigEditor>` props

`bind:value` binds the document. The props are the document, its callbacks
(Svelte 5 callback props: `onSave`, `onDirtyChange`, …) and every
[setting](#settings) (`theme`, `showFooter`, `folding`, …): the
`ConfigEditorProps` type extends the generated `EditorSettings`.

| Prop                 | Type                                        | Default          | Purpose                                                                                                                                                                 |
| -------------------- | ------------------------------------------- | ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `value`              | `string`                                    | —                | The document, in `language`: `bind:value`, or one way with `onChange` (a parent that applies the edits late never rolls the document back; see [below](#late-parents)). |
| `defaultValue`       | `string`                                    | `""`             | The initial document when `value` is not given.                                                                                                                         |
| `onChange`           | `(value: string) => void`                   | —                | Called with the document after every edit (after each typing pause while editing as another language).                                                                  |
| `language`           | `string`                                    | `"yaml"`         | `"yaml"`, `"json"`, `"toml"`, `"hcl"`, `"xml"`, or a registered one. Changing it re-mounts.                                                                             |
| `editAs`             | `string \| null`                            | —                | Show and edit the document in another language; it is saved back in `language`, keeping its comments.                                                                   |
| `onEditAsChange`     | `(language: string) => void`                | —                | An `editAs` switch completed, or undo / redo stepped back across one. Label the language shown from this, not from `editAs`.                                            |
| `onChangeSummary`    | `(summary: ChangeSummary) => void`          | —                | `{ added, removed, modified, hunks }` against `baseline`, on mount and whenever it changes.                                                                             |
| `onPathChange`       | `(path: EditorPath) => void`                | —                | The path of what the caret is on, each time it changes, for a breadcrumb. Computed only while set.                                                                      |
| `hostDiagnostics`    | `HostDiagnostic[]`                          | —                | Problems the host found (a server's validation), by path or range, shown with the editor's own. Updates live, also when changed in place.                               |
| `readOnlyPaths`      | `FieldPath[]`                               | —                | Fields the user cannot change (each a whole subtree), on top of the schema's `readOnly` ones. Updates live.                                                             |
| `readOnlyRanges`     | `TextRangeInput[]`                          | —                | Stretches of the document (in `language`) the user cannot change. Updates live.                                                                                         |
| `secrets`            | `SecretOptions`                             | defaults         | Secret placeholders (`«secret 3f2a»`) in `writeOnly` / `x-secret` fields stay atomic. Changing `placeholder`, `hint` or `referencePrefixes` re-mounts.                  |
| `schema`             | `string`                                    | —                | JSON Schema as JSON text: completion, hover docs and validation. Changing it re-mounts.                                                                                 |
| `onSchemaWarning`    | `(message: string) => void`                 | console warning  | Once per thing in `schema` the editor cannot use.                                                                                                                       |
| `baseline`           | `string`                                    | initial value    | Text the diff gutter compares against: the version last saved or loaded. A new one under unsaved edits asks the user (Reload / Keep mine).                              |
| `onSave`             | `(value: string, language: string) => void` | —                | `Ctrl+S` / `Cmd+S` (the browser's dialog prevented) and `save()`. Nothing saves by itself. Without it, `Ctrl+S` is the browser's.                                       |
| `onDirtyChange`      | `(dirty: boolean) => void`                  | —                | Once on mount, then each time whether the document differs from `baseline` changes.                                                                                     |
| `onBaselineConflict` | `(event: BaselineConflict) => void`         | —                | A new `baseline` arrived under unsaved edits (`"opened"`) and when that is settled (`"reloaded"`, `"kept-mine"`, `"settled"`).                                          |
| `readOnly`           | `boolean`                                   | `false`          | View mode.                                                                                                                                                              |
| `fileName`           | `string`                                    | `untitled.<ext>` | Shown in the toolbar. Changing it re-mounts.                                                                                                                            |
| `editorClass`        | `string`                                    | —                | Class on the editor root. Changing it re-mounts.                                                                                                                        |
| `class`, `style`     | `string`                                    | —                | Class and style of the component's `<div>`.                                                                                                                             |
| `onError`            | `(error: unknown) => void`                  | `console.error`  | Errors from loading or from a prop the editor rejects (unknown language, theme or color token, invalid schema).                                                         |

`value`, `editAs`, `readOnly`, `baseline`, `hostDiagnostics`,
`readOnlyPaths`, `readOnlyRanges`, every setting and every callback update
the live editor (the caret, undo history and folds stay); a `$state` list or
object changed in place counts. `language`, `schema`, `fileName`,
`editorClass` and the `secrets` patterns re-create it; the document, its diff
baseline and the caret (when it had focus) carry over, undo history starts
over.

### Settings

The theme, the colors and every on/off switch, each a prop with a default.
Changing one applies live, and one no longer passed returns to its
default.

```svelte
<!-- A plain viewer: no colors, no diff, no suggestions, no popups. -->
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

`strings` rewords or translates every built-in text; give only the texts to
change (the others stay English), and new texts apply live:

```svelte
<ConfigEditor
  bind:value={text}
  editAs="json"
  strings={{ noProblems: "Keine Fehler", editAsDismiss: "Schließen" }}
/>
```

Placeholders are written in braces (`{count}`, `{line}`); counts come in
pairs, `…One` for 1 and `…Other` otherwise.

<!-- strings:start -->

<!-- Generated from crates/config-editor-leptos/src/strings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`. -->

<details>
<summary>Every text and its default (152 texts)</summary>

| Key                         | Default                                                                                                    |                                                                                                                              |
| --------------------------- | ---------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `suggestionsHint`           | `"{key} for suggestions"`                                                                                  | The footer's suggestions hint; `{key}` is where the chord goes.                                                              |
| `suggestionsKey`            | `"Ctrl+Space"`                                                                                             | The chord that opens suggestions, as the footer draws it: keys between `+`.                                                  |
| `noProblems`                | `"No errors"`                                                                                              | The footer's problem count when there are none.                                                                              |
| `errorsOne`                 | `"{count} error"`                                                                                          | One error, in the footer's problem count.                                                                                    |
| `errorsOther`               | `"{count} errors"`                                                                                         | Several errors, in the footer's problem count.                                                                               |
| `warningsOne`               | `"{count} warning"`                                                                                        | One warning, in the footer's problem count.                                                                                  |
| `warningsOther`             | `"{count} warnings"`                                                                                       | Several warnings, in the footer's problem count.                                                                             |
| `infosOne`                  | `"{count} info"`                                                                                           | One info, in the footer's problem count.                                                                                     |
| `infosOther`                | `"{count} info"`                                                                                           | Several infos, in the footer's problem count.                                                                                |
| `hintsOne`                  | `"{count} hint"`                                                                                           | One hint, in the footer's problem count.                                                                                     |
| `hintsOther`                | `"{count} hints"`                                                                                          | Several hints, in the footer's problem count.                                                                                |
| `problemsHint`              | `"hover or focus underlined text"`                                                                         | After the problem count when every problem has its place.                                                                    |
| `problemsCut`               | `"only the first {count} are shown"`                                                                       | After the problem count when the list was cut; `{count}` is how many are shown.                                              |
| `problemsMore`              | `"Only the first {count} problems are shown; more follow."`                                                | The problem that says the list was cut, after `{count}` problems.                                                            |
| `changesOne`                | `"{count} change"`                                                                                         | The footer's change count against the baseline, for one change.                                                              |
| `changesOther`              | `"{count} changes"`                                                                                        | The footer's change count, for several changes.                                                                              |
| `changeDetailsOne`          | `"{lines} line · Alt+F5 for the next"`                                                                     | The change count's tooltip when one line changed; `{lines}` lists the added, modified and removed counts.                    |
| `changeDetailsOther`        | `"{lines} lines · Alt+F5 for the next"`                                                                    | The change count's tooltip when several lines changed.                                                                       |
| `changedAdded`              | `"{count} added"`                                                                                          | Added lines, in the change count's tooltip.                                                                                  |
| `changedModified`           | `"{count} modified"`                                                                                       | Modified lines, in the change count's tooltip.                                                                               |
| `changedRemoved`            | `"{count} removed"`                                                                                        | Removed lines, in the change count's tooltip.                                                                                |
| `linesOne`                  | `"{count} line"`                                                                                           | The footer's line count, for one line.                                                                                       |
| `linesOther`                | `"{count} lines"`                                                                                          | The footer's line count, for several lines.                                                                                  |
| `tabFocus`                  | `"Tab moves focus"`                                                                                        | The footer's indicator while Tab moves the focus.                                                                            |
| `tabFocusTitle`             | `"Tab moves focus (Ctrl+M to indent with Tab)"`                                                            | Its tooltip; clicking it turns the mode off.                                                                                 |
| `tabFocusOn`                | `"Tab moves focus: Tab and Shift+Tab now leave the editor. Ctrl+M to indent with Tab again."`              | Announced when `Ctrl+M` makes Tab move the focus.                                                                            |
| `tabFocusOff`               | `"Tab indents: Tab and Shift+Tab indent again. Ctrl+M to move focus with Tab."`                            | Announced when `Ctrl+M` makes Tab indent again.                                                                              |
| `schemaConnected`           | `"Schema connected"`                                                                                       | The toolbar's schema state.                                                                                                  |
| `editAsHostComments`        | `"The {host} file's comments and formatting are not shown while editing as {edit}; saving keeps them."`    | The Edit-as notice when the host's file has comments, which the edited language does not show but saving keeps.              |
| `editAsTypedComments`       | `"Comments typed in {edit} are not saved: the {host} file keeps only its own comments."`                   | The Edit-as notice when comments are typed in the edited language, which saving drops, and the host's language has comments. |
| `editAsTypedCommentsNone`   | `"Comments typed in {edit} are not saved: {host} has no comments."`                                        | The same when the host's language has no comments (JSON).                                                                    |
| `editAsDismiss`             | `"Dismiss"`                                                                                                | The Edit-as notice's close button.                                                                                           |
| `baselineChanged`           | `"The file changed on the server."`                                                                        | The notice shown when the host's baseline changes under unsaved edits.                                                       |
| `baselineChangedDetail`     | `"Reload it, losing your changes, or keep your text and compare it with the new version."`                 | What the notice's choices do, after its message.                                                                             |
| `reload`                    | `"Reload"`                                                                                                 | The notice's button that takes the new baseline as the text.                                                                 |
| `reloadTitle`               | `"Replace your text with the new version (Ctrl+Z brings yours back)"`                                      | Its tooltip.                                                                                                                 |
| `keepMine`                  | `"Keep mine"`                                                                                              | The notice's button that keeps the text against the new baseline.                                                            |
| `keepMineTitle`             | `"Keep your text; the changes are now shown against the new version (Escape)"`                             | Its tooltip.                                                                                                                 |
| `baselineChangedKeys`       | `"F6 moves to the choices and back."`                                                                      | How to reach the notice's choices from the keyboard, shown on it (and announced with it).                                    |
| `findWidget`                | `"Find and replace"`                                                                                       | The find widget's accessible name.                                                                                           |
| `toggleReplace`             | `"Toggle replace"`                                                                                         | The button showing the replace row.                                                                                          |
| `toggleReplaceTitle`        | `"Toggle replace (Ctrl+H)"`                                                                                | Its tooltip.                                                                                                                 |
| `find`                      | `"Find"`                                                                                                   | The query field's accessible name and placeholder.                                                                           |
| `matchCase`                 | `"Match case"`                                                                                             | The match-case toggle.                                                                                                       |
| `matchCaseTitle`            | `"Match case (Alt+C)"`                                                                                     | Its tooltip.                                                                                                                 |
| `wholeWord`                 | `"Match whole word"`                                                                                       | The whole-word toggle.                                                                                                       |
| `wholeWordTitle`            | `"Match whole word (Alt+W)"`                                                                               | Its tooltip.                                                                                                                 |
| `useRegex`                  | `"Use regular expression"`                                                                                 | The regular-expression toggle.                                                                                               |
| `useRegexTitle`             | `"Use regular expression (Alt+R)"`                                                                         | Its tooltip.                                                                                                                 |
| `previousMatch`             | `"Previous match"`                                                                                         | The previous-match button.                                                                                                   |
| `previousMatchTitle`        | `"Previous match (Shift+Enter)"`                                                                           | Its tooltip.                                                                                                                 |
| `nextMatch`                 | `"Next match"`                                                                                             | The next-match button.                                                                                                       |
| `nextMatchTitle`            | `"Next match (Enter)"`                                                                                     | Its tooltip.                                                                                                                 |
| `close`                     | `"Close"`                                                                                                  | The find widget's close button.                                                                                              |
| `closeTitle`                | `"Close (Escape)"`                                                                                         | Its tooltip.                                                                                                                 |
| `replace`                   | `"Replace"`                                                                                                | The replacement field's accessible name and placeholder, and the button replacing the current match.                         |
| `replaceTitle`              | `"Replace (Enter)"`                                                                                        | The replace button's tooltip.                                                                                                |
| `replaceAll`                | `"All"`                                                                                                    | The replace-all button.                                                                                                      |
| `replaceAllLabel`           | `"Replace all"`                                                                                            | Its accessible name.                                                                                                         |
| `replaceAllTitle`           | `"Replace all (Ctrl+Alt+Enter)"`                                                                           | Its tooltip.                                                                                                                 |
| `matchPosition`             | `"{current} of {total}"`                                                                                   | The match count: match `{current}` of `{total}`.                                                                             |
| `noResults`                 | `"No results"`                                                                                             | The match count when nothing matches.                                                                                        |
| `invalidPattern`            | `"Invalid pattern"`                                                                                        | The match count when the regular expression is invalid.                                                                      |
| `replacedOne`               | `"Replaced {count} occurrence"`                                                                            | After Replace All, for one match.                                                                                            |
| `replacedOther`             | `"Replaced {count} occurrences"`                                                                           | After Replace All, for several.                                                                                              |
| `replaceSkipped`            | `"; skipped {count} in read-only text or secret placeholders"`                                             | Added when Replace All left matches in protected text alone.                                                                 |
| `nothingReplaced`           | `"Every match is in read-only text; nothing was replaced"`                                                 | After Replace All when every match is in read-only text.                                                                     |
| `goToLine`                  | `"Go to line"`                                                                                             | The go-to-line field's accessible name.                                                                                      |
| `goToLinePlaceholder`       | `"line or line:column"`                                                                                    | Its placeholder.                                                                                                             |
| `goToLineHint`              | `"Line 1–{lines}, or line:column"`                                                                         | Its hint: the range of lines.                                                                                                |
| `goToLineInvalid`           | `"Type a line number between 1 and {lines}"`                                                               | Its hint when the text is no position.                                                                                       |
| `addedLine`                 | `"Added line"`                                                                                             | The change preview's title for an added line.                                                                                |
| `modifiedLine`              | `"Modified line"`                                                                                          | For a modified line.                                                                                                         |
| `removedLines`              | `"Removed lines"`                                                                                          | For removed lines.                                                                                                           |
| `removedLinesAtEnd`         | `"Removed lines at end of document"`                                                                       | For lines removed after the last one.                                                                                        |
| `showAdded`                 | `"Show added line {line}"`                                                                                 | The gutter marker of an added line.                                                                                          |
| `showAddedTitle`            | `"Show added line"`                                                                                        | Its tooltip.                                                                                                                 |
| `showModified`              | `"Show before and after for modified line {line}"`                                                         | The gutter marker of a modified line.                                                                                        |
| `showModifiedTitle`         | `"Show before and after"`                                                                                  | Its tooltip.                                                                                                                 |
| `showRemoved`               | `"Show removed lines before line {line}"`                                                                  | The gutter marker of removed lines.                                                                                          |
| `showRemovedTitle`          | `"Show removed lines"`                                                                                     | Its tooltip, and that of lines removed at the end.                                                                           |
| `showRemovedAtEnd`          | `"Show removed lines at end of document"`                                                                  | The gutter marker of lines removed after the last one.                                                                       |
| `previewHeading`            | `"{title} · line {line}"`                                                                                  | The change preview's heading: its `{title}` and `{line}`.                                                                    |
| `previewLabel`              | `"{title}, line {line}"`                                                                                   | The change preview's accessible name.                                                                                        |
| `revert`                    | `"Revert"`                                                                                                 | The preview's revert button.                                                                                                 |
| `revertTitle`               | `"Revert this change (R)"`                                                                                 | Its tooltip, for a one-line change.                                                                                          |
| `revertLinesTitle`          | `"Revert this change (lines {first}–{last}) (R)"`                                                          | Its tooltip, for a change over lines `{first}` to `{last}`.                                                                  |
| `revertLineAndRemovedTitle` | `"Revert this change (line {line} and the removed lines) (R)"`                                             | Its tooltip, for a changed line with removed lines.                                                                          |
| `closePreview`              | `"Close change preview"`                                                                                   | The preview's close button.                                                                                                  |
| `before`                    | `"Before"`                                                                                                 | The preview's column of the baseline's lines.                                                                                |
| `current`                   | `"Current"`                                                                                                | The preview's column of the current lines.                                                                                   |
| `fold`                      | `"Fold line {line}"`                                                                                       | The chevron of an open range.                                                                                                |
| `unfold`                    | `"Unfold lines {first}–{last}"`                                                                            | The chevron or chip of a closed fold.                                                                                        |
| `foldedChanges`             | `"Folded lines have changes"`                                                                              | A closed fold's line number when it hides changes or problems.                                                               |
| `required`                  | `"required"`                                                                                               | The hover card's mark of a required key.                                                                                     |
| `deprecated`                | `"deprecated"`                                                                                             | The hover card's mark of a deprecated key.                                                                                   |
| `defaultValue`              | `"Default"`                                                                                                | The hover card's default row.                                                                                                |
| `examples`                  | `"Examples"`                                                                                               | The hover card's examples row.                                                                                               |
| `documentation`             | `"Documentation"`                                                                                          | The hover card's link to the schema's documentation.                                                                         |
| `openDocs`                  | `"Open docs ↗ (Alt+Enter)"`                                                                                | The link under a host problem's message.                                                                                     |
| `error`                     | `"Error"`                                                                                                  | An error, as a screen reader hears it.                                                                                       |
| `warning`                   | `"Warning"`                                                                                                | A warning, as a screen reader hears it.                                                                                      |
| `info`                      | `"Info"`                                                                                                   | An info, as a screen reader hears it.                                                                                        |
| `hint`                      | `"Hint"`                                                                                                   | A hint, as a screen reader hears it.                                                                                         |
| `problemSource`             | `" from {source}"`                                                                                         | Who reported a host problem: the `{source}` of the problem texts below (empty for the editor's own problems).                |
| `problem`                   | `"{severity}{source}: {message}"`                                                                          | A problem at the caret, for screen readers.                                                                                  |
| `problemPosition`           | `"{severity} {position} of {total}{source}: {message}"`                                                    | Announced on `F8`: problem `{position}` of `{total}`.                                                                        |
| `problemCode`               | `" Code {code}."`                                                                                          | After a problem's text for screen readers: its `{code}` (from the host).                                                     |
| `problemDocs`               | `" Documentation: {url} (Alt+Enter opens it)."`                                                            | After a problem's text for screen readers: its documentation's `{url}`, and the key that opens it.                           |
| `problemMarker`             | `"{language} {severity}{source} on line {line}: {message}"`                                                | An underlined problem's accessible name.                                                                                     |
| `protectedNote`             | `"{label}: {message}"`                                                                                     | What protected text at the caret is, for screen readers: its `{label}` and `{message}`.                                      |
| `sourceLabel`               | `"{language} source"`                                                                                      | The text area.                                                                                                               |
| `suggestionsLabel`          | `"{language} suggestions"`                                                                                 | The suggestions list.                                                                                                        |
| `problemsLabel`             | `"Inline {language} diagnostics"`                                                                          | The layer of underlined problems.                                                                                            |
| `editorLabel`               | `"{language} editor"`                                                                                      | The editor's frame.                                                                                                          |
| `readOnlyField`             | `"This field is read-only"`                                                                                | Why a field is read-only when neither the schema nor the host says.                                                          |
| `readOnlyText`              | `"This text is read-only"`                                                                                 | Why text in a read-only range the host gave cannot change.                                                                   |
| `holdsReadOnly`             | `"Holds a read-only field: {reason}"`                                                                      | Why renaming or moving the key above a read-only field is refused.                                                           |
| `saveRefused`               | `"{pointer} is read-only and this change would alter it"`                                                  | A save refused because it would change a read-only value.                                                                    |
| `readOnlyLabel`             | `"Read-only"`                                                                                              | The gutter lock's label, before the reason.                                                                                  |
| `secretLabel`               | `"Secret placeholder"`                                                                                     | The gutter key's label, before the hint.                                                                                     |
| `secretHint`                | `"Keeps the current secret unless you replace it"`                                                         | The hover text of a secret placeholder, unless the host gives one.                                                           |
| `secretValue`               | `"the secret value"`                                                                                       | What a schema problem says instead of a secret field's value.                                                                |
| `unknownPlaceholder`        | `"{pointer}: unknown secret placeholder; it stands for no current secret, so write the new value instead"` | A placeholder the host did not give, in a secret field.                                                                      |
| `movedPlaceholder`          | `"{pointer}: this placeholder stands for the secret of {owner}, not this field's; here it is plain text"`  | One of the host's placeholders outside the field it stands in.                                                               |
| `envReference`              | `"Read the secret from an environment variable"`                                                           | The detail of the `${…}` reference suggestion.                                                                               |
| `fileReference`             | `"Read the secret from a file"`                                                                            | The detail of the `file:` reference suggestion.                                                                              |
| `otherReference`            | `"A reference starting with {prefix}"`                                                                     | The detail of another reference suggestion.                                                                                  |
| `referenceType`             | `"reference"`                                                                                              | The type column of a reference suggestion.                                                                                   |
| `closesTag`                 | `"Closes <{name}>"`                                                                                        | The detail of the end tag XML suggests.                                                                                      |
| `endTag`                    | `"end tag"`                                                                                                | Its type column.                                                                                                             |
| `keyNotAllowed`             | `"{pointer}: '{key}' is not allowed here"`                                                                 | A key the schema does not allow.                                                                                             |
| `textNotAllowed`            | `"{pointer}: text is not allowed here, only elements"`                                                     | Text in an element the schema wants elements in (XML).                                                                       |
| `keyDeprecated`             | `"{pointer}: '{key}' is deprecated"`                                                                       | A deprecated key.                                                                                                            |
| `keyDeprecatedInstead`      | `"{pointer}: '{key}' is deprecated. {message}"`                                                            | A deprecated key whose schema says what to use instead.                                                                      |
| `notOneOf`                  | `"{value} is not one of {allowed}"`                                                                        | A value not among the allowed ones.                                                                                          |
| `wrongType`                 | `"{value} is not of type {types}"`                                                                         | A value of none of the accepted types.                                                                                       |
| `invalidSchema`             | `"Invalid JSON Schema: {error}"`                                                                           | A schema that does not compile.                                                                                              |
| `hostCannotKeep`            | `"{pointer}: {host} cannot keep this value exactly; it reads back differently"`                            | A value the host's language reads back differently (Edit-as).                                                                |
| `editCannotShow`            | `"{pointer}: {edit} cannot show this value exactly; editing it here may change it"`                        | A value the edited language cannot show exactly (Edit-as).                                                                   |
| `notSaved`                  | `"Not saved as {host}: {error}"`                                                                           | An edit the host's language cannot save (Edit-as).                                                                           |
| `noNull`                    | `"{host} has no null value (at {pointer})"`                                                                | A null the host's language cannot write (Edit-as).                                                                           |
| `noNonFinite`               | `"{host} has no infinity or NaN (at {pointer})"`                                                           | An infinity or NaN the host's language has no number for (Edit-as).                                                          |
| `cannotEditAs`              | `"Cannot edit this {host} as {edit}: {error}"`                                                             | A host document that does not convert for editing.                                                                           |
| `cannotSwitch`              | `"Cannot switch to {edit} yet: {error}"`                                                                   | An Edit-as switch waiting until the document converts.                                                                       |
| `mixedContent`              | `"{pointer}: text beside child elements cannot be shown in another language"`                              | XML text beside child elements, which no other language can show (Edit-as).                                                  |
| `tabList`                   | `"Open files"`                                                                                             | The tab list's accessible name, unless the host gives one.                                                                   |
| `noDocuments`               | `"No open files"`                                                                                          | Shown in place of an editor while no document is open.                                                                       |
| `closeTab`                  | `"Close {name}"`                                                                                           | A tab's close button.                                                                                                        |
| `closeTabTitle`             | `"Close (Delete)"`                                                                                         | Its tooltip.                                                                                                                 |
| `unsaved`                   | `"unsaved changes"`                                                                                        | The dirty dot's tooltip, and what a tab's accessible name adds for it.                                                       |
| `readOnlyTab`               | `"read-only"`                                                                                              | What a read-only document's tab adds to its accessible name.                                                                 |

</details>

<!-- strings:end -->

### Late parents

`bind:value` is immediate. A parent that applies the edits late (a store, a
debounce, a server round trip) passes `value` one way and updates it from
`onChange`; it cannot roll the document back to an older one: a `value`
equal to one the editor sent in the last 10 seconds, while newer ones are
still on their way, is taken as that late echo. Any other value (a revert, a
reset) is applied. To reset to a text the user may have just passed through
while a late parent is catching up, call `setValue(text)` on the instance,
which always applies (also before the editor has mounted) and is reported
through `value` and `onChange`.

```svelte
<ConfigEditor value={$config} onChange={(next) => config.saveDebounced(next)} />
```

## The instance

Svelte components export functions: `bind:this` gives them.

```svelte
<script lang="ts">
  import { ConfigEditor } from "@pngwasi/config-editor-svelte";

  let text = $state("listen: 7835\n");
  let editor: ReturnType<typeof ConfigEditor> | undefined = $state();
</script>

<ConfigEditor bind:this={editor} bind:value={text} />
<button onclick={() => editor?.reveal(["web", "listen"])}>Go to the port</button>
```

| Function                                       | Description                                                                                                |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `getValue()`                                   | The current document, in `language`.                                                                       |
| `getEditor()`                                  | The underlying `@pngwasi/config-editor` instance, once mounted.                                            |
| `setValue(text)`                               | Replace the document, always; reported through `value` and `onChange`.                                     |
| `unifiedDiff(context?)`                        | The `diff -u` of the baseline against the document.                                                        |
| `reveal(path)`                                 | Scroll to a field, select it, focus; `false` if absent. A path is a JSON pointer or keys and indices.      |
| `rangeOf(path)`, `pathAt(index)`               | Where a field is written in the text on screen / the path at a string index.                               |
| `getCurrentPath()`, `getText()`                | The caret's path; the text on screen (the `editAs` language while editing as another).                     |
| `fold(path)`, `unfold(path)`                   | Fold the block a key opens / open the folds around a field; `false` when there was nothing to do.          |
| `foldAll()`, `unfoldAll()`, `getFoldedLines()` | Every block; the folds, one-based `{ startLine, endLine }`.                                                |
| `focus()`                                      | Focus the editor, keeping its selection.                                                                   |
| `save()`, `isDirty()`                          | As `Ctrl+S` (calls `onSave`; `false` when nothing was sent); whether the document differs from `baseline`. |

They are the members of `ConfigEditorHandle` (React's ref, Vue's template
ref), a getter `x` written `getX()` (Svelte exports functions only).

### Saving

```svelte
<script lang="ts">
  import { ConfigEditor } from "@pngwasi/config-editor-svelte";

  let { loaded }: { loaded: string } = $props();
  let saved = $state(loaded);
  let text = $state(loaded);
  let dirty = $state(false);
</script>

<ConfigEditor
  bind:value={text}
  baseline={saved}
  onSave={async (next) => {
    await putConfig(next);
    saved = next; // nothing unsaved any more
  }}
  onDirtyChange={(next) => (dirty = next)}
/>
```

When `baseline` changes while the user has unsaved edits (another client
saved), the editor asks: **Reload** or **Keep mine**. A baseline equal to the
document, or to a text the editor sent, never asks.

### Several documents

```svelte
<script lang="ts">
  import {
    type CloseRequest,
    ConfigDocuments,
    type DocumentSpec,
  } from "@pngwasi/config-editor-svelte";

  let files: DocumentSpec[] = $state([
    { id: "main", fileName: "burrow.yaml", value: mainText, schema },
    { id: "tls", fileName: "conf.d/tls.toml", value: tlsText, readOnly: true },
  ]);
  let tabs: ReturnType<typeof ConfigDocuments> | undefined = $state();
  const find = (id: string) => files.find((file) => file.id === id)!;
  function close({ id, dirty }: CloseRequest) {
    if (dirty && !confirm(`Discard unsaved changes to ${id}?`)) return;
    files = files.filter((file) => file.id !== id);
  }
</script>

<ConfigDocuments
  bind:this={tabs}
  documents={files}
  theme="nord"
  onChange={(id, value) => (find(id).value = value)}
  onSave={async (id, value) => {
    await putFile(id, value);
    find(id).baseline = value;
  }}
  onClose={close}
/>
<button onclick={() => tabs?.saveAll()}>Save all</button>
```

Tabs over one editor per document, each with its own language, baseline,
read-only state, schema and, optionally, settings; switching keeps each
document's caret, undo history, scroll and folds. The parent owns the list,
changed in place or replaced (a late parent never rolls a document back);
nothing closes by itself. A new `activeId` shows that document, leaving the
focus where it is. Props: `documents`, `activeId`, `onSelect`,
`onClose` (without it, tabs have no close button), `onChange`, `onSave`,
`onDirtyChange`, `label`, `viewClass`, `class`, `style`, `onError` and
every [setting](#settings). Exported functions: `select(id)`, `isDirty(id)`,
`save(id)`, `saveAll()`, `focus()`, `getActiveId()`, `getInstance()`, and
`editor(id)`, the editor of a document, an `EditorView` (`value`, `reveal`,
`rangeOf`, `fold`, `unifiedDiff`, …). See
the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#several-documents).

## Keyboard and screen readers

The keys are those of [`@pngwasi/config-editor`](../config-editor/README.md#keyboard).
`Tab` indents while editing; `Ctrl+M` switches `Tab` to moving the focus,
and right after an `Escape` that had nothing to close one `Tab` leaves.

## Exports

This package exports its components (`ConfigEditor`, `ConfigDocuments`),
their props and handle types (`ConfigEditorProps`, `ConfigEditorHandle`,
`ConfigDocumentsProps`, `ConfigDocumentsHandle`), and the types of what
they take and return: `EditorSettings`, `EditorStrings`, `ThemeName`,
`ThemeColorMap`, `LanguageName`, `DocumentSpec`, `CloseRequest`,
`ChangeSummary`, `BaselineConflict`, `FieldPath`, `EditorPath`,
`EditorRange`, `LineRange`, `EditorView` (what `editor(id)` returns),
`HostDiagnostic`, `Severity`, `SecretOptions`, `TextRangeInput`.

The helpers come from `@pngwasi/config-editor`, a peer dependency:
`loadConfigEditor`, `languages()`, `themes()`, `languageForFile` and
`registerLanguage` from its main entry, and the tools (`validate`,
`convert`, `parse`, `serialize`, `unifiedDiff`, `changeSummary`, `rangeOf`,
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
