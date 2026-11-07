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

export interface EditorOptions {
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
  theme?: ThemeName;
  colors?: ThemeColorMap;
  readOnly?: boolean;
  showToolbar?: boolean;
  showFooter?: boolean;
  /** Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`). Default `true`; `false` leaves those keys to the browser. */
  search?: boolean;
  /** Go to line (`Ctrl+G`). Default `true`; `false` leaves the key to the browser. */
  goToLine?: boolean;
  /** Folding by structure: gutter chevrons, `Ctrl+Shift+[` / `Ctrl+Shift+]`, `Ctrl+K Ctrl+0` / `Ctrl+K Ctrl+J` (`Cmd` on macOS). View only. Default `true`. */
  folding?: boolean;
  /** Show line changes against `baseline` in the gutter (and `Alt+F5` / `Alt+F3` between them). Default `true`. */
  showDiff?: boolean;
  /** Show syntax and schema problems inline, in the footer and on hover (`F8` between them). Default `true`. */
  showDiagnostics?: boolean;
  /** Schema completion (`Ctrl+Space`). Default `true`; `false` offers no suggestions. */
  completions?: boolean;
  /** Open suggestions while typing and after accepting a key. Default `true`; `false` opens them only with `Ctrl+Space`. */
  autoTriggerCompletions?: boolean;
  /** Schema docs (description, type, default, examples) when hovering a key. Default `true`. */
  hoverDocs?: boolean;
  /** Syntax colors. Default `true`. */
  highlighting?: boolean;
  /** Auto-close quotes and brackets, wrap selections, delete empty pairs on Backspace. Default `true`. */
  autoClosing?: boolean;
  /** Inject the preset stylesheet. Default `true`; `false` leaves the `config-editor__*` markup to your styles (see `presetCss()`). */
  styled?: boolean;
  /** Whether `Ctrl+S` / `Cmd+S` saves (with `onSave` set). Default `true`; `false` leaves the key to the browser and only `save()` saves. Switches live with `setSaveShortcut`. */
  saveShortcut?: boolean;
  /** Extra class on the editor root. */
  className?: string;
  /** Called with the document, in `language`, after every edit. */
  onChange?: (value: string) => void;
  /** Called when an `editAs` switch completes (it waits while the document cannot convert yet). */
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
}

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
  range?: { start: number; end?: number } | { startLine: number; startColumn: number; endLine?: number; endColumn?: number };
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

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "EditorOptions")]
    pub type EditorOptions;
    #[wasm_bindgen(typescript_type = "CallbackUpdate")]
    pub type EditorUpdate;
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
