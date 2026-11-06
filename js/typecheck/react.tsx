// The React README examples, type-checked against the built packages.
import { useState, useRef } from "react";
import { ConfigEditor, type ConfigEditorHandle, loadConfigEditor, validate, convert } from "@pngwasi/config-editor-react";
export function Settings({ schema }: { schema: string }) {
  const [text, setText] = useState("listen: 7835\n");
  const ref = useRef<ConfigEditorHandle>(null);
  ref.current?.value; ref.current?.editor; ref.current?.setValue("listen: 1\n");
  return (<><ConfigEditor language="yaml" schema={schema} value={text} onChange={setText} showToolbar showFooter editAs={null} />
    <ConfigEditor ref={ref} defaultValue={text} /></>);
}
declare const wasmUrl: string; declare const text: string; declare const schema: string;
loadConfigEditor({ module_or_path: wasmUrl });
await loadConfigEditor();
const problems = validate("toml", text, schema);
convert("yaml", text, "json");
