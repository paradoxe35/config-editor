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
  /** The schema for completion, hover docs and validation: JSON Schema as JSON or YAML text, or a schema in `schemaFormat`. One that does not compile throws. */
  schema?: string;
  /** The format `schema` is written in. Default `"json-schema"`; never guessed from the text (`schemaFormatForFile` names the format of a file). What the format cannot express in JSON Schema reaches `onSchemaWarning`. */
  schemaFormat?: SchemaFormatName;
  /** The files `schema` refers to (an include, an import, an external `$ref`), by location: the editor fetches nothing (`schemaImports` lists what to fetch). */
  schemaResources?: Record<string, string>;
  /** Options of `schemaFormat`, such as an XSD's `{ root: "project" }`. */
  schemaOptions?: Record<string, unknown>;
  /** Text the diff gutter compares against. Default: the initial `value`. */
  baseline?: string;
  /** File name shown in the toolbar. */
  fileName?: string;
  /** View mode; switches live (`update`). */
  readOnly?: boolean;
  /** Why the editor is read-only (a file the host manages, a missing permission), while `readOnly` is on: shown with the "Cannot edit in a read-only editor" notice when the user tries to edit, and in the text area's accessible description. Updates live. */
  readOnlyReason?: string;
  /** Extra class on the editor root. */
  className?: string;
  /** Set it when you hand every `onChange` text back through `update({ value })`, possibly late and in any order (a store that re-renders after a delay, a fetch per edit): a `value` that `onChange` gave and that has not come back yet is then its echo and keeps the document, so nothing typed is lost (until 5 s after a later text came back: one skipped before it is then yours). A text it never gave applies; go back to one it gave with `replaceValue`. Without it (the default) every `value` applies. Fixed for the editor's life. */
  echoes?: boolean;
  /** Called with the document, in `language`, after every edit. */
  onChange?: (value: string) => void;
  /** Called with the language on screen: once on mount, then each time an `editAs` switch completes and when undo or redo steps back across one (it waits while the document cannot convert yet, the host's language staying on screen). */
  onEditAsChange?: (language: LanguageName) => void;
  /** Called with whether the text on screen is written back to the document (`value`): once on mount, then each time that changes. `false` while an `editAs` edit cannot convert back (`value` stays the last document, `onChange` waits), from whatever brought it there (typing, paste, undo, a switch, a reload, a new `value`); read after each pause in typing. Always `true` while editing in `language`. */
  onSyncChange?: (inSync: boolean) => void;
  /** Called with how the document differs from `baseline`: on mount, then whenever that changes. */
  onChangeSummary?: (summary: ChangeSummary) => void;
  /** Called with the editor's own problems (syntax, schema, secret placeholders, conversion; `origin: "editor"`, never the host's `hostDiagnostics`), as checked after each pause in typing: once on mount, then each time they change, never while they stay the same. With the `showDiagnostics` setting off it finds none. */
  onProblems?: (problems: Diagnostic[]) => void;
  /** Called with the path of what the caret is on each time it changes (and once on mount), for a breadcrumb. */
  onPathChange?: (path: EditorPath) => void;
  /** Problems the host found (its own checks: a service, a build step), shown next to the editor's own; `update` replaces them. */
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
  /** The text `readOnlyRanges` and `hostDiagnostics` ranges were given for, when it is not `value` (a check of a text the document has moved past): they are checked in it and moved to `value`. Default: `value`. */
  rangesFor?: string;
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
export type TextRangeInput = { start: number; end: number } | { startLine: number; startColumn: number; endLine?: number; endColumn?: number };

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
  /** A stretch of the document in `language`: string indices, or one-based lines and columns. With no `endColumn` (and no `endLine` past `startLine`) it is a point: it marks the whole secret placeholder it is on, else one character there (at a line's end, the place just past it). While editing as another language it marks the field it starts in. */
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
 * returns it to its default; `strings` merges text by text and `colors`
 * token by token (a text or token set to `null` returns to its default).
 */
export type SettingsUpdate = {
  [K in Exclude<keyof EditorSettings, "strings" | "colors">]?: EditorSettings[K] | null;
} & {
  strings?: { [K in keyof EditorStrings]?: EditorStrings[K] | null } | null;
  colors?: { [K in keyof ThemeColorMap]?: ThemeColorMap[K] | null } | null;
};

