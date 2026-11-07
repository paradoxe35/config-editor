//! Browser bindings for `config-editor-leptos`, published to npm as
//! `config-editor`. The editor itself is the crate's Leptos component, mounted
//! into any element; JavaScript sees a plain class and functions. Framework
//! wrappers (`config-editor-react`, …) build on this package.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::HashMap;

use config_editor_leptos::language::{
    ConfigLanguage, LanguageConfig, LanguageInfo, Lexical, ParseError, SchemaPathSegment, Span,
    Token, TokenKind, Tokens,
};
use config_editor_leptos::{
    ChangeSummary, ConfigEditor as Editor, EditorHandle, EditorTheme, FieldPath, HostDiagnostic,
    Language, Schema, SchemaSource, SecretOptions, Severity, TextPosition, TextRange, ThemeColor,
    ThemeColors,
};
use js_sys::{Array, Function, Object, Reflect};
use leptos::prelude::*;
use serde::Serialize;
use serde_json::Value;
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
}

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

thread_local! {
    /// Languages registered from JavaScript, by lowercase label.
    static REGISTERED: RefCell<HashMap<String, Language>> = RefCell::new(HashMap::new());
}

fn language_named(name: &str) -> Result<Language, JsError> {
    let key = name.trim().to_lowercase();
    if let Some(language) = REGISTERED.with(|languages| languages.borrow().get(&key).copied()) {
        return Ok(language);
    }
    // A registered language's file extension names it too, as `yml` does.
    let extension = key.trim_start_matches('.');
    if let Some(language) = REGISTERED.with(|languages| {
        languages.borrow().values().copied().find(|language| {
            language
                .extensions()
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
    }) {
        return Ok(language);
    }
    name.parse::<Language>()
        .map_err(|error| JsError::new(&error.to_string()))
}

/// `language` if documents can be written in it: an `editAs` target must.
fn writable_edit_as(language: Language) -> Result<Language, JsError> {
    language
        .serialize(&Value::Object(serde_json::Map::new()))
        .map(|_| language)
        .map_err(|error| JsError::new(&format!("editAs: {error}")))
}

fn theme_named(name: &str) -> Result<EditorTheme, JsError> {
    name.parse::<EditorTheme>()
        .map_err(|error| JsError::new(&error.to_string()))
}

fn colors_from(value: &JsValue) -> Result<ThemeColors, JsError> {
    let mut colors = ThemeColors::new();
    if value.is_undefined() || value.is_null() {
        return Ok(colors);
    }
    if !value.is_object() || Array::is_array(value) || value.is_function() {
        return Err(JsError::new(
            "colors must be an object of token: CSS color, such as { key: \"#ffcc66\" }",
        ));
    }
    let entries = Object::entries(value.unchecked_ref());
    for entry in entries.iter() {
        let pair: Array = entry.unchecked_into();
        let token = pair.get(0).as_string().unwrap_or_default();
        let css = pair.get(1).as_string().ok_or_else(|| {
            JsError::new(&format!("color \"{token}\" must be a CSS color string"))
        })?;
        // As the browser reads colors (`var(--x)` included): a typo or a
        // `url(…)` would otherwise be dropped, or fetched, silently.
        if !web_sys::css::supports_with_value("color", &css).unwrap_or(false) {
            return Err(JsError::new(&format!(
                "color \"{token}\": \"{css}\" is not a CSS color"
            )));
        }
        let color = ThemeColor::ALL
            .iter()
            .copied()
            .find(|color| color.css_var().strip_prefix("--config-editor-") == Some(token.as_str()))
            .ok_or_else(|| JsError::new(&format!("unknown color token \"{token}\"")))?;
        colors = colors.set(color, css);
    }
    Ok(colors)
}

/// A string argument, or a `TypeError`-style error naming it.
fn text_arg(value: &JsValue, name: &str) -> Result<String, JsError> {
    value
        .as_string()
        .ok_or_else(|| JsError::new(&format!("{name} must be a string")))
}

/// An optional string argument: absent (`undefined`/`null`) or a string.
fn optional_text(value: &JsValue, name: &str) -> Result<Option<String>, JsError> {
    if value.is_undefined() || value.is_null() {
        Ok(None)
    } else {
        text_arg(value, name).map(Some)
    }
}

thread_local! {
    /// How many host callbacks are running: `destroy` from inside one waits
    /// until the editor's own handler has returned.
    static IN_CALLBACK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Calls a host callback; an exception it throws is logged, not swallowed.
fn call_host(callback: &Function, argument: &JsValue) {
    IN_CALLBACK.with(|depth| depth.set(depth.get() + 1));
    let result = callback.call1(&JsValue::NULL, argument);
    IN_CALLBACK.with(|depth| depth.set(depth.get() - 1));
    if let Err(error) = result {
        web_sys::console::error_2(
            &JsValue::from_str("config-editor: a callback threw"),
            &error,
        );
    }
}

fn field(object: &JsValue, name: &str) -> JsValue {
    Reflect::get(object, &JsValue::from_str(name)).unwrap_or(JsValue::UNDEFINED)
}

fn string_field(object: &JsValue, name: &str) -> Option<String> {
    field(object, name).as_string()
}

/// The constructor's options, for the unknown-option warning.
const OPTIONS: [&str; 23] = [
    "value",
    "language",
    "editAs",
    "schema",
    "baseline",
    "fileName",
    "theme",
    "colors",
    "readOnly",
    "showToolbar",
    "showFooter",
    "search",
    "goToLine",
    "className",
    "onChange",
    "onEditAsChange",
    "onChangeSummary",
    "onPathChange",
    "hostDiagnostics",
    "onSchemaWarning",
    "readOnlyPaths",
    "readOnlyRanges",
    "secrets",
];

/// Keys `secrets` can hold.
const SECRET_KEYS: &[&str] = &["placeholder", "isPlaceholder", "hint", "referencePrefixes"];

/// Logs a warning for each option the editor does not know (`readonly` for
/// `readOnly`), which would otherwise be ignored without a word.
fn warn_unknown_options(options: &JsValue) {
    warn_unknown_keys(options, &OPTIONS, "option");
}

/// Keys a `registerLanguage` definition can hold.
const DEFINITION_KEYS: &[&str] = &[
    "label",
    "icon",
    "extensions",
    "parse",
    "serialize",
    "keySpan",
    "valueSpan",
    "highlight",
    "comments",
    "brackets",
    "separators",
];

/// Keys `setCallbacks` takes.
const CALLBACK_KEYS: &[&str] = &[
    "onChange",
    "onEditAsChange",
    "onChangeSummary",
    "onPathChange",
];

/// A console warning for each key of `object` not in `known`, with the
/// closest known one as a hint (`valu` → `value`).
fn warn_unknown_keys(object: &JsValue, known: &[&str], what: &str) {
    if !object.is_object() {
        return;
    }
    for key in Object::keys(object.unchecked_ref::<Object>()).iter() {
        let Some(key) = key.as_string() else { continue };
        if known.contains(&key.as_str()) {
            continue;
        }
        let hint = known
            .iter()
            .map(|name| {
                (
                    edit_distance(&key.to_lowercase(), &name.to_lowercase()),
                    name,
                )
            })
            .filter(|(distance, _)| *distance <= 2)
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, name)| format!(" (did you mean \"{name}\"?)"))
            .unwrap_or_default();
        web_sys::console::warn_1(&JsValue::from_str(&format!(
            "config-editor: unknown {what} \"{key}\" is ignored{hint}"
        )));
    }
}

/// Levenshtein distance, for "did you mean" hints.
fn edit_distance(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut row: Vec<usize> = (0..=right.len()).collect();
    for (index, left_char) in left.chars().enumerate() {
        let mut previous = row[0];
        row[0] = index + 1;
        for (column, right_char) in right.iter().enumerate() {
            let substitute = previous + usize::from(left_char != *right_char);
            previous = row[column + 1];
            row[column + 1] = substitute.min(row[column] + 1).min(previous + 1);
        }
    }
    row[right.len()]
}

/// An optional string option: absent, or a string (anything else is an error).
fn string_option(options: &JsValue, name: &str) -> Result<Option<String>, JsError> {
    optional_text(&field(options, name), name)
}

/// An optional boolean option: absent, or a boolean.
fn bool_option(options: &JsValue, name: &str) -> Result<Option<bool>, JsError> {
    let value = field(options, name);
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    value
        .as_bool()
        .map(Some)
        .ok_or_else(|| JsError::new(&format!("{name} must be a boolean")))
}

/// An optional callback option: absent, or a function.
fn function_option(options: &JsValue, name: &str) -> Result<Option<Function>, JsError> {
    let value = field(options, name);
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    value
        .dyn_into::<Function>()
        .map(Some)
        .map_err(|_| JsError::new(&format!("{name} must be a function")))
}

/// Deepest nesting accepted from a custom language's `parse`.
const MAX_DEPTH: usize = 256;

/// A JavaScript value as JSON: plain data only, bounded in depth so a cyclic
/// or runaway result is an error instead of a crash.
fn json_from_js(value: &JsValue, depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "it nests deeper than {MAX_DEPTH} levels (or is cyclic)"
        ));
    }
    if value.is_null() {
        return Ok(Value::Null);
    }
    if let Some(flag) = value.as_bool() {
        return Ok(Value::Bool(flag));
    }
    if let Some(number) = value.as_f64() {
        // Whole numbers stay integers (5, not 5.0), as JSON.parse reads them.
        if number.fract() == 0.0 && number.abs() < 9.007_199_254_740_992e15 {
            return Ok(Value::from(number as i64));
        }
        return serde_json::Number::from_f64(number)
            .map(Value::Number)
            .ok_or_else(|| format!("{number} is not a JSON number"));
    }
    if let Some(text) = value.as_string() {
        return Ok(Value::String(text));
    }
    if Array::is_array(value) {
        return Array::from(value)
            .iter()
            .map(|item| json_from_js(&item, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    if value.is_object() && !value.is_function() {
        let mut map = serde_json::Map::new();
        for entry in Object::entries(value.unchecked_ref()).iter() {
            let pair: Array = entry.unchecked_into();
            let key = pair.get(0).as_string().unwrap_or_default();
            let item = pair.get(1);
            if item.is_undefined() {
                continue;
            }
            map.insert(key, json_from_js(&item, depth + 1)?);
        }
        return Ok(Value::Object(map));
    }
    Err("it holds a value JSON cannot (undefined, a function, a symbol, or a bigint)".to_string())
}

/// The live state a mounted editor reads; setters write it.
#[derive(Clone, Copy)]
struct State {
    value: RwSignal<String>,
    baseline: RwSignal<String>,
    edit_as: RwSignal<Option<Language>>,
    theme: RwSignal<EditorTheme>,
    colors: RwSignal<ThemeColors>,
    read_only: RwSignal<bool>,
    on_change: StoredValue<Option<Function>, LocalStorage>,
    on_edit_as_change: StoredValue<Option<Function>, LocalStorage>,
    on_change_summary: StoredValue<Option<Function>, LocalStorage>,
    on_path_change: StoredValue<Option<Function>, LocalStorage>,
    /// Whether `onPathChange` is set: only then is the caret's path computed.
    follows_path: RwSignal<bool>,
    handle: EditorHandle,
    host_diagnostics: RwSignal<Vec<HostDiagnostic>>,
    read_only_paths: RwSignal<Vec<FieldPath>>,
    read_only_ranges: RwSignal<Vec<TextRange>>,
}

/// The latest document, outside the reactive arena: an edit saved while the
/// editor unmounts arrives after its signals are gone.
type Latest = std::sync::Arc<std::sync::Mutex<String>>;

/// A config editor mounted into an element.
///
/// ```js
/// import init, { ConfigEditor } from "@pngwasi/config-editor";
/// await init();
/// const editor = new ConfigEditor(document.getElementById("config"), {
///   language: "yaml",
///   value: "listen: 7835\n",
///   schema: schemaJson,
///   onChange: (text) => save(text),
/// });
/// ```
#[wasm_bindgen]
pub struct ConfigEditor {
    state: State,
    latest: Latest,
    /// The document when `destroy` ran, so `value` still answers afterwards.
    final_value: Option<String>,
    /// The editor's entry in [`MOUNTED`] until `destroy`.
    id: Option<u32>,
}

/// The unmount handle, reactive owner, and element of an editor.
type Mounted = (Box<dyn std::any::Any>, Owner, web_sys::HtmlElement);

/// Unmounts the editors mounted in `element` or inside it: mounting a new one
/// there replaces them (as the element's content), and they would otherwise
/// stay alive, unseen, until the page closes.
fn replace_editors_in(element: &web_sys::HtmlElement) {
    let replaced: Vec<Mounted> = MOUNTED.with(|editors| {
        let mut editors = editors.borrow_mut();
        let ids: Vec<u32> = editors
            .iter()
            .filter(|(_, (_, _, mounted_in))| element.contains(Some(mounted_in)))
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| editors.remove(&id))
            .collect()
    });
    for (mounted, owner, _) in replaced {
        drop(mounted);
        owner.cleanup();
    }
}

thread_local! {
    /// Mounted editors, kept here rather than in the JavaScript object: an
    /// instance the page no longer references is garbage-collected (and its
    /// Rust side dropped), but the editor stays on the page until `destroy`.
    static MOUNTED: RefCell<HashMap<u32, Mounted>> = RefCell::new(HashMap::new());
    static NEXT_ID: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[wasm_bindgen]
impl ConfigEditor {
    /// Mounts an editor into `element` (its content is replaced).
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_param_type = "HTMLElement")] element: JsValue,
        options: EditorOptions,
    ) -> Result<ConfigEditor, JsError> {
        let element: web_sys::HtmlElement = element.dyn_into().map_err(|_| {
            JsError::new(
                "ConfigEditor needs an HTMLElement to mount into (got null or another value)",
            )
        })?;
        let options: JsValue = options.into();
        let language = match string_option(&options, "language")? {
            Some(name) => language_named(&name)?,
            None => Language::default(),
        };
        let edit_as = string_option(&options, "editAs")?
            .map(|name| language_named(&name).and_then(writable_edit_as))
            .transpose()?
            .filter(|edit| *edit != language);
        let theme = match string_option(&options, "theme")? {
            Some(name) => theme_named(&name)?,
            None => EditorTheme::default(),
        };
        let colors = colors_from(&field(&options, "colors"))?;
        let schema = match string_option(&options, "schema")? {
            Some(json) => {
                Some(Schema::compile(&json).map_err(|error| JsError::new(&error.to_string()))?)
            }
            None => None,
        };
        let value = string_option(&options, "value")?.unwrap_or_default();
        let baseline = string_option(&options, "baseline")?.unwrap_or_else(|| value.clone());
        let file_name = string_option(&options, "fileName")?;
        let class = string_option(&options, "className")?;
        let show_toolbar = bool_option(&options, "showToolbar")?.unwrap_or(false);
        let show_footer = bool_option(&options, "showFooter")?.unwrap_or(false);
        let read_only = bool_option(&options, "readOnly")?.unwrap_or(false);
        let enable_search = bool_option(&options, "search")?.unwrap_or(true);
        let enable_go_to_line = bool_option(&options, "goToLine")?.unwrap_or(true);
        let on_change = function_option(&options, "onChange")?;
        let on_edit_as_change = function_option(&options, "onEditAsChange")?;
        let on_change_summary = function_option(&options, "onChangeSummary")?;
        let on_path_change = function_option(&options, "onPathChange")?;
        let host_diagnostics = match field(&options, "hostDiagnostics") {
            list if list.is_undefined() || list.is_null() => Vec::new(),
            list => host_diagnostics_from(&list, &value)?,
        };
        let on_schema_warning = function_option(&options, "onSchemaWarning")?;
        let read_only_paths = paths_from(&field(&options, "readOnlyPaths"), "readOnlyPaths")?;
        let read_only_ranges =
            ranges_from(&field(&options, "readOnlyRanges"), &value, "readOnlyRanges")?;
        warn_unknown_options(&options);
        // The editor replaces whatever the element held, editors included.
        replace_editors_in(&element);
        element.set_text_content(None);

        let latest: Latest = std::sync::Arc::new(std::sync::Mutex::new(value.clone()));
        let latest_for_view = latest.clone();
        let owner = Owner::new();
        let secrets = owner.with(|| secrets_from(&field(&options, "secrets")))?;
        let state = owner.with(|| State {
            value: RwSignal::new(value.clone()),
            baseline: RwSignal::new(baseline),
            edit_as: RwSignal::new(edit_as),
            theme: RwSignal::new(theme),
            colors: RwSignal::new(colors),
            read_only: RwSignal::new(read_only),
            on_change: StoredValue::new_local(on_change),
            follows_path: RwSignal::new(on_path_change.is_some()),
            on_edit_as_change: StoredValue::new_local(on_edit_as_change),
            on_change_summary: StoredValue::new_local(on_change_summary),
            on_path_change: StoredValue::new_local(on_path_change),
            handle: EditorHandle::new(),
            host_diagnostics: RwSignal::new(host_diagnostics),
            read_only_paths: RwSignal::new(read_only_paths),
            read_only_ranges: RwSignal::new(read_only_ranges),
        });
        let mount_element = element.clone();
        let mounted = owner.with(|| {
            leptos::mount::mount_to(element, move || {
                let latest = latest_for_view.clone();
                // The callback is taken out first: it may replace itself or
                // destroy the editor.
                let changed = move |text: String| {
                    *latest
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = text.clone();
                    let _ = state.value.try_set(text.clone());
                    if let Some(callback) = state.on_change.try_get_value().flatten() {
                        call_host(&callback, &JsValue::from_str(&text));
                    }
                };
                let switched = move |language: Language| {
                    if let Some(callback) = state.on_edit_as_change.try_get_value().flatten() {
                        call_host(
                            &callback,
                            &JsValue::from_str(&language.label().to_lowercase()),
                        );
                    }
                };
                let summarized = move |summary: ChangeSummary| {
                    if let Some(callback) = state.on_change_summary.try_get_value().flatten() {
                        call_host(&callback, &to_js(&ChangeSummaryOut::from(summary)));
                    }
                };
                follow_path(state);
                let host_diagnostics = Signal::derive(move || state.host_diagnostics.get());
                let read_only_paths = Signal::derive(move || state.read_only_paths.get());
                let read_only_ranges = Signal::derive(move || state.read_only_ranges.get());
                let warn_with = StoredValue::new_local(on_schema_warning.clone());
                let schema_warning = move |message: String| match warn_with
                    .try_get_value()
                    .flatten()
                {
                    Some(callback) => call_host(&callback, &JsValue::from_str(&message)),
                    None => web_sys::console::warn_1(&format!("config-editor: {message}").into()),
                };
                let edit_as = Signal::derive(move || state.edit_as.get());
                let value_signal = Signal::derive(move || state.value.get());
                let baseline = Signal::derive(move || state.baseline.get());
                let theme = Signal::derive(move || state.theme.get());
                let colors = Signal::derive(move || Some(state.colors.get()));
                let read_only = Signal::derive(move || state.read_only.get());
                let file_name = file_name.clone().unwrap_or_default();
                let class = class.clone().unwrap_or_default();
                // An optional prop takes its value, not an `Option`.
                match schema.clone() {
                    Some(schema) => view! {
                        <Editor
                            language=language
                            edit_as=edit_as
                            initial_value=value.clone()
                            value=value_signal
                            baseline=baseline
                            schema=SchemaSource::from(schema)
                            file_name=file_name
                            class=class
                            theme=theme
                            colors=colors
                            read_only=read_only
                            show_toolbar=show_toolbar
                            show_footer=show_footer
                            enable_search=enable_search
                            enable_go_to_line=enable_go_to_line
                            on_change=changed
                            on_edit_as_change=switched
                            on_change_summary=summarized
                            handle=state.handle
                            host_diagnostics=host_diagnostics
                            on_schema_warning=schema_warning
                            read_only_paths=read_only_paths
                            read_only_ranges=read_only_ranges
                            secrets=secrets.clone()
                        />
                    }
                    .into_any(),
                    None => view! {
                        <Editor
                            language=language
                            edit_as=edit_as
                            initial_value=value.clone()
                            value=value_signal
                            baseline=baseline
                            file_name=file_name
                            class=class
                            theme=theme
                            colors=colors
                            read_only=read_only
                            show_toolbar=show_toolbar
                            show_footer=show_footer
                            enable_search=enable_search
                            enable_go_to_line=enable_go_to_line
                            on_change=changed
                            on_edit_as_change=switched
                            on_change_summary=summarized
                            handle=state.handle
                            host_diagnostics=host_diagnostics
                            read_only_paths=read_only_paths
                            read_only_ranges=read_only_ranges
                            secrets=secrets.clone()
                        />
                    }
                    .into_any(),
                }
            })
        });
        let id = NEXT_ID.with(|next| {
            let id = next.get();
            next.set(id.wrapping_add(1));
            id
        });
        MOUNTED.with(|editors| {
            editors
                .borrow_mut()
                .insert(id, (Box::new(mounted), owner, mount_element));
        });
        Ok(ConfigEditor {
            state,
            latest,
            final_value: None,
            id: Some(id),
        })
    }

    /// The document, in `language`.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> String {
        // Input the browser sent in this task, and an Edit-as edit still
        // waiting out the typing pause, first: the document as shown.
        config_editor_leptos::flush_pending_edits();
        match &self.final_value {
            Some(value) => value.clone(),
            None => self
                .latest
                .lock()
                .map(|text| text.clone())
                .unwrap_or_default(),
        }
    }

    /// Replaces the document (as a host update; the caret is kept when the
    /// text is unchanged).
    #[wasm_bindgen(js_name = setValue)]
    pub fn set_value(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] value: JsValue,
    ) -> Result<(), JsError> {
        // Input the browser sent in this task, and an Edit-as edit still
        // waiting out the typing pause, first: the reset replaces them.
        config_editor_leptos::flush_pending_edits();
        if self.id.is_none() {
            return Ok(());
        }
        let value = text_arg(&value, "value")?;
        *self
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = value.clone();
        let _ = self.state.value.try_set(value);
        Ok(())
    }

    /// Sets the text the diff gutter compares against.
    #[wasm_bindgen(js_name = setBaseline)]
    pub fn set_baseline(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] baseline: JsValue,
    ) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let _ = self
            .state
            .baseline
            .try_set(text_arg(&baseline, "baseline")?);
        Ok(())
    }

    /// Edits in another language, or in `language` again with `null`.
    #[wasm_bindgen(js_name = setEditAs)]
    pub fn set_edit_as(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string | null | undefined")] language: JsValue,
    ) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let language = if language.is_null() || language.is_undefined() {
            None
        } else {
            Some(writable_edit_as(language_named(&text_arg(
                &language, "editAs",
            )?)?)?)
        };
        let _ = self.state.edit_as.try_set(language);
        Ok(())
    }

    /// Switches the theme; `null` returns to the default one.
    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(
        &self,
        #[wasm_bindgen(unchecked_param_type = "ThemeName | null | undefined")] theme: JsValue,
    ) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let theme = match optional_text(&theme, "theme")? {
            Some(name) => theme_named(&name)?,
            None => EditorTheme::default(),
        };
        let _ = self.state.theme.try_set(theme);
        Ok(())
    }

    /// Replaces the color overrides; `{}` clears them.
    #[wasm_bindgen(js_name = setColors)]
    pub fn set_colors(&self, colors: ColorMap) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let _ = self.state.colors.try_set(colors_from(&colors.into())?);
        Ok(())
    }

    #[wasm_bindgen(js_name = setReadOnly)]
    #[allow(non_snake_case, reason = "the parameter name shows in TypeScript")]
    pub fn set_read_only(
        &self,
        #[wasm_bindgen(unchecked_param_type = "boolean")] readOnly: JsValue,
    ) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let read_only = readOnly
            .as_bool()
            .ok_or_else(|| JsError::new("readOnly must be a boolean"))?;
        let _ = self.state.read_only.try_set(read_only);
        Ok(())
    }

    /// Replaces the change callbacks (`onChange`, `onEditAsChange`).
    #[wasm_bindgen(js_name = setCallbacks)]
    pub fn set_callbacks(&self, callbacks: EditorUpdate) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let callbacks: JsValue = callbacks.into();
        if !callbacks.is_object() || Array::is_array(&callbacks) {
            return Err(JsError::new(
                "setCallbacks takes an object: { onChange?, onEditAsChange? }",
            ));
        }
        warn_unknown_keys(&callbacks, CALLBACK_KEYS, "callback");
        // Check both before changing either.
        let read = |name: &str| -> Result<Option<Option<Function>>, JsError> {
            let value = field(&callbacks, name);
            if value.is_undefined() {
                Ok(None)
            } else if value.is_null() {
                Ok(Some(None))
            } else {
                value
                    .dyn_into::<Function>()
                    .map(|callback| Some(Some(callback)))
                    .map_err(|_| JsError::new(&format!("{name} must be a function or null")))
            }
        };
        // A function replaces a callback; `null` removes it; absent keeps it.
        let on_change = read("onChange")?;
        let on_edit_as_change = read("onEditAsChange")?;
        let on_change_summary = read("onChangeSummary")?;
        let on_path_change = read("onPathChange")?;
        if let Some(callback) = on_change {
            let _ = self.state.on_change.try_set_value(callback);
        }
        if let Some(callback) = on_edit_as_change {
            let _ = self.state.on_edit_as_change.try_set_value(callback);
        }
        if let Some(callback) = on_change_summary {
            let _ = self.state.on_change_summary.try_set_value(callback);
        }
        if let Some(callback) = on_path_change {
            let _ = self.state.follows_path.try_set(callback.is_some());
            let _ = self.state.on_path_change.try_set_value(callback);
        }
        Ok(())
    }

    /// The unified diff (`diff -u`) of the baseline against the document,
    /// with `context` unchanged lines around each change (default 3); empty
    /// when they are the same.
    #[wasm_bindgen(js_name = unifiedDiff)]
    pub fn unified_diff(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "number")] context: JsValue,
    ) -> Result<String, JsError> {
        let context = context_arg(&context)?;
        let current = self.value();
        let baseline = self
            .state
            .baseline
            .try_get_untracked()
            .unwrap_or_else(|| current.clone());
        Ok(config_editor_leptos::unified_diff(
            &baseline, &current, context,
        ))
    }

    /// Replaces the host's problems (a server's validation), shown next to
    /// the editor's own; `[]` clears them.
    #[wasm_bindgen(js_name = setHostDiagnostics)]
    pub fn set_host_diagnostics(
        &self,
        #[wasm_bindgen(unchecked_param_type = "HostDiagnostic[]")] diagnostics: JsValue,
    ) -> Result<(), JsError> {
        config_editor_leptos::flush_pending_input();
        let host_text = self
            .latest
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default();
        let diagnostics = if diagnostics.is_undefined() || diagnostics.is_null() {
            Vec::new()
        } else {
            host_diagnostics_from(&diagnostics, &host_text)?
        };
        let _ = self.state.host_diagnostics.try_set(diagnostics);
        Ok(())
    }

    /// Replaces the fields the user cannot change (on top of the schema's
    /// `readOnly` ones); `[]` clears them.
    #[wasm_bindgen(js_name = setReadOnlyPaths)]
    pub fn set_read_only_paths(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath[]")] paths: JsValue,
    ) -> Result<(), JsError> {
        config_editor_leptos::flush_pending_input();
        let paths = paths_from(&paths, "readOnlyPaths")?;
        let _ = self.state.read_only_paths.try_set(paths);
        Ok(())
    }

    /// Replaces the stretches of the document (in `language`) the user
    /// cannot change; `[]` clears them.
    #[wasm_bindgen(js_name = setReadOnlyRanges)]
    pub fn set_read_only_ranges(
        &self,
        #[wasm_bindgen(unchecked_param_type = "TextRangeInput[]")] ranges: JsValue,
    ) -> Result<(), JsError> {
        config_editor_leptos::flush_pending_input();
        let host_text = self
            .latest
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default();
        let ranges = ranges_from(&ranges, &host_text, "readOnlyRanges")?;
        let _ = self.state.read_only_ranges.try_set(ranges);
        Ok(())
    }

    /// Where the field at `path` is written in the text on screen, or `null`.
    #[wasm_bindgen(js_name = rangeOf)]
    pub fn range_of(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<RangeResult, JsError> {
        config_editor_leptos::flush_pending_input();
        let path = path_from_js(&path)?;
        let handle = self.state.handle;
        let found = handle.range_of(path).zip(handle.text());
        Ok(found
            .map_or(JsValue::NULL, |(range, text)| range_to_js(&text, range))
            .unchecked_into())
    }

    /// The path of what the text on screen holds at string index `offset`.
    #[wasm_bindgen(js_name = pathAt)]
    pub fn path_at(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] offset: JsValue,
    ) -> Result<PathResult, JsError> {
        config_editor_leptos::flush_pending_input();
        let offset = offset
            .as_f64()
            .filter(|offset| offset.is_finite() && *offset >= 0.0 && offset.fract() == 0.0)
            .ok_or_else(|| JsError::new("offset must be a non-negative integer"))?
            as usize;
        let handle = self.state.handle;
        let path = handle.text().and_then(|text| {
            if offset > text.encode_utf16().count() {
                return None;
            }
            handle.path_at(TextPosition::from_offset(&text, offset))
        });
        Ok(path
            .map_or(JsValue::NULL, |path| path_to_js(&path))
            .unchecked_into())
    }

    /// The path of what the caret is on.
    #[wasm_bindgen(getter, js_name = currentPath)]
    pub fn current_path(&self) -> PathResult {
        untrack(|| self.state.handle.current_path())
            .map_or(JsValue::NULL, |path| path_to_js(&path))
            .unchecked_into()
    }

    /// Scrolls to the field at `path`, selects it and focuses the editor;
    /// `false` when the document has no such field.
    pub fn reveal(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<bool, JsError> {
        config_editor_leptos::flush_pending_input();
        Ok(self.state.handle.reveal(path_from_js(&path)?))
    }

    /// Focuses the editor, keeping its selection.
    pub fn focus(&self) {
        self.state.handle.focus();
    }

    /// Unmounts the editor. The instance cannot be used afterwards. An edit
    /// still waiting to be saved (while editing as another language) is sent
    /// to `onChange` first.
    pub fn destroy(&mut self) {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let Some((mounted, owner, _)) = self
            .id
            .take()
            .and_then(|id| MOUNTED.with(|editors| editors.borrow_mut().remove(&id)))
        else {
            return;
        };
        let unmount = move || {
            drop(mounted);
            owner.cleanup();
        };
        if IN_CALLBACK.with(|depth| depth.get()) > 0 {
            // Called from a callback: the editor's handler is still running,
            // so it unmounts once that returns.
            self.final_value = Some(
                self.latest
                    .lock()
                    .map(|text| text.clone())
                    .unwrap_or_default(),
            );
            let deferred = Closure::once_into_js(unmount);
            if let Ok(queue) = field(&js_sys::global(), "queueMicrotask").dyn_into::<Function>() {
                let _ = queue.call1(&JsValue::NULL, &deferred);
            }
        } else {
            unmount();
            self.final_value = Some(
                self.latest
                    .lock()
                    .map(|text| text.clone())
                    .unwrap_or_default(),
            );
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticOut {
    // Absent, not `null`, without a position (as the typings say).
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_column: Option<usize>,
    message: String,
    severity: &'static str,
}

#[derive(Serialize)]
struct ChangeSummaryOut {
    added: usize,
    removed: usize,
    modified: usize,
    hunks: usize,
}

impl From<ChangeSummary> for ChangeSummaryOut {
    fn from(summary: ChangeSummary) -> Self {
        Self {
            added: summary.added,
            removed: summary.removed,
            modified: summary.modified,
            hunks: summary.hunks,
        }
    }
}

/// Unchanged lines around each change in a unified diff: a whole number,
/// 3 when absent.
fn context_arg(value: &JsValue) -> Result<usize, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(3);
    }
    value
        .as_f64()
        .filter(|context| context.fract() == 0.0 && *context >= 0.0)
        .map(|context| context.min(u32::MAX.into()) as usize)
        .ok_or_else(|| JsError::new("context must be a whole number of lines, 0 or more"))
}

/// The unified diff (`diff -u`) of `baseline` against `current`, with
/// `context` unchanged lines around each change (default 3): `--- baseline`
/// and `+++ current` headers, then `@@` hunks. Empty when they are the same.
#[wasm_bindgen(js_name = unifiedDiff)]
pub fn unified_diff(
    #[wasm_bindgen(unchecked_param_type = "string")] baseline: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] current: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "number")] context: JsValue,
) -> Result<String, JsError> {
    Ok(config_editor_leptos::unified_diff(
        &text_arg(&baseline, "baseline")?,
        &text_arg(&current, "current")?,
        context_arg(&context)?,
    ))
}

/// Host diagnostics from JavaScript; `host_text` (the document in
/// `language`) turns string-index ranges into lines and columns.
fn host_diagnostics_from(list: &JsValue, host_text: &str) -> Result<Vec<HostDiagnostic>, JsError> {
    if !Array::is_array(list) {
        return Err(JsError::new(
            "hostDiagnostics must be an array of { path | range, severity, message, code?, source?, docsUrl? }",
        ));
    }
    Array::from(list)
        .iter()
        .enumerate()
        .map(|(index, item)| {
            host_diagnostic_from(&item, host_text)
                .map_err(|error| JsError::new(&format!("hostDiagnostics[{index}]: {error}")))
        })
        .collect()
}

/// Keys a host diagnostic can hold.
const HOST_DIAGNOSTIC_KEYS: &[&str] = &[
    "path", "range", "severity", "message", "code", "source", "docsUrl",
];

fn host_diagnostic_from(item: &JsValue, host_text: &str) -> Result<HostDiagnostic, String> {
    if !item.is_object() || Array::is_array(item) {
        return Err("must be an object".to_string());
    }
    warn_unknown_keys(item, HOST_DIAGNOSTIC_KEYS, "host diagnostic key");
    let message = string_field(item, "message").ok_or("message must be a string")?;
    let severity = match field(item, "severity") {
        value if value.is_undefined() || value.is_null() => Severity::Error,
        value => value
            .as_string()
            .ok_or_else(|| "severity must be a string".to_string())?
            .parse::<Severity>()?,
    };
    let path = field(item, "path");
    let range = field(item, "range");
    let mut diagnostic = if !path.is_undefined() && !path.is_null() {
        let path = path_from_js(&path).map_err(|_| {
            "path must be a JSON pointer or an array of keys and indices".to_string()
        })?;
        HostDiagnostic::at_path(path, severity, message)
    } else if !range.is_undefined() && !range.is_null() {
        HostDiagnostic::at_range(range_from_js(&range, host_text)?, severity, message)
    } else {
        HostDiagnostic::document(severity, message)
    };
    let optional = |name: &str| -> Result<Option<String>, String> {
        let value = field(item, name);
        if value.is_undefined() || value.is_null() {
            return Ok(None);
        }
        value
            .as_string()
            .map(Some)
            .ok_or(format!("{name} must be a string"))
    };
    diagnostic.code = optional("code")?;
    diagnostic.source = optional("source")?;
    diagnostic.docs_url = optional("docsUrl")?;
    Ok(diagnostic)
}

/// A range from JavaScript: string indices `{ start, end }` into
/// `host_text`, or one-based `{ startLine, startColumn, endLine?, endColumn? }`.
/// The `secrets` option (absent: the defaults). Call inside the editor's
/// owner: a host check is kept there.
fn secrets_from(value: &JsValue) -> Result<SecretOptions, JsError> {
    let mut options = SecretOptions::default();
    if value.is_undefined() || value.is_null() {
        return Ok(options);
    }
    if !value.is_object() || Array::is_array(value) {
        return Err(JsError::new(
            "secrets must be an object: { placeholder?, isPlaceholder?, hint?, referencePrefixes? }",
        ));
    }
    warn_unknown_keys(value, SECRET_KEYS, "secrets key");
    if let Some(pattern) = optional_text(&field(value, "placeholder"), "secrets.placeholder")? {
        options = options.with_placeholder(pattern);
    }
    if let Some(hint) = optional_text(&field(value, "hint"), "secrets.hint")? {
        options = options.with_hint(hint);
    }
    let prefixes = field(value, "referencePrefixes");
    if !prefixes.is_undefined() && !prefixes.is_null() {
        let list = Array::is_array(&prefixes)
            .then(|| Array::from(&prefixes))
            .ok_or_else(|| JsError::new("secrets.referencePrefixes must be an array of strings"))?;
        let prefixes: Option<Vec<String>> = list.iter().map(|prefix| prefix.as_string()).collect();
        options = options.with_reference_prefixes(prefixes.ok_or_else(|| {
            JsError::new("secrets.referencePrefixes must be an array of strings")
        })?);
    }
    if let Some(check) = function_option(value, "isPlaceholder")? {
        let check = StoredValue::new_local(Some(check));
        options = options.with_matcher(move |token: String| {
            check
                .try_with_value(|check| {
                    check.as_ref().is_some_and(|check| {
                        check
                            .call1(&JsValue::NULL, &JsValue::from_str(&token))
                            .is_ok_and(|real| real.is_truthy())
                    })
                })
                .unwrap_or(false)
        });
    }
    Ok(options)
}

/// An optional list of paths (`readOnlyPaths`).
fn paths_from(list: &JsValue, name: &str) -> Result<Vec<FieldPath>, JsError> {
    if list.is_undefined() || list.is_null() {
        return Ok(Vec::new());
    }
    if !Array::is_array(list) {
        return Err(JsError::new(&format!(
            "{name} must be an array of paths (JSON pointers or arrays of keys and indices)"
        )));
    }
    Array::from(list)
        .iter()
        .map(|path| path_from_js(&path))
        .collect()
}

/// An optional list of ranges of `host_text` (`readOnlyRanges`).
fn ranges_from(list: &JsValue, host_text: &str, name: &str) -> Result<Vec<TextRange>, JsError> {
    if list.is_undefined() || list.is_null() {
        return Ok(Vec::new());
    }
    if !Array::is_array(list) {
        return Err(JsError::new(&format!("{name} must be an array of ranges")));
    }
    Array::from(list)
        .iter()
        .enumerate()
        .map(|(index, range)| {
            range_from_js(&range, host_text)
                .map_err(|error| JsError::new(&format!("{name}[{index}]: {error}")))
        })
        .collect()
}

fn range_from_js(range: &JsValue, host_text: &str) -> Result<TextRange, String> {
    let number = |name: &str| {
        field(range, name)
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|value| value as usize)
    };
    if let Some(start) = number("start") {
        let end = number("end").unwrap_or(start).max(start);
        return Ok(TextRange::new(
            TextPosition::from_offset(host_text, start),
            TextPosition::from_offset(host_text, end),
        ));
    }
    let line = number("startLine").filter(|line| *line >= 1);
    let column = number("startColumn").filter(|column| *column >= 1);
    let (Some(line), Some(column)) = (line, column) else {
        return Err(
            "range must be { start, end } (string indices) or { startLine, startColumn, endLine?, endColumn? } (one-based)"
                .to_string(),
        );
    };
    let end_line = number("endLine").unwrap_or(line).max(line);
    let end_column = number("endColumn").unwrap_or(column + 1);
    Ok(TextRange::new(
        TextPosition::new(line, column),
        TextPosition::new(end_line, end_column),
    ))
}

