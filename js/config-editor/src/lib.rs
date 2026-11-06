//! Browser bindings for `config-editor-leptos`, published to npm as
//! `config-editor`. The editor itself is the crate's Leptos component, mounted
//! into any element; JavaScript sees a plain class and functions. Framework
//! wrappers (`config-editor-react`, …) build on this package.
#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::HashMap;

use config_editor_leptos::language::{
    ConfigLanguage, LanguageConfig, LanguageInfo, Lexical, ParseError, Span, Token, TokenKind,
    Tokens,
};
use config_editor_leptos::{
    ConfigEditor as Editor, EditorTheme, Language, Schema, SchemaSource, ThemeColor, ThemeColors,
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
  /** Extra class on the editor root. */
  className?: string;
  /** Called with the document, in `language`, after every edit. */
  onChange?: (value: string) => void;
  /** Called when an `editAs` switch completes (it waits while the document cannot convert yet). */
  onEditAsChange?: (language: LanguageName) => void;
}

/** Callbacks to replace: a function replaces, `null` removes, absent keeps. */
export interface CallbackUpdate {
  onChange?: ((value: string) => void) | null;
  onEditAsChange?: ((language: LanguageName) => void) | null;
}

export interface Diagnostic {
  line?: number;
  column?: number;
  endColumn?: number;
  message: string;
  severity: "error" | "warning";
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
const OPTIONS: [&str; 14] = [
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
    "className",
    "onChange",
    "onEditAsChange",
];

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
const CALLBACK_KEYS: &[&str] = &["onChange", "onEditAsChange"];

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
        let on_change = function_option(&options, "onChange")?;
        let on_edit_as_change = function_option(&options, "onEditAsChange")?;
        warn_unknown_options(&options);
        // The editor replaces whatever the element held, editors included.
        replace_editors_in(&element);
        element.set_text_content(None);

        let latest: Latest = std::sync::Arc::new(std::sync::Mutex::new(value.clone()));
        let latest_for_view = latest.clone();
        let owner = Owner::new();
        let state = owner.with(|| State {
            value: RwSignal::new(value.clone()),
            baseline: RwSignal::new(baseline),
            edit_as: RwSignal::new(edit_as),
            theme: RwSignal::new(theme),
            colors: RwSignal::new(colors),
            read_only: RwSignal::new(read_only),
            on_change: StoredValue::new_local(on_change),
            on_edit_as_change: StoredValue::new_local(on_edit_as_change),
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
                            on_change=changed
                            on_edit_as_change=switched
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
                            on_change=changed
                            on_edit_as_change=switched
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
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
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
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
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
        if let Some(callback) = on_change {
            let _ = self.state.on_change.try_set_value(callback);
        }
        if let Some(callback) = on_edit_as_change {
            let _ = self.state.on_edit_as_change.try_set_value(callback);
        }
        Ok(())
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
    line: Option<usize>,
    column: Option<usize>,
    end_column: Option<usize>,
    message: String,
    severity: &'static str,
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
            severity: match problem.severity {
                config_editor_leptos::Severity::Warning => "warning",
                _ => "error",
            },
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
    let separators = string_list(&field(definition, "separators"), "separators")?;
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
