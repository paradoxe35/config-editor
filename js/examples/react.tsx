import { StrictMode, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import * as core from "@pngwasi/config-editor";
import * as tools from "@pngwasi/config-editor/tools";
import { loadConfigEditor, themes } from "@pngwasi/config-editor";
import * as api from "@pngwasi/config-editor-react";
import {
  ConfigEditor,
  type ConfigEditorHandle,
  type ConfigEditorProps,
  type HostDiagnostic,
} from "@pngwasi/config-editor-react";

// The bundle does not sit next to the package's wasm, so say where it is.
// `?late-load` loads it only after the component's own load (from its
// default URL, which fails here) has failed: the editor still mounts.
const lateLoad = new URLSearchParams(location.search).has("late-load");
// `?double-load` passes a wrong URL, then the right one, while the first
// still loads: the editor mounts from the second.
const doubleLoad = new URLSearchParams(location.search).has("double-load");
const wasm = { module_or_path: "../config-editor/pkg/config_editor_bg.wasm" };
if (doubleLoad) void loadConfigEditor({ module_or_path: "../config-editor/pkg/missing.wasm" }).catch(() => {});
const ready = lateLoad
  ? new Promise((resolve) => setTimeout(resolve, 800)).then(() => loadConfigEditor(wasm))
  : loadConfigEditor(wasm);

// `?uncontrolled` renders the editor with `defaultValue` instead of `value`.
const uncontrolled = new URLSearchParams(location.search).has("uncontrolled");
// `?late` applies onChange values 400 ms late and only the latest, as a
// debounced parent (a store, a network round trip) would.
const late = new URLSearchParams(location.search).has("late");
// `?slow` applies every onChange value 1.5 s late, none skipped: a slow store
// round trip with many values in flight.
const slow = new URLSearchParams(location.search).has("slow");
// `?random` applies every onChange value 0-200 ms late, none skipped, in
// whatever order the delays give (a fetch per edit).
const random = new URLSearchParams(location.search).has("random");
// `?reset-early` calls `ref.setValue` from an effect, before the editor has
// mounted (it mounts once the wasm is ready).
const resetEarly = new URLSearchParams(location.search).has("reset-early");
// For tests: `?props={...}` adds or overrides props, and `setExtraProps`
// changes them later; `packageApi` is the package's exports, `coreApi`
// and `toolsApi` those of `@pngwasi/config-editor` and its `/tools`.
const initialExtra = JSON.parse(new URLSearchParams(location.search).get("props") ?? "{}") as ConfigEditorProps;
Object.assign(window, { packageApi: api, coreApi: core, toolsApi: tools });
const initialText = "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n";

function App({ schema }: { schema: string }) {
  const [value, setValue] = useState(initialText);
  // The version the host last saved; saves and other clients move it.
  const [baseline, setBaseline] = useState(initialText);
  const [dirty, setDirty] = useState(false);
  const [saves, setSaves] = useState<string[]>([]);
  const [conflicts, setConflicts] = useState<string[]>([]);
  const lateTimer = useRef<ReturnType<typeof setTimeout>>();
  // The value before the latest edit, for the parent-side revert button.
  const previous = useRef(value);
  const [fileName, setFileName] = useState("tunnel.yaml");
  const [editAs, setEditAs] = useState<string | null>(null);
  const [theme, setTheme] = useState("vscode-dark");
  const [readOnly, setReadOnly] = useState(false);
  const [editing, setEditing] = useState("yaml");
  const [changes, setChanges] = useState("");
  const [path, setPath] = useState("");
  const [hostProblems, setHostProblems] = useState<HostDiagnostic[]>([]);
  const [lockAddress, setLockAddress] = useState(false);
  const [lockFirstLine, setLockFirstLine] = useState(false);
  const [themeNames, setThemeNames] = useState<string[]>([]);
  const handle = useRef<ConfigEditorHandle>(null);
  const [extra, setExtra] = useState(initialExtra);
  (window as unknown as { setExtraProps: typeof setExtra }).setExtraProps = setExtra;
  // For tests: the ref, as an app would read it.
  (window as unknown as { editorHandle: typeof handle }).editorHandle = handle;
  // For tests: a re-mount prop changed without moving focus.
  (window as unknown as { renameFile: () => void }).renameFile = () => setFileName((name) => `renamed-${name}`);
  // For tests: the parent's own state setter.
  (window as unknown as { setParentValue: typeof setValue }).setParentValue = setValue;
  // For tests: unmounting and mounting the component again.
  const [shown, setShown] = useState(true);
  (window as unknown as { setShown: typeof setShown }).setShown = setShown;
  useEffect(() => {
    void ready.then(() => setThemeNames(themes().map((theme) => theme.name)));
  }, []);
  useEffect(() => {
    if (resetEarly) handle.current?.setValue("from-effect: 1\n");
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
        <button id="reveal" type="button" onClick={() => handle.current?.reveal(["public_address"])}>
          Go to public_address
        </button>
        <button
          id="host-problems"
          type="button"
          onClick={() =>
            setHostProblems((shown) =>
              shown.length ? [] : [{ path: "/listen", severity: "warning", message: "port in use", source: "server" }],
            )
          }
        >
          Host problems
        </button>
        <label>
          <input id="lock-address" type="checkbox" checked={lockAddress} onChange={(event) => setLockAddress(event.target.checked)} />{" "}
          Lock public_address
        </label>
        <label>
          <input id="lock-first-line" type="checkbox" checked={lockFirstLine} onChange={(event) => setLockFirstLine(event.target.checked)} />{" "}
          Lock line 1
        </label>
        <button id="load-secret" type="button" onClick={() => setValue("clients:\n  acme:\n    secret: «secret 3f2a»\n")}>
          Load a secret
        </button>
        <button id="external-change" type="button" onClick={() => setBaseline((text) => text.replace("7835", "7836"))}>
          External change
        </button>
        <button id="save" type="button" onClick={() => handle.current?.save()}>
          Save
        </button>
        <span id="dirty">{dirty ? "unsaved" : "saved"}</span>
        <span id="saves" data-count={saves.length}>{saves.at(-1) ?? ""}</span>
        <span id="conflicts">{conflicts.join(",")}</span>
        <span id="editing">{editing}</span>
        <span id="changes">{changes}</span>
        <code id="breadcrumb">{path}</code>
      </header>
      {shown && (
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
          if (random) return void setTimeout(apply, Math.random() * 200);
          if (!late) return apply();
          clearTimeout(lateTimer.current);
          lateTimer.current = setTimeout(apply, 400);
        }}
        editAs={editAs}
        onEditAsChange={setEditing}
        onChangeSummary={(summary) =>
          setChanges(`${summary.hunks} changes: +${summary.added} ~${summary.modified} -${summary.removed}`)
        }
        onPathChange={(next) => setPath(next.pointer || "/")}
        hostDiagnostics={hostProblems}
        secrets={{ hint: "Keeps the stored secret" }}
        readOnlyPaths={lockAddress ? ["/public_address"] : []}
        readOnlyRanges={lockFirstLine ? [{ startLine: 1, startColumn: 1, endLine: 2, endColumn: 1 }] : []}
        baseline={baseline}
        onSave={(text) => {
          setSaves((all) => [...all, text]);
          setBaseline(text);
        }}
        onDirtyChange={setDirty}
        onBaselineConflict={(event) => setConflicts((all) => [...all, event])}
        theme={theme}
        readOnly={readOnly}
        colors={{ key: "#ffcc66" }}
        fileName={fileName}
        showToolbar
        showFooter
        {...extra}
      />
      )}
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