/// Calls `onPathChange` with the caret's path each time it changes, while one
/// is set (only then is the path computed).
fn follow_path(state: State) {
    Effect::new(move |_| {
        if !state.follows_path.get() {
            return;
        }
        let Some(path) = state.handle.current_path() else {
            return;
        };
        if let Some(callback) = state.on_path_change.try_get_value().flatten() {
            call_host(&callback, &path_to_js(&path));
        }
    });
}

/// A path from JavaScript: a JSON pointer, or keys (strings) and indices
/// (non-negative integers).
fn path_from_js(value: &JsValue) -> Result<FieldPath, JsError> {
    if let Some(pointer) = value.as_string() {
        return Ok(FieldPath::from(pointer));
    }
    if !Array::is_array(value) {
        return Err(JsError::new(
            "a path is a JSON pointer (\"/web/hosts/0\") or an array of keys and indices ([\"web\", \"hosts\", 0])",
        ));
    }
    let mut path = FieldPath::root();
    for segment in Array::from(value).iter() {
        path = if let Some(key) = segment.as_string() {
            path.key(key)
        } else if let Some(index) = segment
            .as_f64()
            .filter(|index| *index >= 0.0 && index.fract() == 0.0 && index.is_finite())
        {
            path.index(index as usize)
        } else {
            return Err(JsError::new(
                "a path segment is a key (string) or an index (non-negative integer)",
            ));
        };
    }
    Ok(path)
}

