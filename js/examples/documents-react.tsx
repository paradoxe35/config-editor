import { StrictMode, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  ConfigDocuments,
  type ConfigDocumentsHandle,
  type DocumentSpec,
  type EditorSettings,
  loadConfigEditor,
} from "@pngwasi/config-editor-react";

// The bundle does not sit next to the package's wasm, so say where it is.
await loadConfigEditor({ module_or_path: "../config-editor/pkg/config_editor_bg.wasm" });
const schema = await (await fetch("../../schema.json")).text();

const initialFiles: DocumentSpec[] = [
  { id: "main", fileName: "burrow.yaml", value: "listen: 7835\npublic_address: tunnel.example.com\n", schema },
  { id: "limits", fileName: "conf.d/limits.toml", value: "[limits]\nmax_pending = 512\n", schema },
  { id: "clients", fileName: "conf.d/clients.json", value: '{\n  "clients": {}\n}\n', schema, readOnly: true },
];

function App() {
  const [files, setFiles] = useState(initialFiles);
  const [log, setLog] = useState<string[]>([]);
  const tabs = useRef<ConfigDocumentsHandle>(null);
  // For tests: the view's settings (`setViewSettings`), a document's own
  // fields (`updateFile`) and the ref (`documentsHandle`).
  const [settings, setSettings] = useState<EditorSettings>({});
  const hooks = window as unknown as {
    setViewSettings: typeof setSettings;
    updateFile: (id: string, change: Partial<DocumentSpec>) => void;
    documentsHandle: typeof tabs;
  };
  hooks.setViewSettings = setSettings;
  hooks.documentsHandle = tabs;
  const note = (line: string) => setLog((lines) => [...lines, line]);
  const update = (id: string, change: Partial<DocumentSpec>) =>
    setFiles((list) => list.map((file) => (file.id === id ? { ...file, ...change } : file)));
  hooks.updateFile = update;

  return (
    <>
      <header>
        <strong>ConfigDocuments (React)</strong>
        <button id="save-all" type="button" onClick={() => tabs.current?.saveAll()}>
          Save all
        </button>
      </header>
      <ConfigDocuments
        ref={tabs}
        {...settings}
        documents={files}
        onChange={(id, value) => {
          note(`change ${id}`);
          update(id, { value });
        }}
        onSave={(id, value, language) => {
          note(`save ${id} ${language}`);
          update(id, { baseline: value });
        }}
        onSelect={(id) => note(`select ${id}`)}
        onClose={({ id, dirty }) => {
          if (dirty && !confirm(`Discard unsaved changes to ${id}?`)) return;
          setFiles((list) => list.filter((file) => file.id !== id));
        }}
      />
      <pre id="log">{log.join("\n")}</pre>
    </>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
document.body.dataset.ready = "true";
