//! [`EditorSettings`] from JavaScript: the settings keys of an options
//! object read into the crate's struct through serde (camelCase keys,
//! missing ones taking their defaults), and back. The TypeScript type is
//! generated from the struct (`settings.d.ts`, see `tests/settings.rs`).

use config_editor_leptos::{EditorSettings, EditorStrings, ThemeColors};
use js_sys::{Object, Reflect};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::options::warn_unknown_keys;
use crate::typings::{SettingKeys, SettingsResult};
use crate::values::{field, to_js};

thread_local! {
    static KEYS: Vec<String> = keys_of(&EditorSettings::default());
    static STRING_KEYS: Vec<String> = keys_of(&EditorStrings::default());
}

/// The keys `value` serializes with, as JavaScript spells them.
fn keys_of(value: &impl Serialize) -> Vec<String> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::Object(fields)) => fields.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// The settings keys, as JavaScript spells them (`showFooter`).
pub(crate) fn setting_keys() -> Vec<String> {
    KEYS.with(Clone::clone)
}

/// The settings `object` holds (other keys are left alone); absent,
/// `undefined` and `null` take the defaults. A wrong value is an error
/// naming its key.
pub(crate) fn settings_from(object: &JsValue) -> Result<EditorSettings, JsError> {
    let given = Object::new();
    if object.is_object() {
        for key in setting_keys() {
            let value = field(object, &key);
            if !value.is_undefined() && !value.is_null() {
                let _ = Reflect::set(&given, &JsValue::from_str(&key), &value);
            }
        }
    }
    // serde-wasm-bindgen reads a map's entries with `Reflect`, which throws
    // its own error for anything else.
    let strings = field(&given, "strings");
    if !strings.is_undefined() {
        if !strings.is_object() || strings.is_function() || js_sys::Array::is_array(&strings) {
            return Err(JsError::new(
                "strings must be an object of text key: text, such as { noProblems: \"Keine Fehler\" }",
            ));
        }
        let known = STRING_KEYS.with(Clone::clone);
        let names: Vec<&str> = known.iter().map(String::as_str).collect();
        warn_unknown_keys(&strings, &names, "string");
    }
    let colors = field(&given, "colors");
    if !colors.is_undefined()
        && (!colors.is_object() || colors.is_function() || js_sys::Array::is_array(&colors))
    {
        return Err(JsError::new(
            "colors must be an object of token: CSS color, such as { key: \"#ffcc66\" }",
        ));
    }
    let deserializer = serde_wasm_bindgen::Deserializer::from(JsValue::from(given));
    let settings: EditorSettings =
        serde_path_to_error::deserialize(deserializer).map_err(|error| {
            JsError::new(&describe(
                &error.path().to_string(),
                &error.inner().to_string(),
            ))
        })?;
    check_colors(&settings.colors)?;
    Ok(settings)
}

/// A serde error as the other options word theirs: "showDiff must be a
/// boolean (got string "no")"; other errors keep their text after the key.
fn describe(key: &str, error: &str) -> String {
    // serde-wasm-bindgen's errors are JavaScript errors: "Error: …".
    let error = match error.split_once(": ") {
        Some((kind, rest)) if kind.ends_with("Error") => rest,
        _ => error,
    };
    let wrong_type = error
        .strip_prefix("invalid type: ")
        .and_then(|rest| rest.split_once(", expected "));
    match wrong_type {
        Some((got, expected)) => format!("{key} must be {expected} (got {got})"),
        None => format!("{key}: {error}"),
    }
}

/// `overrides` (a document's `settings`) over `base` (the view's): the keys
/// it sets replace the base's, and the texts of its `strings` the base's
/// texts.
pub(crate) fn settings_over(
    base: &EditorSettings,
    overrides: &JsValue,
) -> Result<EditorSettings, JsError> {
    let merged = settings_to_js(base);
    for key in setting_keys() {
        let value = field(overrides, &key);
        if value.is_undefined() || value.is_null() {
            continue;
        }
        let base_strings = field(&merged, "strings");
        let value = if key == "strings" && value.is_object() && base_strings.is_object() {
            let texts = Object::assign(&Object::new(), base_strings.unchecked_ref());
            Object::assign(&texts, value.unchecked_ref()).into()
        } else {
            value
        };
        let _ = Reflect::set(&merged, &JsValue::from_str(&key), &value);
    }
    settings_from(&merged)
}

/// Every color as the browser reads colors (`var(--x)` included): a typo
/// or a `url(…)` would otherwise be dropped, or fetched, silently.
fn check_colors(colors: &ThemeColors) -> Result<(), JsError> {
    for (color, css) in colors.iter() {
        if !web_sys::css::supports_with_value("color", css).unwrap_or(false) {
            return Err(JsError::new(&format!(
                "colors: color \"{}\": \"{css}\" is not a CSS color",
                color.token()
            )));
        }
    }
    Ok(())
}

/// The settings as a plain object, every key set.
pub(crate) fn settings_to_js(settings: &EditorSettings) -> JsValue {
    settings
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or_else(|_| Object::new().into())
}

/// The settings `object` sets: the typed result for a getter.
pub(crate) fn settings_result(settings: &EditorSettings) -> SettingsResult {
    settings_to_js(settings).unchecked_into()
}

/// The keys of `EditorSettings`, such as `"showFooter"`: what the editors'
/// options and `setSettings` read as settings.
#[wasm_bindgen(js_name = settingKeys)]
pub fn setting_keys_js() -> SettingKeys {
    to_js(&setting_keys()).unchecked_into()
}
