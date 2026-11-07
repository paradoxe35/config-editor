//! The TypeScript declarations wasm-bindgen cannot derive: options,
//! callbacks and the shapes the API takes and returns.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** A built-in language name (`"yaml"`, `"json"`, `"toml"`, `"hcl"`, `"xml"`) or one from `registerLanguage`. */
export type LanguageName = string;

/** A bundled theme slug, such as `"vscode-dark"`, `"vscode-light"`, `"nord"`. See `themes()`. */
export type ThemeName = string;

/** Color overrides keyed by token (`bg`, `key`, `string`, `selection`, …; see `colorTokens()`). */
export type ThemeColorMap = Record<string, string>;

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
  /** View mode; switches live with `setReadOnly`. */
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
  /** Problems the host found (a server's validation), shown next to the editor's own. See `setHostDiagnostics`. */
  hostDiagnostics?: HostDiagnostic[];
  /** Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile. Without it, each is a console warning. */
  onSchemaWarning?: (message: string) => void;
  /** Called when the user presses `Ctrl+S` / `Cmd+S` (the browser's dialog prevented) or you call `save()`, with the document in `language`. Nothing saves by itself. Without it, `Ctrl+S` is the browser's. */
  onSave?: (value: string, language: LanguageName) => void;
  /** Called with whether the document differs from `baseline`: once on mount, then each time that changes. */
  onDirtyChange?: (dirty: boolean) => void;
  /** Called when a new `baseline` arrives under unsaved edits (the editor asks the user to reload or keep their text), and when that is settled. */
  onBaselineConflict?: (event: BaselineConflict) => void;
  /** Fields the user cannot change (each with everything inside it), on top of those the schema marks `readOnly`. They hold while editing as another language too. See `setReadOnlyPaths`. */
  readOnlyPaths?: FieldPath[];
  /** Stretches of the document (in `language`) the user cannot change; they follow the text through edits. See `setReadOnlyRanges`. */
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

/** Callbacks to replace: a function replaces, `null` removes, absent keeps. */
export interface CallbackUpdate {
  onChange?: ((value: string) => void) | null;
  onEditAsChange?: ((language: LanguageName) => void) | null;
  onChangeSummary?: ((summary: ChangeSummary) => void) | null;
  onPathChange?: ((path: EditorPath) => void) | null;
  onSave?: ((value: string, language: LanguageName) => void) | null;
  onDirtyChange?: ((dirty: boolean) => void) | null;
  onBaselineConflict?: ((event: BaselineConflict) => void) | null;
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

/** What a language name stands for (`languageInfo`). */
export interface LanguageInfo {
  /** The canonical name, as `languages()` lists it: `"yaml"` for `"yml"`. */
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

/** A bundled theme (`themeInfo`). */
export interface ThemeInfo {
  name: ThemeName;
  /** Display name, such as `"VS Code Dark"`. */
  label: string;
}

export interface Diagnostic {
  line?: number;
  column?: number;
  endColumn?: number;
  message: string;
  severity: Severity;
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
  /** What separates a key from its value, e.g. `["="]` or `[":"]`: key renames, `Tab` through empty values. */
  separators?: string[];
}
"#;

#[wasm_bindgen(typescript_custom_section)]
const DOCUMENT_TYPES: &str = r#"
/** One open document of a `ConfigDocuments` view. */
export interface DocumentSpec {
  /** Identifies the document across `setDocuments` calls and in every callback. Unique in the list. */
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
  /** Settings for this document's editor, over the view's: the keys given replace the view's (which still apply to the rest, and follow its `setSettings`). Switch live. The view's `styled` applies to every editor. */
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

/** Callbacks of a `ConfigDocuments` to replace: a function replaces, `null` removes, absent keeps. */
export interface DocumentsCallbackUpdate {
  onSelect?: ((id: string) => void) | null;
  onClose?: ((request: CloseRequest) => void) | null;
  onChange?: ((id: string, value: string) => void) | null;
  onSave?: ((id: string, value: string, language: LanguageName) => void) | null;
  onDirtyChange?: ((id: string, dirty: boolean) => void) | null;
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
    #[wasm_bindgen(typescript_type = "EditorSettings")]
    pub type Settings;
    #[wasm_bindgen(typescript_type = "Required<EditorSettings>")]
    pub type SettingsResult;
    #[wasm_bindgen(typescript_type = "(keyof EditorSettings)[]")]
    pub type SettingKeys;
    #[wasm_bindgen(typescript_type = "DocumentEditor | null")]
    pub type DocumentEditorResult;
    #[wasm_bindgen(typescript_type = "CallbackUpdate")]
    pub type EditorUpdate;
    #[wasm_bindgen(typescript_type = "DocumentsOptions")]
    pub type DocumentsOptions;
    #[wasm_bindgen(typescript_type = "DocumentSpec[]")]
    pub type DocumentList;
    #[wasm_bindgen(typescript_type = "DocumentsCallbackUpdate")]
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
    #[wasm_bindgen(typescript_type = "LanguageInfo")]
    pub type LanguageInfoResult;
    #[wasm_bindgen(typescript_type = "LanguageName | null")]
    pub type NameResult;
    #[wasm_bindgen(typescript_type = "ThemeInfo")]
    pub type ThemeInfoResult;
}
