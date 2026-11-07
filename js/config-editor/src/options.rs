//! Reading the constructor's options, and warning about unknown keys.

use js_sys::{Function, Object};
use wasm_bindgen::prelude::*;

use crate::values::{field, optional_text};

/// The constructor's options, for the unknown-option warning.
pub(crate) const OPTIONS: [&str; 33] = [
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
    "folding",
    "showDiff",
    "showDiagnostics",
    "completions",
    "autoTriggerCompletions",
    "hoverDocs",
    "highlighting",
    "autoClosing",
    "styled",
    "saveShortcut",
    "className",
    "onChange",
    "onEditAsChange",
    "onChangeSummary",
    "onPathChange",
    "hostDiagnostics",
    "onSchemaWarning",
    "onSave",
    "onDirtyChange",
    "onBaselineConflict",
];

/// Logs a warning for each option the editor does not know (`readonly` for
/// `readOnly`), which would otherwise be ignored without a word.
pub(crate) fn warn_unknown_options(options: &JsValue) {
    warn_unknown_keys(options, &OPTIONS, "option");
}

/// Keys `setCallbacks` takes.
pub(crate) const CALLBACK_KEYS: &[&str] = &[
    "onChange",
    "onEditAsChange",
    "onChangeSummary",
    "onPathChange",
    "onSave",
    "onDirtyChange",
    "onBaselineConflict",
];

/// A console warning for each key of `object` not in `known`, with the
/// closest known one as a hint (`valu` → `value`).
pub(crate) fn warn_unknown_keys(object: &JsValue, known: &[&str], what: &str) {
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
pub(crate) fn edit_distance(left: &str, right: &str) -> usize {
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
pub(crate) fn string_option(options: &JsValue, name: &str) -> Result<Option<String>, JsError> {
    optional_text(&field(options, name), name)
}

/// An optional boolean option: absent, or a boolean.
pub(crate) fn bool_option(options: &JsValue, name: &str) -> Result<Option<bool>, JsError> {
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
pub(crate) fn function_option(options: &JsValue, name: &str) -> Result<Option<Function>, JsError> {
    let value = field(options, name);
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    value
        .dyn_into::<Function>()
        .map(Some)
        .map_err(|_| JsError::new(&format!("{name} must be a function")))
}

/// The constructor's on/off options, with the component's defaults.
#[derive(Clone, Copy)]
pub(crate) struct Toggles {
    pub(crate) show_toolbar: bool,
    pub(crate) show_footer: bool,
    pub(crate) search: bool,
    pub(crate) go_to_line: bool,
    pub(crate) folding: bool,
    pub(crate) show_diff: bool,
    pub(crate) show_diagnostics: bool,
    pub(crate) completions: bool,
    pub(crate) auto_trigger_completions: bool,
    pub(crate) hover_docs: bool,
    pub(crate) highlighting: bool,
    pub(crate) auto_closing: bool,
    pub(crate) styled: bool,
}

impl Toggles {
    pub(crate) fn read(options: &JsValue) -> Result<Self, JsError> {
        let flag = |name: &str, default: bool| {
            Ok::<_, JsError>(bool_option(options, name)?.unwrap_or(default))
        };
        Ok(Self {
            show_toolbar: flag("showToolbar", false)?,
            show_footer: flag("showFooter", false)?,
            search: flag("search", true)?,
            go_to_line: flag("goToLine", true)?,
            folding: flag("folding", true)?,
            show_diff: flag("showDiff", true)?,
            show_diagnostics: flag("showDiagnostics", true)?,
            completions: flag("completions", true)?,
            auto_trigger_completions: flag("autoTriggerCompletions", true)?,
            hover_docs: flag("hoverDocs", true)?,
            highlighting: flag("highlighting", true)?,
            auto_closing: flag("autoClosing", true)?,
            styled: flag("styled", true)?,
        })
    }
}
