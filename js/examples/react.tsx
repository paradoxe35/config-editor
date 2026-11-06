import { StrictMode, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  ConfigEditor,
  type ConfigEditorHandle,
  loadConfigEditor,
  themes,
} from "@pngwasi/config-editor-react";

// The bundle does not sit next to the package's wasm, so say where it is.
const ready = loadConfigEditor({ module_or_path: "../config-editor/pkg/config_editor_bg.wasm" });

// `?uncontrolled` renders the editor with `defaultValue` instead of `value`.
const uncontrolled = new URLSearchParams(location.search).has("uncontrolled");
// `?late` applies onChange values 400 ms late and only the latest, as a
// debounced parent (a store, a network round trip) would.
const late = new URLSearchParams(location.search).has("late");
// `?slow` applies every onChange value 1.5 s late, none skipped: a slow store
// round trip with many values in flight.
const slow = new URLSearchParams(location.search).has("slow");
const initialText = "# Burrow server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n";

function App({ schema }: { schema: string }) {
  const [value, setValue] = useState(initialText);
  const lateTimer = useRef<ReturnType<typeof setTimeout>>();
  // The value before the latest edit, for the parent-side revert button.
  const previous = useRef(value);
  const [fileName, setFileName] = useState("burrow.yaml");
  const [editAs, setEditAs] = useState<string | null>(null);
  const [theme, setTheme] = useState("vscode-dark");
  const [readOnly, setReadOnly] = useState(false);
  const [editing, setEditing] = useState("yaml");
  const [themeNames, setThemeNames] = useState<string[]>([]);
  const handle = useRef<ConfigEditorHandle>(null);
  // For tests: the ref, as an app would read it.
  (window as unknown as { editorHandle: typeof handle }).editorHandle = handle;
  // For tests: the parent's own state setter.
  (window as unknown as { setParentValue: typeof setValue }).setParentValue = setValue;
  useEffect(() => {
    void ready.then(() => setThemeNames(themes() as unknown as string[]));
  }, []);
  // A discard that also drops an edit the late parent has not applied yet.
  const discard = (text: string) => {
    clearTimeout(lateTimer.current);
    setValue(text);
    handle.current?.setValue(text);
  };

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
        <button id="revert" type="button" onClick={() => setValue(previous.current)}>
          Revert last edit
        </button>
        <button id="reset" type="button" onClick={() => setValue(initialText)}>
          Reset
        </button>
        <button id="force-reset" type="button" onClick={() => discard(initialText)}>
          Reset (always)
        </button>
        <button id="bad-theme" type="button" onClick={() => setTheme("not-a-theme")}>
          Invalid theme
        </button>
        <button id="rename" type="button" onClick={() => setFileName((name) => `renamed-${name}`)}>
          Rename (re-mounts)
        </button>
        <span id="editing">{editing}</span>
      </header>
      <ConfigEditor
        ref={handle}
        language="yaml"
        schema={schema}
        {...(uncontrolled ? { defaultValue: initialText } : { value })}
        onChange={(next) => {
          const apply = () =>
            setValue((current) => {
              previous.current = current;
              return next;
            });
          if (slow) return void setTimeout(apply, 1500);
          if (!late) return apply();
          clearTimeout(lateTimer.current);
          lateTimer.current = setTimeout(apply, 400);
        }}
        editAs={editAs}
        onEditAsChange={setEditing}
        theme={theme}
        readOnly={readOnly}
        colors={{ key: "#ffcc66" }}
        fileName={fileName}
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
