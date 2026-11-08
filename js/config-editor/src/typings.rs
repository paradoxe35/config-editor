//! The TypeScript declarations wasm-bindgen cannot derive: options,
//! callbacks and the shapes the API takes and returns.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** A built-in language name (`"yaml"`, `"json"`, `"toml"`, `"hcl"`, `"xml"`) or one from `registerLanguage`. */
export type LanguageName = string;

/** A bundled theme slug, such as `"vscode-dark"`, `"vscode-light"`, `"nord"`. See `themes()`. */
export type ThemeName = string;

/** What `new ConfigEditor` takes: the document and its callbacks, and every setting of `EditorSettings`. */
export interface EditorOptions extends EditorSettings {
  /** The document, in `language`. */
  value?: string;
  /** Language of the document. Default `"yaml"`. */
  language?: LanguageName;
  /** Edit in another language; `value` and `onChange` stay in `language`. */
  editAs?: LanguageName;
  /** JSON Schema (as JSON text) for completion, hover docs and validation. */
  schema?: string;
  /** Text the diff gutter compares against. Default: the initial `value`. */
  baseline?: string;
  /** File name shown in the toolbar. */
  fileName?: string;
  /** View mode; switches live (`update`). */
  readOnly?: boolean;
  /** Extra class on the editor root. */
  className?: string;
  /** Called with the document, in `language`, after every edit. */
  onChange?: (value: string) => void;
  /** Called with the language on screen when an `editAs` switch completes, and when undo or redo steps back across one (it waits while the document cannot convert yet, the host's language staying on screen). */
  onEditAsChange?: (language: LanguageName) => void;
  /** Called with how the document differs from `baseline`: on mount, then whenever that changes. */
  onChangeSummary?: (summary: ChangeSummary) => void;
  /** Called with the path of what the caret is on each time it changes (and once on mount), for a breadcrumb. */
  onPathChange?: (path: EditorPath) => void;
  /** Problems the host found (a server's validation), shown next to the editor's own; `update` replaces them. */
  hostDiagnostics?: HostDiagnostic[];
  /** Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile. Without it, each is a console warning. */
  onSchemaWarning?: (message: string) => void;
  /** Called when the user presses `Ctrl+S` / `Cmd+S` (the browser's dialog prevented) or you call `save()`, with the document in `language`. Nothing saves by itself. Without it, `Ctrl+S` is the browser's. */
  onSave?: (value: string, language: LanguageName) => void;
  /** Called with whether the document differs from `baseline`: once on mount, then each time that changes. */
  onDirtyChange?: (dirty: boolean) => void;
  /** Called when a new `baseline` arrives under unsaved edits (the editor asks the user to reload or keep their text), and when that is settled. */
  onBaselineConflict?: (event: BaselineConflict) => void;
  /** Fields the user cannot change (each with everything inside it), on top of those the schema marks `readOnly`. They hold while editing as another language too; `update` replaces them. */
  readOnlyPaths?: FieldPath[];
  /** Stretches of the document (in `language`) the user cannot change; they follow the text through edits; `update` replaces them. */
  readOnlyRanges?: TextRangeInput[];
  /** How secret placeholders (`«secret 3f2a»`) are matched, described and completed in fields the schema marks `writeOnly` or `x-secret`. */
  secrets?: SecretOptions;
}

/** Secret placeholders: tokens the host writes in place of real secrets. */
export interface SecretOptions {
  /** A regular expression (Rust `regex` syntax) matching placeholder tokens. Default `«secret [0-9a-f]+»`. Only tokens in the host's own `value` are real. */
  placeholder?: string;
  /** Decides which matched tokens are real placeholders. */
  isPlaceholder?: (token: string) => boolean;
  /** Hover text of a placeholder. Default "Keeps the current secret unless you replace it". */
  hint?: string;
  /** Reference forms offered in a secret field's value. Default `["${", "file:"]`. */
  referencePrefixes?: string[];
}

/** A stretch of the host's document: string indices, or one-based lines and columns. */
export type TextRangeInput = { start: number; end?: number } | { startLine: number; startColumn: number; endLine?: number; endColumn?: number };

/**
 * A baseline conflict's progress: `"opened"` (the notice asks the user),
 * `"reloaded"` (the document is the new baseline, reported through
 * `onChange`), `"kept-mine"` (the document stays, diffed against the new
 * baseline), `"settled"` (the document came to equal the new baseline, or the
 * baseline was set back).
 */