/**
 * What `editor.update` takes: the constructor's options, any of them. A key
 * left out (or `undefined`) stays as it is; `null` removes a callback,
 * clears a list, turns `editAs` and `readOnly` off, or returns a setting to
 * its default; settings merge key by key. The options the editor is built
 * from (`language`, `schema`, `fileName`, `className`, `secrets`, `echoes`) cannot
 * change: create a new editor.
 */
export interface EditorUpdate extends SettingsUpdate {
  /** Replaces the document (in `language`), as a host update, whatever it is (the text given last included): the caret stays when the text is unchanged. With `echoes`, a text `onChange` gave that has not come back yet is its echo and keeps the document, and so does the text given last, given again (a render with a store behind the typing): reset to it with `replaceValue`. */
  value?: string;
  /** The text this update's `readOnlyRanges` and `hostDiagnostics` ranges were given for, when it is not the document (a check of a text the user has typed past): they are checked in it and moved to the document. Default: `value`, else the document. */
  rangesFor?: string;
  /** The text the diff gutter compares against. */
  baseline?: string;
  /** Edit in another language; `null` edits in `language` again. */
  editAs?: LanguageName | null;
  /** View mode; `null` is `false`. */
  readOnly?: boolean | null;
  /** Why the editor is read-only; `null` says no reason. */
  readOnlyReason?: string | null;
  /** Replaces the host's problems; `null` or `[]` clears them. */
  hostDiagnostics?: HostDiagnostic[] | null;
  /** Replaces the fields the user cannot change; `null` or `[]` clears them. */
  readOnlyPaths?: FieldPath[] | null;
  /** Replaces the stretches the user cannot change (in `rangesFor`, else `value`, else the document); `null` or `[]` clears them. The ones given last, given again with no new text, stay where the editor moved them as the user typed. */
  readOnlyRanges?: TextRangeInput[] | null;
  onChange?: ((value: string) => void) | null;
  onEditAsChange?: ((language: LanguageName) => void) | null;
  onSyncChange?: ((inSync: boolean) => void) | null;
  onChangeSummary?: ((summary: ChangeSummary) => void) | null;
  onProblems?: ((problems: Diagnostic[]) => void) | null;
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
  schemaFormat?: never;
  /** The editor is built from it: create a new one to change it. */
  schemaResources?: never;
  /** The editor is built from it: create a new one to change it. */
  schemaOptions?: never;
  /** The editor is built from it: create a new one to change it. */
  fileName?: never;
  /** The editor is built from it: create a new one to change it. */
  className?: never;
  /** The editor is built from it: create a new one to change it. */
  secrets?: never;
  /** The editor is built from it: create a new one to change it. */
  echoes?: never;
}

/**
 * What a mounted editor answers and does: a `ConfigEditor`, and a
 * document's editor in a `ConfigDocuments` (`view.editor(id)`).
 */