#[derive(Serialize)]
#[serde(untagged)]
enum SegmentOut {
    Key(String),
    Index(usize),
}

#[derive(Serialize)]
struct PathOut {
    pointer: String,
    segments: Vec<SegmentOut>,
}

fn path_to_js(path: &FieldPath) -> JsValue {
    to_js(&PathOut {
        pointer: path.to_pointer(),
        segments: path
            .segments()
            .iter()
            .map(|segment| match segment {
                SchemaPathSegment::Key(key) => SegmentOut::Key(key.clone()),
                SchemaPathSegment::ArrayItem(index) => SegmentOut::Index(*index),
            })
            .collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeOut {
    start: usize,
    end: usize,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

fn range_to_js(text: &str, range: TextRange) -> JsValue {
    to_js(&RangeOut {
        start: range.start.to_offset(text),
        end: range.end.to_offset(text),
        start_line: range.start.line,
        start_column: range.start.column,
        end_line: range.end.line,
        end_column: range.end.column,
    })
}

fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

/// Syntax and schema problems in `text`, as the editor shows them. Without a
/// schema only syntax is checked.
#[wasm_bindgen]
pub fn validate(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "string")] schema: JsValue,
) -> Result<Diagnostics, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let text = text_arg(&text, "text")?;
    let text = text.as_str();
    let schema = optional_text(&schema, "schema")?;
    let problems = match schema {
        Some(json) => Schema::compile(&json)
            .map_err(|error| JsError::new(&error.to_string()))?
            .validate(language, text),
        None => config_editor_leptos::check_syntax(language, text),
    };
    let out: Vec<DiagnosticOut> = problems
        .into_iter()
        .map(|problem| DiagnosticOut {
            line: problem.line,
            column: problem.column,
            end_column: problem.end_column,
            message: problem.message,
            severity: problem.severity.as_str(),
        })
        .collect();
    Ok(to_js(&out).unchecked_into())
}

/// Converts `text` from one language to another. With a schema, lone XML
/// elements where it expects a list convert as one-item lists.
#[wasm_bindgen]
pub fn convert(
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] from: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] to: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "string")] schema: JsValue,
) -> Result<ConversionResult, JsError> {
    let schema = optional_text(&schema, "schema")?;
    let from = language_named(&text_arg(&from, "from")?)?;
    let to = language_named(&text_arg(&to, "to")?)?;
    let text = text_arg(&text, "text")?;
    let text = text.as_str();
    let converted = match schema {
        Some(json) => Schema::compile(&json)
            .map_err(|error| JsError::new(&error.to_string()))?
            .convert(from, text, to),
        None => from.convert(text, to),
    }
    .map_err(|error| JsError::new(&error.to_string()))?;
    #[derive(Serialize)]
    struct Out {
        text: String,
        losses: Vec<String>,
    }
    Ok(to_js(&Out {
        text: converted.text,
        losses: converted.losses,
    })
    .unchecked_into())
}