export type BaselineConflict = "opened" | "reloaded" | "kept-mine" | "settled";

/** A problem the host found, by `path` (preferred: it follows the field through edits and `editAs`) or `range`; with neither, it is about the whole document. */
export interface HostDiagnostic {
  path?: FieldPath;
  /** A stretch of the document in `language`: string indices, or one-based lines and columns. Unplaced while editing as another language. */
  range?: TextRangeInput;
  /** Default `"error"`. */
  severity?: Severity;
  message: string;
  /** Shown with the message, e.g. `"E_PORT"`. */
  code?: string;
  /** Who reported it, shown with the message; default `"host"`. */
  source?: string;
  /** A page explaining the problem, opened from its message. */
  docsUrl?: string;
}

export type Severity = "error" | "warning" | "info" | "hint";

/**
 * Settings to change: each key left out (or `undefined`) stays, `null`
 * returns it to its default; `strings` merges text by text (a text set to
 * `null` returns to its default).
 */
export type SettingsUpdate = {
  [K in Exclude<keyof EditorSettings, "strings">]?: EditorSettings[K] | null;
} & {
  strings?: { [K in keyof EditorStrings]?: EditorStrings[K] | null } | null;
};

/**
 * What `editor.update` takes: the constructor's options, any of them. A key
 * left out (or `undefined`) stays as it is; `null` removes a callback,
 * clears a list, turns `editAs` and `readOnly` off, or returns a setting to
 * its default; settings merge key by key. The options the editor is built
 * from (`language`, `schema`, `fileName`, `className`, `secrets`) cannot
 * change: create a new editor.
 */
export interface EditorUpdate extends SettingsUpdate {
  /** Replaces the document (in `language`), as a host update: the caret stays when the text is unchanged. */
  value?: string;
  /** The text the diff gutter compares against. */
  baseline?: string;
  /** Edit in another language; `null` edits in `language` again. */
  editAs?: LanguageName | null;
  /** View mode; `null` is `false`. */
  readOnly?: boolean | null;
  /** Replaces the host's problems; `null` or `[]` clears them. */
  hostDiagnostics?: HostDiagnostic[] | null;
  /** Replaces the fields the user cannot change; `null` or `[]` clears them. */
  readOnlyPaths?: FieldPath[] | null;
  /** Replaces the stretches the user cannot change (in the document as it is after this update); `null` or `[]` clears them. */
  readOnlyRanges?: TextRangeInput[] | null;
  onChange?: ((value: string) => void) | null;
  onEditAsChange?: ((language: LanguageName) => void) | null;
  onChangeSummary?: ((summary: ChangeSummary) => void) | null;
  onPathChange?: ((path: EditorPath) => void) | null;
  onSchemaWarning?: ((message: string) => void) | null;
  /** `null` gives `Ctrl+S` back to the browser. */
  onSave?: ((value: string, language: LanguageName) => void) | null;
  onDirtyChange?: ((dirty: boolean) => void) | null;
  onBaselineConflict?: ((event: BaselineConflict) => void) | null;
  /** The editor is built from it: create a new one to change it. */
  language?: never;
  /** The editor is built from it: create a new one to change it. */
  schema?: never;
  /** The editor is built from it: create a new one to change it. */
  fileName?: never;
  /** The editor is built from it: create a new one to change it. */
  className?: never;
  /** The editor is built from it: create a new one to change it. */
  secrets?: never;
}

/**
 * What a mounted editor answers and does: a `ConfigEditor`, and a
 * document's editor in a `ConfigDocuments` (`view.editor(id)`).
 */
