// The README examples, type-checked against the built packages (`npm run typecheck`).
import init, { ConfigEditor, validate, convert, registerLanguage } from "@pngwasi/config-editor";
await init();
declare const schemaText: string; declare const element: HTMLElement; declare function save(t: string): void;
const editor = new ConfigEditor(document.getElementById("config")!, { language: "yaml", value: "listen: 7835\n", showToolbar: true, onChange: (text) => save(text) });
validate("json", '{"insecure_plaintext": "yes"}', schemaText);
convert("yaml", "listen: 7835\n", "toml");
validate("yaml", "a: 1\n");
editor.setEditAs(null); editor.setTheme(null); editor.setCallbacks({ onChange: null });
const env = registerLanguage({
  label: "ENV", icon: "E", extensions: ["env"],
  parse(text) { return Object.fromEntries(text.split("\n").filter(Boolean).map((line) => line.split("="))); },
  serialize(value) { return Object.entries(value).map(([key, item]) => `${key}=${item}`).join("\n") + "\n"; },
  keySpan(text, pointer) {},
  valueSpan(text, pointer) { },
  highlight(line) {},
  comments: { lineComment: "#" },
  brackets: [["{", "}"], ["[", "]"]],
  separators: ["="],
});
new ConfigEditor(element, { language: env, schema: schemaText });
convert("env", "listen=7835\n", "yaml").text;
await init({ module_or_path: "/assets/config_editor_bg.wasm" });
