# @pngwasi/config-editor-react

React component for [`@pngwasi/config-editor`](https://www.npmjs.com/package/@pngwasi/config-editor):
a schema-aware YAML, JSON, TOML, HCL and XML editor running as WebAssembly,
with completion from a JSON Schema, hover docs, validation, a diff gutter,
editing one language as another, and eleven themes.

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
| `onPathChange` | `(path: EditorPath) => void` | — | Called with the path of what the caret is on (`{ pointer: "/web/listen", segments: ["web", "listen"] }`) each time it changes, for a breadcrumb. The path is computed only while this is set. |
| `hostDiagnostics` | `HostDiagnostic[]` | — | Problems the host found (a server's validation): `{ path \| range, severity?, message, code?, source?, docsUrl? }`, shown with the editor's own; see the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#host-diagnostics). Updates live; only the markers change. |
| `schema` | `string` | — | JSON Schema as JSON text: completion, hover docs and validation. |
| `baseline` | `string` | initial value | Text the diff gutter compares against; set it to the saved text to clear the diff. |
| `theme` | `string` | `"vscode-dark"` | A theme slug; see `themes()`. |
| `colors` | `Record<string, string>` | — | Colors overriding the theme's by token (`bg`, `key`, …); see `colorTokens()`. |
| `readOnly` | `boolean` | `false` | View mode. |
| `fileName` | `string` | `untitled.<ext>` | Shown in the toolbar. |
| `showToolbar` / `showFooter` | `boolean` | `false` | Built-in toolbar and status bar. |
| `editorClassName` | `string` | — | Class on the editor root. |
| `className`, `style` | | — | Class and style of the container `<div>`. |
| `onError` | `(error: unknown) => void` | `console.error` | Errors from loading or from a prop the editor rejects (unknown language, theme or color token, invalid schema). They never unmount your tree. |

`value`, `editAs`, `theme`, `colors`, `readOnly`, `baseline`, `hostDiagnostics` and `onPathChange` update the
live editor. `language`, `schema`, `fileName`, `showToolbar`, `showFooter`
and `editorClassName` re-create it; the document and its diff baseline carry
over, undo history starts over.

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

ref.current?.value;   // the current document
ref.current?.editor;  // the underlying @pngwasi/config-editor instance, once mounted
ref.current?.setValue(saved); // replace the document, always; onChange reports it

// Paths: a JSON pointer or keys and indices, the same in every language.
ref.current?.reveal("/web/listen");       // scroll to it, select it, focus; false if absent
ref.current?.rangeOf(["web", "listen"]);  // { start, end, startLine, startColumn, endLine, endColumn } | null
ref.current?.pathAt(42);                   // { pointer, segments } | null, at a string index
ref.current?.currentPath;                  // the caret's path
ref.current?.focus();
```

Ranges and indices refer to the text on screen (the `editAs` language while
editing as another one). `reveal` selects the key and its value when they
share a line, the key alone when the value is a block below it, and the
value alone for a list item.

## Other exports

Re-exported from `@pngwasi/config-editor`; call them after
`loadConfigEditor()` has resolved:

| Export | Description |
| --- | --- |
| `validate(language, text, schema?)` | Syntax and schema problems: `{ line?, column?, endColumn?, message, severity }[]`. |
| `convert(from, text, to, schema?)` | `{ text, losses }`. |
| `registerLanguage(definition)` | Add a language written in JavaScript; see the [`@pngwasi/config-editor` README](https://www.npmjs.com/package/@pngwasi/config-editor#languages-written-in-javascript). |
| `languages()`, `themes()`, `colorTokens()` | Accepted names. |

Types: `ConfigEditorProps`, `ConfigEditorHandle`, `Diagnostic`, `Conversion`,
`FieldPath`, `EditorPath`, `EditorRange`, `HostDiagnostic`, `Severity`,
`CustomLanguage`, `LanguageName`, `ThemeName`, `ThemeColorMap`.

```tsx
import { loadConfigEditor, validate } from "@pngwasi/config-editor-react";

await loadConfigEditor();
const problems = validate("toml", text, schema);
```

## License

MIT. See [LICENSE](LICENSE).