export interface EditorView {
  /** The document, in its language (a document's editor: the host's `value`, then each edit). */
  readonly value: string;
  /** The text on screen: in the `editAs` language while editing as another one (what `rangeOf` and `pathAt` index into). */
  readonly text: string;
  /** Whether the document differs from its baseline. */
  readonly isDirty: boolean;
  /** The path of what the caret is on, or `null` (a document's editor: until it has been shown once). */
  readonly currentPath: EditorPath | null;
  /** The folded blocks, one-based `{ startLine, endLine }` (`startLine` stays visible). */
  readonly foldedLines: LineRange[];
  /** The settings in effect: every key of `EditorSettings`, set. */
  readonly settings: Required<EditorSettings>;
  /** Scrolls to the field at `path`, selects it and focuses the editor; `false` when there is no such field. */
  reveal(path: FieldPath): boolean;
  /** Where the field at `path` is written in the text on screen, or `null`. */
  rangeOf(path: FieldPath): EditorRange | null;
  /** The path of what the text on screen holds at string index `offset`, or `null`. */
  pathAt(offset: number): EditorPath | null;
  /** Folds the block the field at `path` opens; `false` when there is none to fold. */
  fold(path: FieldPath): boolean;
  /** Opens the folds around the field at `path`; `false` when none was closed. */
  unfold(path: FieldPath): boolean;
  /** Folds every block (`Ctrl+K Ctrl+0`). */
  foldAll(): void;
  /** Opens every fold (`Ctrl+K Ctrl+J`). */
  unfoldAll(): void;
  /** Focuses the editor, keeping its selection. */
  focus(): void;
  /** Saves as `Ctrl+S` does (calls `onSave`); `false` when nothing was sent (no `onSave`, or an `editAs` edit that cannot convert back yet). */
  save(): boolean;
  /** The unified diff (`diff -u`) of the baseline against the document, with `context` unchanged lines around each change (default 3); empty when they are the same. */
  unifiedDiff(context?: number): string;
}

/** How the document differs from the baseline, by lines, as the diff gutter shows it. */
export interface ChangeSummary {
  added: number;
  removed: number;
  modified: number;
  /** Separate changes (runs of changed lines); "Revert this change" restores one. */
  hunks: number;
}

/** A field: a JSON pointer (`"/web/hosts/1"`) or its keys and indices (`["web", "hosts", 1]`). The same in every language. */
export type FieldPath = string | (string | number)[];

/** A path the editor reports: as a JSON pointer and as keys and indices. */
export interface EditorPath {
  /** `""` for the whole document. */
  pointer: string;
  segments: (string | number)[];
}

/** A stretch of the text on screen: string indices (`start`, `end`, as `selectionStart` counts) and one-based lines and columns. */
export interface EditorRange {
  start: number;
  end: number;
  startLine: number;
  startColumn: number;
  endLine: number;
  endColumn: number;
}

/** One-based lines `startLine..=endLine`: a fold keeps `startLine` visible and hides the rest. */
export interface LineRange {
  startLine: number;
  endLine: number;
}

/** A language, as `languages()` lists it. */
export interface LanguageInfo {
  /** The canonical name, what `language` and `editAs` take (they also take an extension: `"yml"`). */
  name: LanguageName;
  /** Short UI label, such as `"YAML"`. */
  label: string;
  /** One or two letters for the toolbar icon. */
  icon: string;
  /** Lowercase, without the dot; the first is preferred. */
  extensions: string[];
  /** The name the toolbar shows when `fileName` is not given, such as `"untitled.yaml"`. */
  defaultFileName: string;
}

/** A bundled theme, as `themes()` lists it. */
export interface ThemeInfo {
  /** The slug `theme` takes, such as `"vscode-dark"`. */
  name: ThemeName;
  /** Display name, such as `"VS Code Dark"`. */
  label: string;
  /** Whether it is a dark theme. */
  dark: boolean;
}

export interface Diagnostic {
  line?: number;
  column?: number;
  /** Just past the marker, on `endLine` when it is set. */
  endColumn?: number;
  /** The line the marker ends on, only when it covers several lines. */
  endLine?: number;
  message: string;
  severity: Severity;
  /** `true` on the warning of a key the schema marks `deprecated` (the marker is the key). */
  deprecated?: boolean;
}

export interface Conversion {
  text: string;
  /** JSON pointers of values the target reads back differently. */
  losses: string[];
}