/// What the editor cannot use in a JSON Schema, one message each, such as a
/// `patternProperties` regex its engine cannot compile. An invalid schema
/// throws.
#[wasm_bindgen(js_name = schemaWarnings)]
pub fn schema_warnings(
    #[wasm_bindgen(unchecked_param_type = "string")] schema: JsValue,
) -> Result<Names, JsError> {
    let schema = Schema::compile(&text_arg(&schema, "schema")?)
        .map_err(|error| JsError::new(&error.to_string()))?;
    Ok(to_js(&schema.warnings()).unchecked_into())
}

/// Names of the built-in and registered languages.
#[wasm_bindgen]
pub fn languages() -> Names {
    let mut names: Vec<String> = Language::ALL
        .iter()
        .map(|language| language.label().to_lowercase())
        .collect();
    REGISTERED.with(|languages| names.extend(languages.borrow().keys().cloned()));
    to_js(&names).unchecked_into()
}

/// Slugs of the bundled themes.
#[wasm_bindgen]
pub fn themes() -> Names {
    let names: Vec<&str> = EditorTheme::ALL
        .iter()
        .map(|theme| theme.as_str())
        .collect();
    to_js(&names).unchecked_into()
}

/// Token names accepted in `colors`.
#[wasm_bindgen(js_name = colorTokens)]
pub fn color_tokens() -> Names {
    let names: Vec<&str> = ThemeColor::ALL
        .iter()
        .filter_map(|color| color.css_var().strip_prefix("--config-editor-"))
        .collect();
    to_js(&names).unchecked_into()
}

