# @pngwasi/config-editor

A schema-aware config editor for the browser: YAML, JSON, TOML, HCL and XML,
with completion from a JSON Schema, hover docs, validation, a diff gutter,
editing one language as another, and eleven themes. It is written in Rust
([`config-editor-leptos`](https://crates.io/crates/config-editor-leptos))
and ships as WebAssembly; this package is its framework-free JavaScript API.

For React, use [`@pngwasi/config-editor-react`](https://www.npmjs.com/package/@pngwasi/config-editor-react),
which builds on this package.

## Install

```sh
npm install @pngwasi/config-editor
```

Browsers only (ES modules + WebAssembly). The `.wasm` file is about 1.5 MB
gzipped and loads once per page.

## Quick start

```ts
import init, { ConfigEditor } from "@pngwasi/config-editor";

await init();

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

`init()` fetches `config_editor_bg.wasm` next to the package's JavaScript,
which works with bundlers that copy it (Vite, webpack 5, Parcel). When yours
does not, pass its URL:

```js
// Vite
import wasmUrl from "@pngwasi/config-editor/config_editor_bg.wasm?url";
await init({ module_or_path: wasmUrl });

// Any server: copy node_modules/@pngwasi/config-editor/pkg/config_editor_bg.wasm
await init({ module_or_path: "/assets/config_editor_bg.wasm" });
```

Call `init()` once, before creating editors or calling any other function.

## `new ConfigEditor(element, options)`

Mounts an editor into `element` (an `HTMLElement`; its previous content is
replaced, editors included). Options are type-checked: a value of the wrong type
throws, and an unknown option name is reported in the console.

| Option           | Type                         | Default          | Purpose                                                                                                                                             |
| ---------------- | ---------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `language`       | `string`                     | `"yaml"`         | Language of the document: `"yaml"`, `"json"`, `"toml"`, `"hcl"`, `"xml"`, or a registered one. `value`, `baseline` and `onChange` are always in it. |
| `value`          | `string`                     | `""`             | The document.                                                                                                                                       |
| `editAs`         | `string`                     | —                | Show and edit the document in another language; it is saved back in `language`, keeping its comments.                                               |
| `schema`         | `string`                     | —                | JSON Schema as JSON text: completion, hover docs and validation.                                                                                    |
| `baseline`       | `string`                     | `value`          | Text the diff gutter compares against.                                                                                                              |
| `theme`          | `string`                     | `"vscode-dark"`  | A theme slug; see `themes()`.                                                                                                                       |
| `colors`         | `Record<string, string>`     | —                | CSS colors overriding the theme's, by token (`bg`, `key`, `string`, `selection`, …); see `colorTokens()`.                                           |
| `readOnly`       | `boolean`                    | `false`          | View mode: navigation, selection, diagnostics and hover keep working.                                                                               |
| `fileName`       | `string`                     | `untitled.<ext>` | Shown in the toolbar.                                                                                                                               |
| `showToolbar`    | `boolean`                    | `false`          | File name, schema state and language.                                                                                                               |
| `showFooter`     | `boolean`                    | `false`          | Problem counts and line count.                                                                                                                      |
| `className`      | `string`                     | —                | Extra class on the editor root.                                                                                                                     |
| `onChange`       | `(value: string) => void`    | —                | Called with the document after every edit (after each typing pause while editing as another language).                                              |
| `onEditAsChange` | `(language: string) => void` | —                | Called when an `editAs` switch completes. A switch waits while the document cannot convert yet (TOML has no `null`).                                |

An unknown language, theme or color token, or an invalid schema, throws an
`Error` saying which.

### Instance members

| Member                                         | Description                                                                                                                        |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| `value` (getter)                               | The current document, in `language`. Still readable after `destroy()`.                                                             |
| `setValue(text)`                               | Replace the document (for example after loading a file). The caret stays on the same text when possible.                           |
| `setBaseline(text)`                            | Set the diff baseline; pass the saved text to clear the diff.                                                                      |
| `setEditAs(language \| null)` | Switch the editing language live; `null` edits in `language` again. The language must be able to write documents (a registered one needs `serialize`). |
| `setTheme(name \| null)`                       | Switch theme; `null` returns to the default.                                                                                       |
| `setColors(colors)`                            | Replace the color overrides (`{}` clears them).                                                                                    |
| `setReadOnly(boolean)`                         | Toggle view mode.                                                                                                                  |
| `setCallbacks({ onChange?, onEditAsChange? })` | Replace callbacks; `null` removes one, an absent key keeps it.                                                                     |
| `destroy()`                                    | Unmount. An edit still converting (while editing as another language) is sent to `onChange` first. Safe to call inside `onChange`. |
| `free()`                                       | Release the WebAssembly object after `destroy()`. Optional; the garbage collector does it too. An editor stays on the page until `destroy()`, whether or not you keep a reference to it. |

An exception thrown by a callback is logged to the console and does not
break the editor.

## Functions

| Function                            | Returns            | Description                                                                                                                                                                                                                                                      |
| ----------------------------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `init(input?)`                      | `Promise`          | Load the WebAssembly (default export).                                                                                                                                                                                                                           |
| `validate(language, text, schema?)` | `Diagnostic[]`     | Syntax problems, plus schema problems when a schema is given.                                                                                                                                                                                                    |
| `convert(from, text, to, schema?)`  | `{ text, losses }` | Convert between languages. `losses` lists JSON pointers of values the target reads back differently. Throws when the text does not parse or the target cannot express it. With a schema, a lone XML element where it expects a list converts as a one-item list. |
| `languages()`                       | `string[]`         | Built-in and registered language names.                                                                                                                                                                                                                          |
| `themes()`                          | `string[]`         | Theme slugs.                                                                                                                                                                                                                                                     |
| `colorTokens()`                     | `string[]`         | Tokens accepted by `colors`.                                                                                                                                                                                                                                     |
| `registerLanguage(definition)`      | `string`           | Add a language written in JavaScript (below); returns its name.                                                                                                                                                                                                  |

```ts
interface Diagnostic {
  line?: number; // one-based; absent when the problem has no position
  column?: number;
  endColumn?: number;
  message: string;
  severity: "error" | "warning";
}
```

```js
import { validate, convert } from "@pngwasi/config-editor";

validate("json", '{"insecure_plaintext": "yes"}', schemaText);
// [{ line: 1, column: 24, endColumn: 29, severity: "error",
//    message: '/insecure_plaintext: "yes" is not of type "boolean"' }]

convert("yaml", "listen: 7835\n", "toml");
// { text: "listen = 7835\n", losses: [] }
```

## Keyboard

`Ctrl+Space` suggestions · `Enter`/`Tab` accept · `Tab`/`Shift+Tab` indent ·
`Ctrl+/` comment · `Ctrl+Z`/`Ctrl+Y` undo/redo · `F8` next problem ·
`Ctrl+Enter`/`Ctrl+Shift+Enter` new line below/above · `Escape` close.
`Cmd` works in place of `Ctrl` on macOS.

## Languages written in JavaScript

```js
import { registerLanguage, ConfigEditor, convert } from "@pngwasi/config-editor";

const env = registerLanguage({
  label: "ENV", // its name is the label lowercased: "env"
  icon: "E", // optional, toolbar icon
  extensions: ["env"], // optional
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
  comments: { lineComment: "#" }, // Ctrl+/ (a list: the first is written)
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
for the built-in languages only. Built-in names cannot be replaced;
registering the same label again replaces it for new editors. Its name is
the label lowercased (`"Énv"` → `"énv"`).

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

The `colors` option does the same per editor.

## License

MIT. See [LICENSE](LICENSE).