export interface CustomLanguage {
  label: string;
  /** One or two letters for the toolbar icon. */
  icon?: string;
  extensions?: string[];
  /** Parse to a JSON value, or throw `{ message, line?, column? }`. */
  parse(text: string): unknown;
  /** Text for a JSON value (the document model, usually an object). */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  serialize?(value: any): string;
  /** `{ line, startColumn, endColumn }` (one-based) of the key at a JSON pointer. */
  keySpan?(text: string, pointer: string): { line: number; startColumn: number; endColumn: number } | null | undefined | void;
  valueSpan?(text: string, pointer: string): { line: number; startColumn: number; endColumn: number } | null | undefined | void;
  /** Ranges of one line as JavaScript string indices (as `indexOf` gives): kind is "key" | "string" | "value" | "literal" | "comment" | "punctuation". */
  highlight?(line: string): { start: number; end: number; kind: string }[] | null | undefined | void;
  /** Comment markers, as in VS Code's language configuration: `Ctrl+/` writes the first line comment, and unwraps lines each wrapped in `blockComment`. */
  comments?: { lineComment?: string | string[]; blockComment?: [string, string] };
  /** Bracket pairs Enter opens across lines, e.g. `[["{", "}"], ["[", "]"]]`. */
  brackets?: [string, string][];
  /** What separates a key from its value, e.g. `["="]` or `[":"]`: key renames, `Tab` through empty values. Without the callbacks below they also give a flat language (one `KEY=value` entry per line) schema completion and hover. */
  separators?: string[];
  /** Where the caret at string index `offset` is, for schema completion: typing a key in the object at `path` (`at: "key"`), or in the value of the key that ends `path` (`at: "value"`; a path ending in an index: an array item), with `prefix` typed before it. `null`: nothing to suggest. Replaces the flat default of `separators`. */
  contextAt?(text: string, offset: number): CustomLanguageContext | null | undefined | void;
  /** How a suggested key is written, with the caret's string index in `text`; `kind` is what the schema expects as its value. Lines after the first are indented as the caret's line. `null` leaves the key out. */
  insertKey?(key: string, kind: "scalar" | "object" | "array"): { text: string; caret: number } | null | undefined | void;
  /** The key at string index `offset` (under the mouse), for hover docs: its name, its full path (ending with it), and the string index it starts at. `null`: no key there. */
  keyAt?(text: string, offset: number): { key: string; path: (string | number)[]; start: number } | null | undefined | void;
  /** Strings and comments written over several lines (string indices, as `indexOf` gives): their lines are colored as such up to `end`, the rest of the last line by `highlight`; nothing is suggested inside. */
  multilineRanges?(text: string): { start: number; end: number; kind: "string" | "comment" }[] | null | undefined | void;
}

/** Where the caret is in a custom language's document; see `CustomLanguage.contextAt`. */
export interface CustomLanguageContext {
  /** The object holding the caret (`at: "key"`), or the key whose value it is in (`at: "value"`). */
  path: (string | number)[];
  at: "key" | "value";
  /** The key or value typed before the caret, which suggestions filter by and replace. */
  prefix: string;
}
"#;

#[wasm_bindgen(typescript_custom_section)]
const DOCUMENT_TYPES: &str = r#"
/** One open document of a `ConfigDocuments` view. */
export interface DocumentSpec {
  /** Identifies the document across `update({ documents })` calls and in every callback. Unique in the list. */
  id: string;
  /** The tab's label (a file name or path); its extension gives the language when `language` is not set. */
  fileName: string;
  /** The text, in the document's language. A new one updates the editor, keeping the caret. */
  value: string;
  /** Default: from `fileName`'s extension, else `"yaml"`. A new language mounts a new editor. */
  language?: LanguageName;
  /** Text to diff against and to tell unsaved edits by. Default: `value` as the document was added. */
  baseline?: string;
  /** View mode for this document; switches live. */
  readOnly?: boolean;
  /** JSON Schema as JSON text; documents with the same text share one compiled schema. An invalid schema is reported inline. */
  schema?: string;
  /** Edit in another language than `language`; switches live. */
  editAs?: LanguageName;
  /** Settings for this document's editor, over the view's: the keys given replace the view's (which still apply to the rest, and follow its `update`). Switch live. The view's `styled` applies to every editor. */
  settings?: EditorSettings;
  /** Problems the host found in this document (see `HostDiagnostic`); a new list replaces the markers. */
  hostDiagnostics?: HostDiagnostic[];
  /** Fields of this document the user cannot change; update live. */
  readOnlyPaths?: FieldPath[];
  /** Stretches of this document (in its language) the user cannot change; update live. */
  readOnlyRanges?: TextRangeInput[];
  /** Secret placeholders, as `ConfigEditor`'s `secrets`. Read when the document's editor mounts (the first time it is shown). */
  secrets?: SecretOptions;
}

/** What `onClose` receives: the user asked to close a document. Nothing closes by itself: remove it from the list (asking first when `dirty`). */
export interface CloseRequest {
  id: string;
  /** Whether it has unsaved edits (its text differs from its baseline). */
  dirty: boolean;
}