export interface EditorView {
  /** The document, in its language (a document's editor: the host's `value`, then each edit). `null` only from a document's editor whose document was removed (a `ConfigEditor` always answers a string). */
  readonly value: string | null;
  /** The text on screen: in the `editAs` language while editing as another one (what `rangeOf` and `pathAt` index into). `null` only from a document's editor whose document was removed. */
  readonly text: string | null;
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
  /** Selects `range` (string indices into the text on screen, as `rangeOf` gives them), scrolls it into view inside the editor and focuses the editor without moving the page, as `reveal` does for a field: any stretch, comments and blank lines included. A range past the end is clamped. `false` before the editor mounts and for an empty document. */
  revealRange(range: { start: number; end: number }): boolean;
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

/** A stretch of the text on screen (`text`, folded lines included): string indices (`start`, `end`, UTF-16 units) and one-based lines and columns. */
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
  /** Who found it: the editor (syntax, schema, secret placeholders, conversion) or the host (`hostDiagnostics`). */
  origin: "editor" | "host";
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
  /** `{ line, startColumn, endColumn }` (one-based) of the key at a JSON pointer; `null` or `undefined` leaves it to the default (a flat language finds keys on their lines). */
  keySpan?(text: string, pointer: string): { line: number; startColumn: number; endColumn: number } | null | undefined | void;
  valueSpan?(text: string, pointer: string): { line: number; startColumn: number; endColumn: number } | null | undefined | void;
  /** Ranges of one line as JavaScript string indices (as `indexOf` gives): kind is "key" | "string" | "value" | "literal" | "comment" | "punctuation" | "reference" (an anchor, an alias) | "interpolation" (an expression in a string). */
  highlight?(line: string): { start: number; end: number; kind: string }[] | null | undefined | void;
  /** Comment markers, as in VS Code's language configuration: `Ctrl+/` writes the first line comment, and unwraps lines each wrapped in `blockComment`. */
  comments?: { lineComment?: string | string[]; blockComment?: [string, string] };
  /** Bracket pairs Enter opens across lines, e.g. `[["{", "}"], ["[", "]"]]`. */
  brackets?: [string, string][];
  /** What separates a key from its value, e.g. `["="]` or `[":"]`: key renames, `Tab` through empty values. Without the callbacks below they also give a flat language (one `KEY=value` entry per line) schema completion and hover. */
  separators?: string[];
  /** The brackets around a map written on one line, e.g. `["{", "}"]`: an Edit-as save edits such a map where it is, its entries and the line's comment kept. */
  mapBrackets?: [string, string];
  /** The brackets around a list written on one line, e.g. `["[", "]"]`, edited in place as maps are. */
  listBrackets?: [string, string];
  /** What separates the items and entries between those brackets, one character, e.g. `","`. */
  itemSeparator?: string;
  /** What starts a list item written on lines of its own, followed by a space or the line's end, e.g. `"-"`: a save moves such items with their lines and comments. */
  itemMarker?: string;
  /** The quotes (of `"` and `'`) inside which a backslash is text, not an escape, e.g. `["'"]`. */
  literalQuotes?: string[];
  /** One of `separators` with the spacing a new entry writes it with, e.g. `": "` (default: the first separator as it is, `KEY=`). */
  writtenSeparator?: string;
  /** Where the caret at string index `offset` is, for schema completion: typing a key in the object at `path` (`at: "key"`), or in the value of the key that ends `path` (`at: "value"`; a path ending in an index: an array item), with `prefix` typed before it. `null`: nothing to suggest. Replaces the flat default of `separators`. */
  contextAt?(text: string, offset: number): CustomLanguageContext | null | undefined | void;
  /** How a suggested key is written, with the caret's string index in `text`; `kind` is what the schema expects as its value. Lines after the first are indented as the caret's line. `null` leaves the key out. */
  insertKey?(key: string, kind: "scalar" | "object" | "array"): { text: string; caret: number } | null | undefined | void;
  /** The key at string index `offset` (under the mouse), for hover docs: its name, its full path (ending with it), and the string index it starts at. `null`: no key there. */
  keyAt?(text: string, offset: number): { key: string; path: (string | number)[]; start: number } | null | undefined | void;
  /** Strings and comments written over several lines (string indices, as `indexOf` gives): their lines are colored as such up to `end`, the rest of the last line by `highlight`; nothing is suggested inside. */
  multilineRanges?(text: string): { start: number; end: number; kind: "string" | "comment" }[] | null | undefined | void;
  /** `"text"` when every value is text in the language's syntax (`parse` then returns strings): where the schema gives a place a type, the text there is read as it (`"80"` as the number 80 where it wants an integer, `"true"` as the boolean), and a value edited in another view keeps the document's type. Default `"typed"`: `parse` gives each value its type. */
  scalars?: "typed" | "text";
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
  /** Identifies the document across `update({ documents })` calls and in every callback. Not empty, unique in the list. */
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
  /** Why the document is read-only, while `readOnly` is on: its tab's tooltip and accessible description say it, and so does the "Cannot edit in a read-only editor" notice. Updates live. */
  readOnlyReason?: string;
  /** Whether the user may close it (close button, middle click, `Delete` on its tab ask `onClose`). Default `true`; `false` for a document that must stay open: its tab has no close button. Without `onClose` no tab has one. Switches live. */
  closable?: boolean;
  /** The schema, as `ConfigEditor`'s `schema`: JSON Schema as JSON or YAML text, or a schema in `schemaFormat`; documents with the same schema (text, format, resources and options) share one compiled schema. One that does not compile throws, as it does for a `ConfigEditor`. */
  schema?: string;
  /** The format `schema` is written in, as `ConfigEditor`'s `schemaFormat`. */
  schemaFormat?: SchemaFormatName;
  /** The files `schema` refers to, by location, as `ConfigEditor`'s `schemaResources`. */
  schemaResources?: Record<string, string>;
  /** Options of `schemaFormat`, as `ConfigEditor`'s `schemaOptions`. */
  schemaOptions?: Record<string, unknown>;
  /** Edit in another language than `language`; switches live. A list with the same `editAs` changes nothing: to ask for it again (after an undo stepped back across the switch), call `editAs(id, language)`. */
  editAs?: LanguageName;
  /** The text on screen when the document's editor opens, in the `editAs` language, in place of `value` converted: what the user had on screen before the view went away (`editor(id).text`). `value` and `baseline` stay yours; it converts back like an edit, so `onChange` hears it once it converts, and until then `onSyncChange` says `false`. Undo starts at it. Read when the editor mounts (the first time the document is shown); ignored without `editAs`. */
  editAsText?: string;
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
  /** Set it when you list every `onChange` text back as the document's `value` in `update({ documents })`, possibly late and in any order: a `value` its editor gave that has not come back yet is then its echo and keeps what the editor holds (until 5 s after a later text came back: one skipped before it is then yours). A text it never gave applies; go back to one it gave with `replaceValue(id, text)`. Without it (the default) every listed `value` applies. Fixed for the view's life. */
  echoes?: boolean;
  /** Called with a document's id when it becomes the active one (a tab, `Alt+PageDown` / `Alt+PageUp`, `select()`, or the active one closing). */
  onSelect?: (id: string) => void;
  /** Called when the user asks to close a document (close button, middle click, `Delete` on its tab); only `closable` documents ask. Without it, tabs have no close button. */
  onClose?: (request: CloseRequest) => void;
  /** Called when the user asks for a new document: the "+" button after the last tab, shown only with it (it stays at the strip's end when the tabs overflow). It only asks: add the new document to the list; the next one added is selected and its editor focused. No key adds one (`Ctrl+T` / `Ctrl+N` are the browser's). */
  onAdd?: () => void;
  /** Called with a document's id and text after every edit: store it as the document's `value`. */
  onChange?: (id: string, value: string) => void;
  /** Called when the user saves a document (`Ctrl+S` / `Cmd+S`) or you call `save(id)` / `saveAll()`. */
  onSave?: (id: string, value: string, language: LanguageName) => void;
  /** Called with a document's id and whether it differs from its baseline: when it is added, then each time that changes. */
  onDirtyChange?: (id: string, dirty: boolean) => void;
  /** Called with a document's id and its editor's own problems (as `ConfigEditor`'s `onProblems`): once its editor mounts (when the document is first shown), then each time they change. */
  onProblems?: (id: string, problems: Diagnostic[]) => void;
  /** Called with a document's id and the language on screen in its editor: once when the editor mounts (the first time the document is shown), then each time that changes (a switch made, one that waited until the text converted, undo or redo across a switch). */
  onEditAsChange?: (id: string, language: LanguageName) => void;
  /** Called with a document's id and whether the text on screen is written back to its `value`: once when its editor mounts, then each time that changes (see `ConfigEditor`'s `onSyncChange`). */
  onSyncChange?: (id: string, inSync: boolean) => void;
}

