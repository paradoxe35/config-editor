// The README examples, type-checked against the built packages (`npm run typecheck`).
import init, { ConfigEditor, validate, convert, registerLanguage, unifiedDiff, type ChangeSummary } from "@pngwasi/config-editor";
const noChanges: ChangeSummary = { added: 0, removed: 0, modified: 0, hunks: 0 };
await init();
declare const schemaText: string; declare const element: HTMLElement; declare function save(t: string): void;
const editor = new ConfigEditor(document.getElementById("config")!, { language: "yaml", value: "listen: 7835\n", showToolbar: true, onChange: (text) => save(text) });
validate("json", '{"insecure_plaintext": "yes"}', schemaText);
convert("yaml", "listen: 7835\n", "toml");
validate("yaml", "a: 1\n");
editor.setEditAs(null); editor.setTheme(null); editor.setCallbacks({ onChange: null, onChangeSummary: ({ hunks }) => hunks > 0 });
const review: string = editor.unifiedDiff() + editor.unifiedDiff(0) + unifiedDiff("a\n", "b\n");
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
// Saving and unsaved edits.
declare function putConfig(text: string): Promise<void>; declare const saveButton: HTMLButtonElement;
const saving = new ConfigEditor(element, {
  value: "a: 1\n", baseline: "a: 1\n",
  onSave: async (text, language) => { await putConfig(text); saving.setBaseline(text); void language; },
  onDirtyChange: (dirty) => (saveButton.disabled = !dirty),
  onBaselineConflict: (event: "opened" | "reloaded" | "kept-mine" | "settled") => console.log(event),
});
const sent: boolean = saving.save(); const unsaved: boolean = saving.isDirty;
saving.setCallbacks({ onSave: null, onDirtyChange: null, onBaselineConflict: null });
new ConfigEditor(element, { folding: false });
editor.foldAll(); editor.unfoldAll();
const folded: boolean = editor.fold(["clients"]) && editor.unfold("/web/listen");
// Parity with the Rust API: feature switches, the handle's text and folds, headless functions.
import { changeSummary, parse, serialize, rangeOf, pathAt, foldRanges, languageInfo, languageForFile, themeInfo, presetCss, type LineRange, type LanguageInfo, type ThemeInfo, type EditorRange, type EditorPath } from "@pngwasi/config-editor";
declare const text: string;
const viewer = new ConfigEditor(element, {
  value: text, readOnly: true, styled: false, saveShortcut: false,
  highlighting: false, showDiff: false, completions: false, autoTriggerCompletions: false, hoverDocs: false, showDiagnostics: false, autoClosing: false,
});
viewer.setSaveShortcut(true);
const onScreen: string = viewer.text; const folds: LineRange[] = viewer.foldedLines;
const counted: number = changeSummary("a\n", "b\n").hunks;
const data: unknown = parse("toml", "[web]\nhttp = 80\n");
const hcl: string = serialize("hcl", { web: { http: 80 } });
const where: EditorRange | null = rangeOf("yaml", "web:\n  http: 80\n", "/web/http");
const at: EditorPath | null = pathAt("yaml", "web:\n  http: 80\n", 12);
const blocks: LineRange[] = foldRanges("json", '{\n  "web": {\n    "http": 80\n  }\n}');
const yaml: LanguageInfo = languageInfo("yml"); const fileLanguage: string | null = languageForFile("deploy/main.tf");
const nord: ThemeInfo = themeInfo("nord"); const css: string = presetCss();
void [onScreen, folds, counted, data, hcl, where, at, blocks, yaml, fileLanguage, nord, css];