/** What `new ConfigDocuments` takes: the documents and callbacks, and the settings (`EditorSettings`) of every editor and of the tabs (`theme`, `colors` with the `tab-*` tokens; `styled` injects the stylesheet once for the view). A document's own `settings` override them. */
export interface DocumentsOptions extends EditorSettings {
  /** The open documents, in tab order. */
  documents: DocumentSpec[];
  /** The document shown first. Default: the first one. */
  activeId?: string;
  /** Extra class on the view's root. */
  className?: string;
  /** The tab list's accessible name. Default `"Open files"`. */
  label?: string;
  /** Called with a document's id when it becomes the active one (a tab, `Alt+PageDown` / `Alt+PageUp`, `select()`, or the active one closing). */
  onSelect?: (id: string) => void;
  /** Called when the user asks to close a document (close button, middle click, `Delete` on its tab). Without it, tabs have no close button. */
  onClose?: (request: CloseRequest) => void;
  /** Called with a document's id and text after every edit: store it as the document's `value`. */
  onChange?: (id: string, value: string) => void;
  /** Called when the user saves a document (`Ctrl+S` / `Cmd+S`) or you call `save(id)` / `saveAll()`. */
  onSave?: (id: string, value: string, language: LanguageName) => void;
  /** Called with a document's id and whether it differs from its baseline: when it is added, then each time that changes. */
  onDirtyChange?: (id: string, dirty: boolean) => void;
}

/**
 * What `view.update` takes: the documents, the active one, the settings and
 * the callbacks, any of them. A key left out (or `undefined`) stays as it
 * is; `null` removes a callback or returns a setting to its default;
 * settings merge key by key. `className` and `label` cannot change.
 */
export interface DocumentsUpdate extends SettingsUpdate {
  /** Replaces the list: a document keeps its editor while its `id` stays listed. */
  documents?: DocumentSpec[];
  /** Shows this document, leaving the focus where it is. */
  activeId?: string;
  onSelect?: ((id: string) => void) | null;
  /** Replaces or removes it; it cannot be added to a view mounted without one (its tabs have no close buttons). */
  onClose?: ((request: CloseRequest) => void) | null;
  onChange?: ((id: string, value: string) => void) | null;
  /** `null` gives `Ctrl+S` back to the browser. */
  onSave?: ((id: string, value: string, language: LanguageName) => void) | null;
  onDirtyChange?: ((id: string, dirty: boolean) => void) | null;
  /** The view is built from it: create a new one to change it. */
  className?: never;
  /** The view is built from it: create a new one to change it. */
  label?: never;
}
"#;

/// `EditorSettings`, generated from the crate's struct (see
/// `tests/settings.rs`).
#[wasm_bindgen(typescript_custom_section)]
const SETTINGS_TYPES: &str = include_str!("settings.d.ts");

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "EditorOptions")]
    pub type EditorOptions;
    #[wasm_bindgen(typescript_type = "Required<EditorSettings>")]
    pub type SettingsResult;
    #[wasm_bindgen(typescript_type = "EditorView | null")]
    pub type DocumentEditorResult;
    #[wasm_bindgen(typescript_type = "EditorUpdate")]
    pub type EditorUpdate;
    #[wasm_bindgen(typescript_type = "DocumentsOptions")]
    pub type DocumentsOptions;
    #[wasm_bindgen(typescript_type = "DocumentsUpdate")]
    pub type DocumentsUpdate;
    #[wasm_bindgen(typescript_type = "CustomLanguage")]
    pub type CustomLanguage;
    #[wasm_bindgen(typescript_type = "ThemeColorMap")]
    pub type ColorMap;
    #[wasm_bindgen(typescript_type = "Diagnostic[]")]
    pub type Diagnostics;
    #[wasm_bindgen(typescript_type = "Conversion")]
    pub type ConversionResult;
    #[wasm_bindgen(typescript_type = "string[]")]
    pub type Names;
    #[wasm_bindgen(typescript_type = "EditorRange | null")]
    pub type RangeResult;
    #[wasm_bindgen(typescript_type = "EditorPath | null")]
    pub type PathResult;
    #[wasm_bindgen(typescript_type = "ChangeSummary")]
    pub type ChangeSummaryResult;
    #[wasm_bindgen(typescript_type = "LineRange[]")]
    pub type LineRanges;
    #[wasm_bindgen(typescript_type = "unknown")]
    pub type Data;
    #[wasm_bindgen(typescript_type = "LanguageInfo[]")]
    pub type LanguageInfos;
    #[wasm_bindgen(typescript_type = "LanguageName | null")]
    pub type NameResult;
    #[wasm_bindgen(typescript_type = "ThemeInfo[]")]
    pub type ThemeInfos;
}
