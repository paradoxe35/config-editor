// The README examples, type-checked against the built packages (`npm run typecheck`).
import { ConfigEditor, loadConfigEditor, registerLanguage, type ChangeSummary, type CustomLanguageContext, type EditorUpdate, type EditorView } from "@pngwasi/config-editor";
import { convert, unifiedDiff, validate } from "@pngwasi/config-editor/tools";
import { DocumentsBinding, EditorBinding, SETTING_TYPES } from "@pngwasi/config-editor/binding";
const noChanges: ChangeSummary = { added: 0, removed: 0, modified: 0, hunks: 0 };
await loadConfigEditor();
declare const schemaText: string; declare const element: HTMLElement; declare function save(t: string): void;
const editor = new ConfigEditor(document.getElementById("config")!, { language: "yaml", value: "listen: 7835\n", showToolbar: true, onChange: (text) => save(text) });
validate("json", '{"insecure_plaintext": "yes"}', schemaText);
convert("yaml", "listen: 7835\n", "toml");
validate("yaml", "a: 1\n");
for (const problem of validate("yaml", "a: 1\n", schemaText)) {
  const struck: boolean = problem.deprecated === true;
  const lastLine: number | undefined = problem.endLine ?? problem.line;
  void struck; void lastLine;
}
// update: partial, merging; null removes or resets.
editor.update({ readOnly: true, theme: "nord" });
declare const loaded: string;
editor.update({ value: loaded, baseline: loaded });
editor.update({ editAs: null, readOnly: null, theme: null, hostDiagnostics: null, readOnlyPaths: null, readOnlyRanges: null });
editor.update({ onChange: null, onChangeSummary: ({ hunks }) => hunks > 0 });
editor.update({ strings: { find: "Suchen" } }); editor.update({ strings: { find: null } }); editor.update({ strings: null });
// @ts-expect-error: the editor is built from its language.
editor.update({ language: "json" });
const partial: EditorUpdate = { showFooter: true, theme: undefined };
editor.update(partial);
const asView: EditorView = editor;
const review: string = editor.unifiedDiff() + editor.unifiedDiff(0) + unifiedDiff("a\n", "b\n") + asView.unifiedDiff();
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
// Schema completion and hover for a nested custom language.
const caretPlace: CustomLanguageContext = { path: ["server", 0], at: "value", prefix: "fa" };
registerLanguage({
  label: "HJSON",
  parse: (text) => JSON.parse(text),
  separators: [":"],
  writtenSeparator: ": ",
  mapBrackets: ["{", "}"],
  listBrackets: ["[", "]"],
  itemSeparator: ",",
  itemMarker: "-",
  literalQuotes: ["'"],
  contextAt: (text, offset) => (offset > 0 ? caretPlace : null),
  insertKey: (key, kind) => (kind === "object" ? { text: `${key}: {\n  \n}`, caret: key.length + 6 } : { text: `${key}: `, caret: key.length + 2 }),
  keyAt: (text, offset) => ({ key: "host", path: ["server", "host"], start: offset }),
  multilineRanges: (text) => [{ start: 0, end: text.length, kind: "comment" }],
});
// @ts-expect-error: `at` is "key" or "value".
registerLanguage({ label: "Bad", parse: () => ({}), contextAt: () => ({ path: [], at: "inside", prefix: "" }) });
// @ts-expect-error: insertKey returns { text, caret }.
registerLanguage({ label: "Bad", parse: () => ({}), insertKey: (key: string) => key });
// @ts-expect-error: a range's kind is "string" or "comment".
registerLanguage({ label: "Bad", parse: () => ({}), multilineRanges: () => [{ start: 0, end: 1, kind: "key" }] });
convert("env", "listen=7835\n", "yaml").text;
await loadConfigEditor({ module_or_path: "/assets/config_editor_bg.wasm" });
// Host diagnostics and read-only fields.
editor.update({
  hostDiagnostics: [
    { path: "/web/listen", severity: "error", message: "port 80 is taken", code: "E_PORT", source: "validator", docsUrl: "https://example.com/docs/ports" },
    { range: { startLine: 4, startColumn: 1, endColumn: 19 }, severity: "hint", message: "consider TLS" },
    { severity: "warning", message: "restart needed to apply" },
  ],
});
editor.update({ readOnlyPaths: ["/limits", "/tls/cert"] });
// Saving and unsaved edits.
declare function putConfig(text: string): Promise<void>; declare const saveButton: HTMLButtonElement;
const saving = new ConfigEditor(element, {
  value: "a: 1\n", baseline: "a: 1\n",
  onSave: async (text, language) => { await putConfig(text); saving.update({ baseline: text }); void language; },
  onDirtyChange: (dirty) => (saveButton.disabled = !dirty),
  onBaselineConflict: (event: "opened" | "reloaded" | "kept-mine" | "settled") => console.log(event),
});
const sent: boolean = saving.save(); const unsaved: boolean = saving.isDirty;
saving.update({ onSave: null, onDirtyChange: null, onBaselineConflict: null });
new ConfigEditor(element, { folding: false });
editor.foldAll(); editor.unfoldAll();
const folded: boolean = editor.fold(["clients"]) && editor.unfold("/web/listen");
// Parity with the Rust API: feature switches, the instance's text and folds, the catalogs and tools.
import { languageForFile, languages, themes, type EditorSettings, type LanguageInfo, type ThemeColorMap, type ThemeInfo } from "@pngwasi/config-editor";
import { changeSummary, parse, pathAt, presetCss, rangeOf, rangesOf, schemaWarnings, serialize, type EditorPath, type EditorRange, type LineRange } from "@pngwasi/config-editor/tools";
declare const text: string;
const viewer = new ConfigEditor(element, {
  value: text, readOnly: true, styled: false, saveShortcut: false,
  highlighting: false, showDiff: false, completions: false, autoTriggerCompletions: false, hoverDocs: false, showDiagnostics: false, autoClosing: false,
});
viewer.update({ showDiagnostics: true, saveShortcut: true });
const everyKey = Object.keys(SETTING_TYPES) as (keyof EditorSettings)[]; const full: Required<EditorSettings> = viewer.settings; void [everyKey, full];
const onScreen: string = viewer.text; const folds: LineRange[] = viewer.foldedLines;
const counted: number = changeSummary("a\n", "b\n").hunks;
const data: unknown = parse("toml", "[web]\nhttp = 80\n");
const hcl: string = serialize("hcl", { web: { http: 80 } });
const where: EditorRange | null = rangeOf("yaml", "web:\n  http: 80\n", "/web/http");
const wheres: (EditorRange | null)[] = rangesOf("yaml", "web:\n  http: 80\n", ["/web/http", ["web"]]);
const at: EditorPath | null = pathAt("yaml", "web:\n  http: 80\n", 12);
const yaml: LanguageInfo | undefined = languages().find((language) => language.extensions.includes("yml")); const fileLanguage: string | null = languageForFile("deploy/main.tf");
const nord: ThemeInfo | undefined = themes().find((theme) => theme.name === "nord"); const css: string = presetCss();
const dark: boolean = themes()[0].dark; const tokens: ThemeColorMap = { bg: "#101418", key: "#7dd3fc" };
// @ts-expect-error: a colour token that does not exist
const typo: ThemeColorMap = { backgrund: "#000" };
const warnings: string[] = schemaWarnings(schemaText);
void [onScreen, folds, counted, data, hcl, where, wheres, at, typo, yaml, fileLanguage, nord, css, dark, tokens, warnings];
// Several documents.
import { ConfigDocuments, type DocumentSpec } from "@pngwasi/config-editor";
declare function putFile(id: string, text: string): Promise<void>; declare const mainText: string;
let files: DocumentSpec[] = [
  { id: "main", fileName: "config.yaml", value: mainText, schema: schemaText, readOnly: false },
  { id: "tls", fileName: "conf.d/tls.toml", value: "", settings: { folding: false }, readOnlyPaths: ["/cert"], hostDiagnostics: [{ path: "/cert", message: "expired" }] },
];
const update = (id: string, change: Partial<DocumentSpec>) => {
  files = files.map((file) => (file.id === id ? { ...file, ...change } : file));
  view.update({ documents: files });
};
const view = new ConfigDocuments(element, {
  documents: files,
  onChange: (id, value) => update(id, { value }),
  onSave: async (id, value, language) => { await putFile(id, value); update(id, { baseline: value }); void language; },
  onClose: ({ id, dirty }) => { if (dirty) return; files = files.filter((file) => file.id !== id); view.update({ documents: files }); },
  onSelect: (id) => console.log(id),
  showFooter: true,
  theme: "nord",
});
const tls: EditorView | null = view.editor("tls");
const revealed: boolean | undefined = tls?.reveal("/cert"); const tlsRange = tls?.rangeOf("/cert"); const rangeShown: boolean | undefined = tlsRange ? tls?.revealRange(tlsRange) : false; void rangeShown; tls?.fold("/ciphers"); const tlsPath = tls?.currentPath; void [revealed, tlsPath];
const tlsText: string | null | undefined = tls?.value; const tlsDiff: string | undefined = tls?.unifiedDiff(0); const tlsTheme = tls?.settings.theme; void [tlsText, tlsDiff, tlsTheme];
const savedIds: string[] = view.saveAll(); const shown: boolean = view.select("limits"); const active: string | undefined = view.activeId; const dirtyMain: boolean = view.isDirty("main");
view.update({ documents: files, showFooter: true }); view.update({ activeId: "limits" });
view.update({ onSave: null }); view.update({ colors: { "tab-dirty": "#ffcc66" } });
// @ts-expect-error: the view is built with its label.
view.update({ label: "Files" });
view.destroy();
// Binding to a framework.
declare const emit: (text: string) => void; declare const bindingTheme: string;
const binding = new EditorBinding("my-framework-config-editor");
binding.attach(element, { value: "a: 1\n", onChange: (text) => emit(text), language: "yaml" });
binding.update({ value: "a: 1\n", onChange: (text) => emit(text), language: "yaml", theme: bindingTheme });
const found: boolean = binding.handle.reveal("/web/listen"); const foundRange: boolean = binding.handle.revealRange({ start: 0, end: 4 }); void foundRange; const bound: string = binding.handle.value;
binding.detach();
new DocumentsBinding("tabs", { documents: [] }).handle.saveAll();
const kind: "boolean" | "string" | "object" = SETTING_TYPES.showFooter;
void [found, bound, kind, noChanges, review, sent, unsaved, folded, savedIds, shown, active, dirtyMain];
