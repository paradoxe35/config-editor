# @pngwasi/config-editor-react

React component for [`@pngwasi/config-editor`](https://www.npmjs.com/package/@pngwasi/config-editor):
a schema-aware YAML, JSON, TOML, HCL and XML editor running as WebAssembly,
with completion from a JSON Schema, hover docs, validation, a diff gutter,
editing one language as another, and eleven themes.

<!-- toc -->
## Contents

- [Install](#install)
- [Quick start](#quick-start)
- [`<ConfigEditor>` props](#configeditor-props)
- [Ref](#ref)
  - [Saving](#saving)
  - [Several documents](#several-documents)
- [Other exports](#other-exports)
- [License](#license)
<!-- /toc -->

## Install

```sh
npm install @pngwasi/config-editor-react @pngwasi/config-editor
```

`@pngwasi/config-editor` is a peer dependency. React 18 or 19.

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
the package's JavaScript, call `loadConfigEditor` first:

```ts
import { loadConfigEditor } from "@pngwasi/config-editor-react";
// Vite
import wasmUrl from "@pngwasi/config-editor/config_editor_bg.wasm?url";

loadConfigEditor({ module_or_path: wasmUrl });
```

## `<ConfigEditor>` props

| Prop | Type | Default | Purpose |
| --- | --- | --- | --- |
| `value` | `string` | — | The document (controlled). |
| `defaultValue` | `string` | `""` | The initial document when uncontrolled. |
| `onChange` | `(value: string) => void` | — | Called with the document after every edit (after each typing pause while editing as another language). |
| `language` | `string` | `"yaml"` | `"yaml"`, `"json"`, `"toml"`, `"hcl"`, `"xml"`, or a registered one. `value`, `baseline` and `onChange` are always in it. |
| `editAs` | `string \| null` | — | Show and edit the document in another language; it is saved back in `language`, keeping its comments. |
| `onEditAsChange` | `(language: string) => void` | — | Called when an `editAs` switch completes (a switch waits while the document cannot convert, e.g. a `null` to TOML). |
| `onChangeSummary` | `(summary: ChangeSummary) => void` | — | Called with `{ added, removed, modified, hunks }` against `baseline`, on mount and whenever it changes. |
| `onPathChange` | `(path: EditorPath) => void` | — | Called with the path of what the caret is on (`{ pointer: "/web/listen", segments: ["web", "listen"] }`) each time it changes, for a breadcrumb. The path is computed only while this is set. |
| `hostDiagnostics` | `HostDiagnostic[]` | — | Problems the host found (a server's validation): `{ path \| range, severity?, message, code?, source?, docsUrl? }`, shown with the editor's own; see the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#host-diagnostics). Updates live; only the markers change. |
| `readOnlyPaths` | `FieldPath[]` | — | Fields the user cannot change (each a whole subtree), on top of the schema's `readOnly` ones; they hold with `editAs` too. See the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#read-only-fields). Updates live. |
| `readOnlyRanges` | `TextRangeInput[]` | — | Stretches of the document (in `language`) the user cannot change: `{ start, end }` or `{ startLine, startColumn, endLine?, endColumn? }`. Updates live. |
| `secrets` | `SecretOptions` | defaults | `{ placeholder?, isPlaceholder?, hint?, referencePrefixes? }`: secret placeholders (`«secret 3f2a»`) in `writeOnly` / `x-secret` fields stay atomic and are kept unless replaced; see the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#secrets). Changing `placeholder`, `hint` or `referencePrefixes` re-mounts; `isPlaceholder` may change freely. |
| `schema` | `string` | — | JSON Schema as JSON text: completion, hover docs and validation. |
| `onSchemaWarning` | `(message: string) => void` | console warning | Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile. |
| `baseline` | `string` | initial value | Text the diff gutter compares against: the version last saved or loaded. A new one under unsaved edits asks the user (Reload / Keep mine). |
| `onSave` | `(value: string, language: string) => void` | — | Called on `Ctrl+S` / `Cmd+S` (the browser's dialog prevented) and `ref.save()`, with the document in `language`. Nothing saves by itself. Without it, `Ctrl+S` is the browser's. |
| `onDirtyChange` | `(dirty: boolean) => void` | — | Called with whether the document differs from `baseline`: once on mount, then each time that changes. |
| `onBaselineConflict` | `(event: BaselineConflict) => void` | — | Called when a new `baseline` arrives under unsaved edits (`"opened"`) and when that is settled (`"reloaded"`, `"kept-mine"`, `"settled"`). |
| `theme` | `string` | `"vscode-dark"` | A theme slug; see `themes()` and the list in the [`@pngwasi/config-editor` README](../config-editor/README.md#themes). |
| `colors` | `Record<string, string>` | — | Colors overriding the theme's by token (`bg`, `key`, …); see `colorTokens()`. |
| `readOnly` | `boolean` | `false` | View mode. |
| `fileName` | `string` | `untitled.<ext>` | Shown in the toolbar. |
| `showToolbar` / `showFooter` | `boolean` | `false` | Built-in toolbar and status bar. |
| `search` | `boolean` | `true` | Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`); `false` leaves those keys to the browser. |
| `goToLine` | `boolean` | `true` | Go to line (`Ctrl+G`); `false` leaves the key to the browser. |
| `folding` | `boolean` | `true` | Folding by structure: gutter chevrons, `Ctrl+Shift+[` / `Ctrl+Shift+]`, `Ctrl+K Ctrl+0` / `Ctrl+K Ctrl+J` (`Cmd` on macOS). View only: `onChange` and copying see the whole text. |
| `showDiff` | `boolean` | `true` | Line changes against `baseline` in the gutter. |
| `showDiagnostics` | `boolean` | `true` | Syntax and schema problems inline, in the footer and on hover. |
| `completions` | `boolean` | `true` | Schema completion; `false` offers no suggestions, `Ctrl+Space` included. |
| `autoTriggerCompletions` | `boolean` | `true` | Open suggestions while typing; `false` opens them only with `Ctrl+Space`. |
| `hoverDocs` | `boolean` | `true` | Schema docs when hovering a key. |
| `highlighting` | `boolean` | `true` | Syntax colors. |
| `autoClosing` | `boolean` | `true` | Auto-close quotes and brackets, wrap selections, delete empty pairs. |
| `styled` | `boolean` | `true` | Inject the preset stylesheet; `false` leaves the `config-editor__*` markup to your styles (`presetCss()` returns the preset). |
| `saveShortcut` | `boolean` | `true` | Whether `Ctrl+S` / `Cmd+S` saves (with `onSave`); `false` leaves it to the browser and only `ref.save()` saves. |
| `editorClassName` | `string` | — | Class on the editor root. |
| `className`, `style` | | — | Class and style of the container `<div>`. |
| `onError` | `(error: unknown) => void` | `console.error` | Errors from loading or from a prop the editor rejects (unknown language, theme or color token, invalid schema). They never unmount your tree. |

`value`, `editAs`, `theme`, `colors`, `readOnly`, `baseline`, `hostDiagnostics`,
`readOnlyPaths`, `readOnlyRanges`, `saveShortcut`, `onPathChange`, `onSave` and the other callbacks update the
live editor. `language`, `schema`, `fileName`, `showToolbar`, `showFooter`,
`search`, `goToLine`, `folding`, `showDiff`, `showDiagnostics`, `completions`,
`autoTriggerCompletions`, `hoverDocs`, `highlighting`, `autoClosing`, `styled`
and `editorClassName` re-create it; the document and its diff baseline carry
over, undo history starts over.

```tsx
// A plain viewer: no colors, no diff, no suggestions, no popups.
<ConfigEditor value={text} readOnly highlighting={false} showDiff={false}
  completions={false} hoverDocs={false} showDiagnostics={false} />
```

A parent that applies `onChange` values late cannot roll the document back to
an older one: a `value` equal to one the editor sent in the last 10 seconds,
while newer ones are still on their way, is taken as that late echo. Any other `value` (a revert, a reset)
is applied. To reset to a text the user may have just passed through while a
late parent is catching up, call `ref.current.setValue(text)`, which always
applies (also before the editor has mounted, from an effect: it is applied
when the editor mounts): it is reported through `onChange` so the parent's state follows,
echoes of values sent before it are skipped, and if the parent's state took
one of them anyway, `onChange` reports the document again so both end on the
reset.

## Ref

```tsx
import { useRef } from "react";
import { ConfigEditor, type ConfigEditorHandle } from "@pngwasi/config-editor-react";

const ref = useRef<ConfigEditorHandle>(null);
<ConfigEditor ref={ref} defaultValue={initial} />;
<ConfigEditor value={text} onChange={setText}
  hostDiagnostics={[{ path: "/web/listen", message: "port 80 is taken", source: "server" }]} />;
<ConfigEditor value={text} onChange={setText} schema={schema}
  readOnlyPaths={["/limits"]} />; // plus the schema's `readOnly` fields
<ConfigEditor value={text} onChange={setText} schema={schema}
  secrets={{ hint: "Keeps the stored secret" }} />; // `«secret 3f2a»` stays atomic

ref.current?.value;   // the current document
ref.current?.editor;  // the underlying @pngwasi/config-editor instance, once mounted
ref.current?.setValue(saved); // replace the document, always; onChange reports it
ref.current?.unifiedDiff(); // the `diff -u` of the baseline against the document

// Paths: a JSON pointer or keys and indices, the same in every language.
ref.current?.reveal("/web/listen");       // scroll to it, select it, focus; false if absent
ref.current?.rangeOf(["web", "listen"]);  // { start, end, startLine, startColumn, endLine, endColumn } | null
ref.current?.pathAt(42);                   // { pointer, segments } | null, at a string index
ref.current?.currentPath;                  // the caret's path
ref.current?.text;                         // the text on screen (the editAs language while editing as another)
ref.current?.focus();
ref.current?.save();     // as Ctrl+S: calls onSave; false when nothing was sent
ref.current?.isDirty;    // the document differs from baseline

// Folding (view only).
ref.current?.foldAll();
ref.current?.unfoldAll();
ref.current?.fold("/clients");             // fold the block its key opens; false if none
ref.current?.unfold("/web/listen");        // open the folds around a field
ref.current?.foldedLines;                  // [{ startLine, endLine }], one-based
```

### Saving

```tsx
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
saved), the editor shows "The file changed on the server." with **Reload**
(the document becomes the new baseline, reported through `onChange`) and
**Keep mine** (or `Escape`: the document stays, diffed against the new
baseline). A baseline equal to the document, or to a text the editor sent
(a save coming back while the user typed on, or a late parent), never asks.
A new `value` from the parent keeps the caret and the scroll position.

Ranges and indices refer to the text on screen (the `editAs` language while
editing as another one). `reveal` selects the key and its value when they
share a line, the key alone when the value is a block below it, and the
value alone for a list item.

### Several documents

```tsx
import { useRef, useState } from "react";
import { ConfigDocuments, type ConfigDocumentsHandle, type DocumentSpec } from "@pngwasi/config-editor-react";

const [files, setFiles] = useState<DocumentSpec[]>([
  { id: "main", fileName: "burrow.yaml", value: mainText, schema },
  { id: "tls", fileName: "conf.d/tls.toml", value: tlsText, schema, readOnly: true },
]);
const update = (id: string, change: Partial<DocumentSpec>) =>
  setFiles((list) => list.map((file) => (file.id === id ? { ...file, ...change } : file)));
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
read-only state and schema; switching keeps each document's caret, undo
history, scroll and folds. The parent owns the list (a late parent never
rolls a document back); nothing closes by itself (`onClose` gets
`{ id, dirty }`). `Alt+PageDown` / `Alt+PageUp` switch documents; the tab list
takes arrows, `Home`, `End`, `Enter` and `Delete`. Props: `documents`,
`activeId`, `onSelect`, `onClose`, `onChange`, `onSave`, `onDirtyChange`,
`theme`, `colors` (the `tab-*` tokens), `styled`, `showFooter`, `showDiff`,
`saveShortcut`, `label`, `viewClassName`, `className`, `style`, `onError`.
See the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#several-documents).

## Other exports

Re-exported from `@pngwasi/config-editor`; call them after
`loadConfigEditor()` has resolved:

| Export | Description |
| --- | --- |
| `validate(language, text, schema?)` | Syntax and schema problems: `{ line?, column?, endColumn?, message, severity }[]`. |
| `convert(from, text, to, schema?)` | `{ text, losses }`. |
| `unifiedDiff(baseline, current, context?)` | The `diff -u` of two texts (`""` when they have the same lines). |
| `changeSummary(baseline, current)` | `{ added, removed, modified, hunks }` between two texts, as `onChangeSummary` counts them. |
| `parse(language, text)`, `serialize(language, value)` | A document as plain data and back; errors are thrown. |
| `rangeOf(language, text, path)`, `pathAt(language, text, index)` | Where a field is written / the path at a string index, as the ref's `rangeOf` and `pathAt` answer. |
| `foldRanges(language, text)` | The blocks that fold, one-based `{ startLine, endLine }`. |
| `languageInfo(name)`, `languageForFile(path)` | `{ name, label, icon, extensions, defaultFileName }` of a language; the language of a file name. |
| `themeInfo(name)`, `presetCss()` | `{ name, label }` of a theme; the preset stylesheet, for `styled={false}`. |
| `schemaWarnings(schema)` | What the editor cannot use in a schema, as `onSchemaWarning` reports it. |
| `registerLanguage(definition)` | Add a language written in JavaScript; see the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#languages-written-in-javascript). |
| `languages()`, `themes()`, `colorTokens()` | Accepted names. |

Types: `ConfigEditorProps`, `ConfigEditorHandle`, `ConfigDocumentsProps`,
`ConfigDocumentsHandle`, `DocumentSpec`, `DocumentsOptions`, `DocumentsCallbackUpdate`,
`CloseRequest`, `ChangeSummary`, `Diagnostic`, `Conversion`,
`FieldPath`, `EditorPath`, `EditorRange`, `LineRange`, `HostDiagnostic`, `Severity`,
`CustomLanguage`, `LanguageName`, `LanguageInfo`, `ThemeName`, `ThemeInfo`, `ThemeColorMap`,
`BaselineConflict`.

```tsx
import { loadConfigEditor, validate } from "@pngwasi/config-editor-react";

await loadConfigEditor();
const problems = validate("toml", text, schema);
```

## License

MIT. See [LICENSE](LICENSE).