/// A language implemented in JavaScript. The editor gives it what the shared
/// core provides from `parse` and the spans it locates: schema validation
/// with positions, conversion to and from every other language, Edit-as,
/// and the editing behavior its configuration describes. Schema completion
/// needs a language's own completion context, which only the built-in
/// languages have.
struct JsLanguage {
    definition: JsValue,
    info: LanguageInfo,
    config: LanguageConfig,
}

// SAFETY: wasm32 without the `atomics` target feature runs on one thread, so
// the JS object is never touched from another thread. The trait requires
// Send + Sync for hosts with threads.
#[cfg(not(target_feature = "atomics"))]
unsafe impl Send for JsLanguage {}
#[cfg(not(target_feature = "atomics"))]
unsafe impl Sync for JsLanguage {}

impl JsLanguage {
    fn method(&self, name: &str) -> Option<Function> {
        field(&self.definition, name).dyn_into::<Function>().ok()
    }

    fn span(&self, name: &str, source: &str, pointer: &str) -> Option<Span> {
        let span = self
            .method(name)?
            .call2(
                &self.definition,
                &JsValue::from_str(source),
                &JsValue::from_str(pointer),
            )
            .ok()?;
        let number = |key: &str| {
            field(&span, key)
                .as_f64()
                .filter(|value| value.is_finite() && *value >= 1.0)
                .map(|value| value as usize)
        };
        // A position outside the text is no position (the problem is shown
        // unplaced); columns are clamped onto their line.
        let line = number("line")?;
        let text = source.split('\n').nth(line - 1)?;
        let width = text.chars().count() + 1;
        let start = number("startColumn")?.min(width);
        let end = number("endColumn")?.min(width).max(start);
        Some(Span {
            line,
            start_column: start,
            end_column: end,
        })
    }
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

#[cfg(not(target_feature = "atomics"))]
impl ConfigLanguage for JsLanguage {
    fn info(&self) -> LanguageInfo {
        self.info
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let parse = self.method("parse").ok_or_else(|| ParseError {
            message: "the language has no parse()".to_string(),
            line: None,
            column: None,
        })?;
        match parse.call1(&self.definition, &JsValue::from_str(source)) {
            Ok(value) if value.is_undefined() => Err(ParseError {
                message: "parse() returned nothing".to_string(),
                line: None,
                column: None,
            }),
            Ok(value) => json_from_js(&value, 0).map_err(|error| ParseError {
                message: format!("parse() returned a value that is not JSON: {error}"),
                line: None,
                column: None,
            }),
            Err(thrown) => {
                // A position outside the text is clamped onto it.
                let lines: Vec<&str> = source.split('\n').collect();
                let number = |key: &str| {
                    field(&thrown, key)
                        .as_f64()
                        .filter(|value| value.is_finite() && *value >= 1.0)
                        .map(|value| value as usize)
                };
                let line = number("line").map(|line| line.min(lines.len()));
                let column = number("column").map(|column| match line {
                    Some(line) => column.min(lines[line - 1].chars().count() + 1),
                    None => column,
                });
                Err(ParseError {
                    message: string_field(&thrown, "message")
                        .or_else(|| thrown.as_string())
                        .unwrap_or_else(|| "parse error".to_string()),
                    line,
                    column,
                })
            }
        }
    }

    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        let serialize = self
            .method("serialize")
            .ok_or_else(|| format!("{} cannot write documents", self.info.label))?;
        serialize
            .call1(&self.definition, &to_js(value))
            .map_err(|error| {
                string_field(&error, "message")
                    .or_else(|| error.as_string())
                    .unwrap_or_else(|| "serialize() failed".to_string())
            })?
            .as_string()
            .ok_or_else(|| "serialize() must return a string".to_string())
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        self.span("keySpan", source, pointer)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        self.span("valueSpan", source, pointer)
    }

