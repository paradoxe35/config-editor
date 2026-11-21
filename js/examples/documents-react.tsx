import { StrictMode, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  ConfigDocuments,
  type ConfigDocumentsHandle,
  type DocumentSpec,
  type EditorSettings,
} from "@pngwasi/config-editor-react";
import { loadConfigEditor } from "@pngwasi/config-editor";
import { later } from "./echo-delay";

// The bundle does not sit next to the package's wasm, so say where it is.
await loadConfigEditor({ module_or_path: "../config-editor/pkg/config_editor_bg.wasm" });
const schema = await (await fetch("../../schema.json")).text();


const initialFiles: DocumentSpec[] = [
  { id: "main", fileName: "tunnel.yaml", value: "listen: 7835\npublic_address: tunnel.example.com\n", schema, closable: false },
  { id: "limits", fileName: "conf.d/limits.toml", value: "[limits]\nmax_pending = 512\n", schema },
  { id: "clients", fileName: "conf.d/clients.json", value: '{\n  "clients": {}\n}\n', schema, readOnly: true },
];

// `?revisions`: each file keeps the revision `onChange` gave its value
// (`valueRevision`), as the README shows: the newest revision wins (a late,
// older one is ignored).
const revisions = new URLSearchParams(location.search).has("revisions");

function App() {
  const [files, setFilesState] = useState(initialFiles);
  // The list as last set, rendered or not: what the next render lists, so a
  // hook reading it works its ranges out on the text it lists them with.
  const latest = useRef(initialFiles);
  const setFiles = (change: (list: DocumentSpec[]) => DocumentSpec[]) => {
    latest.current = change(latest.current);
    setFilesState(latest.current);
  };
  const [log, setLog] = useState<string[]>([]);
  const tabs = useRef<ConfigDocumentsHandle>(null);
  // For tests: the view's settings (`setViewSettings`), a document's own
  // fields (`updateFile`), the ref (`documentsHandle`) and the list
  // (`currentFiles`).
  const [settings, setSettings] = useState<EditorSettings>({});
  const hooks = window as unknown as {
    setViewSettings: typeof setSettings;
    updateFile: (id: string, change: Partial<DocumentSpec>) => void;
    documentsHandle: typeof tabs;
    currentFiles: () => DocumentSpec[];
  };
  hooks.setViewSettings = setSettings;
  hooks.currentFiles = () => latest.current;
  hooks.documentsHandle = tabs;
  const note = (line: string) => setLog((lines) => [...lines, line]);
  const update = (id: string, change: Partial<DocumentSpec>) =>
    setFiles((list) => list.map((file) => (file.id === id ? { ...file, ...change } : file)));
  hooks.updateFile = update;
  // "+": the host makes the new file; the view selects it.
  const added = useRef(0);
  const add = () => {
    added.current += 1;
    const count = added.current;
    note(`add new${count}`);
    setFiles((list) => [...list, { id: `new${count}`, fileName: `conf.d/new-${count}.yaml`, value: "", schema }]);
  };

  return (
    <>
      <header>
        <h1>ConfigDocuments (React)</h1>
        <button id="save-all" type="button" onClick={() => tabs.current?.saveAll()}>
          Save all
        </button>
      </header>
      <ConfigDocuments
        ref={tabs}
        {...settings}
        documents={files}
        onChange={(id, value, revision) => {
          note(`change ${id}`);
          later(() => {
            if (!revisions) return update(id, { value });
            setFiles((list) =>
              list.map((file) =>
                file.id !== id || (file.valueRevision !== undefined && revision < file.valueRevision)
                  ? file
                  : { ...file, value, valueRevision: revision },
              ),
            );
          });
        }}
        onSave={(id, value, language) => {
          note(`save ${id} ${language}`);
          update(id, { baseline: value });
        }}
        onSelect={(id) => note(`select ${id}`)}
        onEditAsChange={(id, language) => note(`language ${id} ${language}`)}
        onSyncChange={(id, inSync) => note(`sync ${id} ${inSync}`)}
        onAdd={add}
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