/**
 * What `view.update` takes: the documents, the active one, the settings and
 * the callbacks, any of them. A key left out (or `undefined`) stays as it
 * is; `null` removes a callback or returns a setting to its default;
 * settings merge key by key. `className`, `label` and `echoes` cannot change.
 */
export interface DocumentsUpdate extends SettingsUpdate {
  /** Replaces the list: a document keeps its editor while its `id` stays listed. */
  documents?: DocumentSpec[];
  /** Shows this document, leaving the focus where it is. */
  activeId?: string;
  onSelect?: ((id: string) => void) | null;
  /** Replaces or removes it; it cannot be added to a view mounted without one (its tabs have no close buttons). */
  onClose?: ((request: CloseRequest) => void) | null;
  /** Replaces or removes it; it cannot be added to a view mounted without one (its strip has no "+"). */
  onAdd?: (() => void) | null;
  onChange?: ((id: string, value: string) => void) | null;
  /** `null` gives `Ctrl+S` back to the browser. */
  onSave?: ((id: string, value: string, language: LanguageName) => void) | null;
  onDirtyChange?: ((id: string, dirty: boolean) => void) | null;
  onProblems?: ((id: string, problems: Diagnostic[]) => void) | null;
  onEditAsChange?: ((id: string, language: LanguageName) => void) | null;
  onSyncChange?: ((id: string, inSync: boolean) => void) | null;
  /** The view is built from it: create a new one to change it. */
  className?: never;
  /** The view is built from it: create a new one to change it. */
  label?: never;
  /** The view is built from it: create a new one to change it. */
  echoes?: never;
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
    #[wasm_bindgen(typescript_type = "(EditorRange | null)[]")]
    pub type RangeResults;
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