    fn highlight(&self, line: &str) -> Vec<Token> {
        let Some(highlight) = self.method("highlight") else {
            return Vec::new();
        };
        let Ok(tokens) = highlight.call1(&self.definition, &JsValue::from_str(line)) else {
            return Vec::new();
        };
        if !Array::is_array(&tokens) {
            return Vec::new();
        }
        let mut out = Tokens::new();
        for token in Array::from(&tokens).iter() {
            let number = |key: &str| field(&token, key).as_f64().map(|value| value as usize);
            let kind = match string_field(&token, "kind").as_deref() {
                Some("key") => TokenKind::Key,
                Some("string") => TokenKind::String,
                Some("literal") => TokenKind::Literal,
                Some("comment") => TokenKind::Comment,
                Some("punctuation") => TokenKind::Punctuation,
                _ => TokenKind::Value,
            };
            // JavaScript string indices (UTF-16 units, as `indexOf` gives).
            let byte = |unit: usize| {
                let mut units = 0;
                for (index, character) in line.char_indices() {
                    if units >= unit {
                        return (units == unit).then_some(index);
                    }
                    units += character.len_utf16();
                }
                (units == unit).then_some(line.len())
            };
            if let (Some(start), Some(end)) = (number("start"), number("end"))
                && start < end
                && let (Some(start), Some(end)) = (byte(start), byte(end))
            {
                out.push(start, end, kind);
            }
        }
        out.finish()
    }

