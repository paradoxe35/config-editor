# config-editor

A schema-aware config editor for the browser: YAML, JSON, TOML, HCL and XML
with completion from a JSON Schema, hover docs, validation, a diff gutter,
editing one language as another, and eleven themes. The editor is written in
Rust ([`leptos-config-editor`](../../crates/leptos-config-editor)) and ships
as WebAssembly; this package is its framework-free JavaScript API.
Framework wrappers build on it: [`config-editor-react`](../config-editor-react).

```js
import init, { ConfigEditor } from "config-editor";

await init(); // or init({ module_or_path: "/assets/config_editor_bg.wasm" })

const editor = new ConfigEditor(document.getElementById("config"), {
  language: "yaml",          // yaml | json | toml | hcl | xml | a registered one
  value: "listen: 7835\n",
  schema: schemaJsonText,    // completion, hover docs, validation
  editAs: "json",            // optional: edit in another language
  theme: "vscode-dark",      // see themes()
  colors: { key: "#ffcc66" },// see colorTokens()
  showFooter: true,
  onChange: (text) => save(text),          // always in `language`
  onEditAsChange: (language) => {},        // when a switch completes
});

editor.setValue(text);       // host updates
editor.setEditAs("toml");    // live switch; null edits in `language`
editor.setTheme("nord");
editor.setColors({ bg: "#101418" });
editor.setReadOnly(true);
editor.value;                // current document
editor.destroy();
```

Headless helpers: `validate(language, text, schema?)`,
`convert(from, text, to, schema?)`, `languages()`, `themes()`,
`colorTokens()`.

## Languages written in JavaScript

```js
import { registerLanguage } from "config-editor";

const env = registerLanguage({
  label: "ENV",
  extensions: ["env"],
  parse(text) { /* return a JSON value, or throw { message, line, column } */ },
  serialize(value) { /* optional: return text, for conversion and Edit as */ },
  keySpan(text, pointer) { /* optional: { line, startColumn, endColumn } for diagnostics */ },
  highlight(line) { /* optional: [{ start, end, kind: "key" | "string" | … }] */ },
});
new ConfigEditor(element, { language: env, schema });
```

The editor validates, converts to and from every built-in language, and
places schema problems with `keySpan`/`valueSpan`.

## Building

`npm run build` (needs Rust with the `wasm32-unknown-unknown` target and
`wasm-bindgen-cli` of the version in `Cargo.lock`; `wasm-opt` is used when
installed). Output: `pkg/`.
