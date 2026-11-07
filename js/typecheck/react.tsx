// The React README examples, type-checked against the built packages.
import { useState, useRef } from "react";
import { ConfigEditor, type ConfigEditorHandle, type ChangeSummary, loadConfigEditor, validate, convert, unifiedDiff } from "@pngwasi/config-editor-react";
export function Settings({ schema }: { schema: string }) {
  const [text, setText] = useState("listen: 7835\n");
  const ref = useRef<ConfigEditorHandle>(null);
  ref.current?.value; ref.current?.editor; ref.current?.setValue("listen: 1\n"); ref.current?.unifiedDiff(); ref.current?.unifiedDiff(0);
  const [changes, setChanges] = useState<ChangeSummary | null>(null);
  return (<><ConfigEditor language="yaml" schema={schema} value={text} onChange={setText} showToolbar showFooter editAs={null} onChangeSummary={setChanges} />{changes?.hunks}
    <ConfigEditor ref={ref} defaultValue={text} /></>);
}
declare const wasmUrl: string; declare const text: string; declare const schema: string;
loadConfigEditor({ module_or_path: wasmUrl });
await loadConfigEditor();
const problems = validate("toml", text, schema);
convert("yaml", text, "json");
const patch: string = unifiedDiff("a\n", "b\n", 3);
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