    fn lexical_state(&self, _source: &str, _cursor: usize) -> Lexical {
        Lexical::default()
    }

    fn config(&self) -> LanguageConfig {
        self.config
    }
}

/// A list of strings, or one string, as `name` (absent is empty).
fn string_list(value: &JsValue, name: &str) -> Result<Vec<String>, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(Vec::new());
    }
    if let Some(text) = value.as_string() {
        if text.is_empty() {
            return Err(JsError::new(&format!("{name} must be a non-empty string")));
        }
        return Ok(vec![text]);
    }
    if !Array::is_array(value) {
        return Err(JsError::new(&format!(
            "{name} must be a string or an array of strings"
        )));
    }
    Array::from(value)
        .iter()
        .map(|item| {
            item.as_string()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| JsError::new(&format!("{name} must hold non-empty strings")))
        })
        .collect()
}

/// The declarative part of a JavaScript language definition.
fn language_config(definition: &JsValue) -> Result<LanguageConfig, JsError> {
    let comments = field(definition, "comments");
    let (line_comments, block_comment) = if comments.is_undefined() || comments.is_null() {
        (Vec::new(), None)
    } else if !comments.is_object() || Array::is_array(&comments) {
        return Err(JsError::new(
            "comments must be an object: { lineComment?, blockComment? }",
        ));
    } else {
        warn_unknown_keys(
            &comments,
            &["lineComment", "blockComment"],
            "comments field",
        );
        let block = string_list(&field(&comments, "blockComment"), "comments.blockComment")?;
        let block = match block.as_slice() {
            [] => None,
            [open, close] => Some((leak(open.clone()), leak(close.clone()))),
            _ => return Err(JsError::new("comments.blockComment must be [open, close]")),
        };
        (
            string_list(&field(&comments, "lineComment"), "comments.lineComment")?,
            block,
        )
    };
    let separators_value = field(definition, "separators");
    if separators_value.is_string() {
        return Err(JsError::new(
            "separators must be an array of strings, e.g. [\"=\"]",
        ));
    }
    let separators = string_list(&separators_value, "separators")?;
    let brackets_value = field(definition, "brackets");
    let mut brackets = Vec::new();
    if !(brackets_value.is_undefined() || brackets_value.is_null()) {
        if !Array::is_array(&brackets_value) {
            return Err(JsError::new(
                "brackets must be an array of [open, close] pairs",
            ));
        }
        for pair in Array::from(&brackets_value).iter() {
            let single = |index: u32| {
                Array::is_array(&pair)
                    .then(|| Array::from(&pair).get(index).as_string())
                    .flatten()
                    .filter(|text| text.chars().count() == 1)
                    .and_then(|text| text.chars().next())
            };
            match (single(0), single(1)) {
                (Some(open), Some(close)) => brackets.push((open, close)),
                _ => {
                    return Err(JsError::new(
                        "brackets must be [open, close] pairs of single characters",
                    ));
                }
            }
        }
    }
    let leak_all = |items: Vec<String>| -> &'static [&'static str] {
        Box::leak(items.into_iter().map(leak).collect())
    };
    Ok(LanguageConfig {
        line_comments: leak_all(line_comments),
        block_comment,
        brackets: Box::leak(brackets.into_boxed_slice()),
        separators: leak_all(separators),
        quotes: &["\"", "'"],
    })
}

