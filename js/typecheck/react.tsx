// The React README examples, type-checked against the built packages.
import { useState, useRef } from "react";
import { ConfigEditor, type ConfigEditorHandle, type ChangeSummary } from "@pngwasi/config-editor-react";
import { loadConfigEditor, themes } from "@pngwasi/config-editor";
import { validate, convert, unifiedDiff } from "@pngwasi/config-editor/tools";
export function Settings({ schema }: { schema: string }) {
  const [text, setText] = useState("listen: 7835\n");
  const ref = useRef<ConfigEditorHandle>(null);
  ref.current?.value; ref.current?.editor; ref.current?.setValue("listen: 1\n"); ref.current?.unifiedDiff(); ref.current?.unifiedDiff(0); ref.current?.foldAll(); ref.current?.unfoldAll(); ref.current?.fold("/clients"); ref.current?.unfold(["web", "listen"]);
  const [changes, setChanges] = useState<ChangeSummary | null>(null);
  return (<><ConfigEditor language="yaml" schema={schema} value={text} onChange={setText} showToolbar showFooter editAs={null} onChangeSummary={setChanges} />{changes?.hunks}
    <ConfigEditor ref={ref} defaultValue={text} folding={false} /></>);
}
declare const wasmUrl: string; declare const text: string; declare const schema: string;
loadConfigEditor({ module_or_path: wasmUrl });
await loadConfigEditor();
const problems = validate("toml", text, schema);
convert("yaml", text, "json");
const patch: string = unifiedDiff("a\n", "b\n", 3);
const names: string[] = themes().map((theme) => theme.label);
// Saving.
declare function putConfig(text: string): Promise<void>;
export function Saving() {
  const [saved, setSaved] = useState("a: 1\n");
  const [text, setText] = useState(saved);
  const [, setDirty] = useState(false);
  const ref = useRef<ConfigEditorHandle>(null);
  const sent: boolean | undefined = ref.current?.save(); const dirty: boolean | undefined = ref.current?.isDirty;
  void sent; void dirty;
  return (<ConfigEditor ref={ref} value={text} onChange={setText} baseline={saved}
    onSave={async (next) => { await putConfig(next); setSaved(next); }} onDirtyChange={setDirty}
    onBaselineConflict={(event) => console.log(event)} />);
}
// Parity with the Rust API.
import { languageForFile, languages, type LanguageInfo, type ThemeInfo } from "@pngwasi/config-editor";
import { changeSummary, parse, serialize, rangeOf, pathAt, presetCss } from "@pngwasi/config-editor/tools";
import type { EditorView, LineRange } from "@pngwasi/config-editor-react";
export function Viewer({ text }: { text: string }) {
  const ref = useRef<ConfigEditorHandle>(null);
  const onScreen: string | undefined = ref.current?.text; const folds: LineRange[] | undefined = ref.current?.foldedLines;
  const info: LanguageInfo | undefined = languages()[0]; const theme: ThemeInfo | undefined = themes().find((one) => one.dark);
  void [onScreen, folds, info, theme, changeSummary("a", "b"), parse("yaml", text), serialize("json", {}), rangeOf("yaml", text, "/a"), pathAt("yaml", text, 0), languageForFile("a.tf"), presetCss()];
  return (<ConfigEditor ref={ref} value={text} readOnly highlighting={false} showDiff={false} completions={false}
    autoTriggerCompletions={false} hoverDocs={false} showDiagnostics={false} autoClosing={false} styled={false} saveShortcut={false} />);
}
// Several documents.
import { ConfigDocuments, type ConfigDocumentsHandle, type DocumentSpec } from "@pngwasi/config-editor-react";
declare function putFile(id: string, text: string): Promise<void>;
export function Files({ schema }: { schema: string }) {
  const [files, setFiles] = useState<DocumentSpec[]>([{ id: "main", fileName: "burrow.yaml", value: "", schema }]);
  const update = (id: string, change: Partial<DocumentSpec>) =>
    setFiles((list) => list.map((file) => (file.id === id ? { ...file, ...change } : file)));
  const tabs = useRef<ConfigDocumentsHandle>(null);
  const sent: string[] | undefined = tabs.current?.saveAll(); tabs.current?.select("tls"); tabs.current?.activeId; void sent;
  tabs.current?.editor("tls")?.reveal("/cert");
  const tlsView: EditorView | null | undefined = tabs.current?.editor("tls"); const tlsValue: string | null | undefined = tlsView?.value; void tlsValue;
  return (
    <ConfigDocuments ref={tabs} documents={files} showFooter folding={false} theme="nord" onChange={(id, value) => update(id, { value })}
      onSave={async (id, value) => { await putFile(id, value); update(id, { baseline: value }); }}
      onClose={({ id, dirty }) => { if (!dirty) setFiles((list) => list.filter((file) => file.id !== id)); }} />
  );
}
