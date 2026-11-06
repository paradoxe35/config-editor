import { StrictMode, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  ConfigEditor,
  type ConfigEditorHandle,
  loadConfigEditor,
  themes,
} from "config-editor-react";

// The bundle does not sit next to the package's wasm, so say where it is.
const ready = loadConfigEditor({ module_or_path: "../config-editor/pkg/config_editor_bg.wasm" });

function App({ schema }: { schema: string }) {
  const [value, setValue] = useState(
    "# Burrow server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n",
  );
  const [editAs, setEditAs] = useState<string | null>(null);
  const [theme, setTheme] = useState("vscode-dark");
  const [readOnly, setReadOnly] = useState(false);
  const [editing, setEditing] = useState("yaml");
  const [themeNames, setThemeNames] = useState<string[]>([]);
  const handle = useRef<ConfigEditorHandle>(null);
  useEffect(() => {
    void ready.then(() => setThemeNames(themes() as unknown as string[]));
  }, []);

  return (
    <>
      <header>
        <strong>config-editor-react</strong>
        <label>
          Edit as{" "}
          <select id="edit-as" value={editAs ?? ""} onChange={(event) => setEditAs(event.target.value || null)}>
            <option value="">Same</option>
            <option>json</option>
            <option>toml</option>
            <option>hcl</option>
            <option>xml</option>
          </select>
        </label>
        <label>
          Theme{" "}
          <select id="theme" value={theme} onChange={(event) => setTheme(event.target.value)}>
            {themeNames.map((name) => (
              <option key={name}>{name}</option>
            ))}
          </select>
        </label>
        <label>
          <input id="read-only" type="checkbox" checked={readOnly} onChange={(event) => setReadOnly(event.target.checked)} />{" "}
          Read-only
        </label>
        <button id="replace" type="button" onClick={() => setValue("listen: 9000\n")}>
          Replace value
        </button>
        <span id="editing">{editing}</span>
      </header>
      <ConfigEditor
        ref={handle}
        language="yaml"
        schema={schema}
        value={value}
        onChange={setValue}
        editAs={editAs}
        onEditAsChange={setEditing}
        theme={theme}
        readOnly={readOnly}
        colors={{ key: "#ffcc66" }}
        fileName="burrow.yaml"
        showToolbar
        showFooter
      />
      <pre id="saved">{value}</pre>
    </>
  );
}

const schema = await (await fetch("../../schema.json")).text();
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App schema={schema} />
  </StrictMode>,
);
document.body.dataset.ready = "true";
