//! Reading the constructor's options, and warning about unknown keys.

use js_sys::{Function, Object};
use wasm_bindgen::prelude::*;

use crate::settings::setting_keys;
use crate::values::{field, optional_text};

/// The constructor's options apart from the settings (`EditorSettings`),
/// for the unknown-option warning.
pub(crate) const EDITOR_OPTIONS: &[&str] = &[
    "value",
    "language",
    "editAs",
    "schema",
    "baseline",
    "fileName",
    "readOnly",
    "readOnlyReason",
    "echoes",
    "className",
    "onChange",
    "onEditAsChange",
    "onSyncChange",
    "onChangeSummary",
    "onProblems",
    "onPathChange",
    "hostDiagnostics",
    "onSchemaWarning",
    "onSave",
    "onDirtyChange",
    "onBaselineConflict",
    "readOnlyPaths",
    "readOnlyRanges",
    "secrets",
];

/// What `update` takes besides the constructor's options: `rangesFor` (the
/// text the update's ranges were given for, when it is not the document).
pub(crate) const EDITOR_UPDATE_ONLY_OPTIONS: &[&str] = &["rangesFor"];

/// Logs a warning for each key of `options` that is neither one of `known`
/// nor a setting (`readonly` for `readOnly`), which would otherwise be
/// ignored without a word.
pub(crate) fn warn_unknown_options(options: &JsValue, known: &[&str]) {
    let settings = setting_keys();
    let mut names: Vec<&str> = known.to_vec();
    names.extend(settings.iter().map(String::as_str));
    warn_unknown_keys(options, &names, "option");
}

/// The options the editor is built from, which `update` refuses: changing
/// one needs a new `ConfigEditor`.
pub(crate) const EDITOR_REMOUNT_OPTIONS: &[&str] = &[
    "language",
    "schema",
    "fileName",
    "className",
    "secrets",
    "echoes",
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
