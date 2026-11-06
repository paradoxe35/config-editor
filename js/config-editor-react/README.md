# config-editor-react

React component for [`config-editor`](../config-editor): a schema-aware
YAML, JSON, TOML, HCL and XML editor running as WebAssembly.

```tsx
import { ConfigEditor, loadConfigEditor } from "config-editor-react";

// Optional: start loading early, or point at the .wasm when your bundler
// does not serve it next to config-editor's JavaScript.
loadConfigEditor({ module_or_path: "/assets/config_editor_bg.wasm" });

function Settings({ schema }: { schema: string }) {
  const [text, setText] = useState("listen: 7835\n");
  return (
    <ConfigEditor
      language="yaml"
      schema={schema}
      value={text}
      onChange={setText}
      editAs="json"
      theme="vscode-dark"
      colors={{ key: "#ffcc66" }}
      showFooter
    />
  );
}
```

`value` is controlled (or use `defaultValue`); `editAs`, `theme`, `colors`,
`readOnly` and `baseline` update live. `language`, `schema`, `fileName`,
`showToolbar` and `showFooter` re-mount the editor when they change. A `ref`
gives `{ value, editor }`. The package re-exports `validate`, `convert`,
`registerLanguage`, `languages`, `themes` and `colorTokens`.