/// Registers a language implemented in JavaScript and returns its name for
/// `language` / `editAs` (its label, lowercased). Registering the same label
/// again replaces it for new editors.
#[cfg(not(target_feature = "atomics"))]
#[wasm_bindgen(js_name = registerLanguage)]
pub fn register_language(definition: CustomLanguage) -> Result<String, JsError> {
    let definition: JsValue = definition.into();
    let label = string_field(&definition, "label")
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .ok_or_else(|| JsError::new("a language needs a label"))?;
    if label.parse::<Language>().is_ok() {
        return Err(JsError::new(&format!("\"{label}\" is a built-in language")));
    }
    warn_unknown_keys(&definition, DEFINITION_KEYS, "language field");
    if field(&definition, "parse").dyn_into::<Function>().is_err() {
        return Err(JsError::new("a language needs parse(text)"));
    }
    for name in ["serialize", "keySpan", "valueSpan", "highlight"] {
        let value = field(&definition, name);
        if !value.is_undefined() && !value.is_null() && !value.is_function() {
            return Err(JsError::new(&format!("{name} must be a function")));
        }
    }
    let icon_value = field(&definition, "icon");
    let icon = if icon_value.is_undefined() || icon_value.is_null() {
        label.chars().next().map(String::from).unwrap_or_default()
    } else {
        match icon_value.as_string() {
            Some(icon)
                if (1..=2).contains(&icon.chars().count())
                    && icon.chars().all(char::is_alphanumeric) =>
            {
                icon
            }
            _ => return Err(JsError::new("icon must be one or two letters")),
        }
    };
    let extensions_value = field(&definition, "extensions");
    let extensions: Vec<&'static str> =
        if extensions_value.is_undefined() || extensions_value.is_null() {
            Vec::new()
        } else if Array::is_array(&extensions_value) {
            Array::from(&extensions_value)
                .iter()
                .map(|extension| {
                    // Written without the dot (`env`); a built-in's is taken.
                    let extension = extension
                        .as_string()
                        .map(|text| text.trim().trim_start_matches('.').to_lowercase())
                        .filter(|text| !text.is_empty())
                        .ok_or_else(|| JsError::new("extensions must hold non-empty strings"))?;
                    if extension.parse::<Language>().is_ok() {
                        return Err(JsError::new(&format!(
                            "extension \"{extension}\" already names a built-in language"
                        )));
                    }
                    Ok(leak(extension))
                })
                .collect::<Result<_, _>>()?
        } else {
            return Err(JsError::new(
                "extensions must be an array of strings, e.g. [\"env\"]",
            ));
        };
    let name = label.to_lowercase();
    // A name or extension another registered language already answers to
    // would be taken from it, or never reach this one.
    REGISTERED.with(|languages| {
        for (other, language) in languages
            .borrow()
            .iter()
            .filter(|(other, _)| **other != name)
        {
            if language.extensions().contains(&name.as_str()) {
                return Err(JsError::new(&format!(
                    "\"{name}\" is already an extension of the registered language \"{other}\""
                )));
            }
            if let Some(extension) = extensions.iter().find(|extension| {
                **extension == other.as_str() || language.extensions().contains(*extension)
            }) {
                return Err(JsError::new(&format!(
                    "extension \"{extension}\" already names the registered language \"{other}\""
                )));
            }
        }
        Ok(())
    })?;
    let config = language_config(&definition)?;
    let language: &'static JsLanguage = Box::leak(Box::new(JsLanguage {
        config,
        definition,
        info: LanguageInfo {
            label: leak(label),
            icon: leak(icon),
            extensions: Box::leak(extensions.into_boxed_slice()),
        },
    }));
    REGISTERED.with(|languages| {
        languages
            .borrow_mut()
            .insert(name.clone(), Language::new(language));
    });
    Ok(name)
}
