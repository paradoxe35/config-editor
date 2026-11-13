//! What `update` on both classes shares: reading a partial options object.
//! A key left out or set to `undefined` stays as it is; `null` removes a
//! callback, clears a list or resets a value; settings merge key by key
//! (and `strings` text by text); a key the component is built from is
//! refused.

use config_editor_leptos::EditorSettings;
use js_sys::{Array, Function, Reflect};
use wasm_bindgen::prelude::*;

use crate::settings::{is_record, merged_texts, setting_keys, settings_from, settings_to_js};
use crate::values::field;

/// `options` is a plain object, else `usage` is the error.
pub(crate) fn plain_object(options: &JsValue, usage: &str) -> Result<(), JsError> {
    if options.is_object() && !options.is_function() && !Array::is_array(options) {
        Ok(())
    } else {
        Err(JsError::new(usage))
    }
}

/// The value of `name` when `options` sets it (`undefined` sets nothing).
pub(crate) fn given(options: &JsValue, name: &str) -> Option<JsValue> {
    Some(field(options, name)).filter(|value| !value.is_undefined())
}

/// An option a view cannot change, and the values that are its default:
/// left out or `null`, the view is built with the default, and any of these
/// names it too (`echoes: false`, `className: ""`).
pub(crate) struct Fixed {
    pub(crate) key: &'static str,
    pub(crate) defaults: Vec<JsValue>,
}

impl Fixed {
    /// `key`, whose default only `null` (or leaving it out) gives.
    pub(crate) fn plain(key: &'static str) -> Self {
        Self::or(key, Vec::new())
    }

    pub(crate) fn or(key: &'static str, defaults: Vec<JsValue>) -> Self {
        Self { key, defaults }
    }

    /// `value` as built: `None` for the default.
    fn built(&self, value: &JsValue) -> Option<JsValue> {
        let default = value.is_undefined()
            || value.is_null()
            || self
                .defaults
                .iter()
                .any(|default| js_sys::Object::is(value, default));
        (!default).then(|| value.clone())
    }
}

/// What a view was built from: each option it cannot change, as it was
/// given (`None`: its default).
pub(crate) struct BuiltFrom(Vec<(Fixed, Option<JsValue>)>);

impl BuiltFrom {
    pub(crate) fn of(options: &JsValue, fixed: Vec<Fixed>) -> Self {
        Self(
            fixed
                .into_iter()
                .map(|fixed| {
                    let built = fixed.built(&field(options, fixed.key));
                    (fixed, built)
                })
                .collect(),
        )
    }

    /// Whether `options` gives `key` another value than the view was built
    /// with: the same value (`Object.is`: a framework passing all its props
    /// again), or the default for one built with it (left out), changes
    /// nothing.
    fn changes(&self, options: &JsValue, key: &str) -> bool {
        given(options, key).is_some_and(|value| {
            !self.0.iter().any(|(fixed, was)| {
                fixed.key == key
                    && match (fixed.built(&value), was) {
                        (None, None) => true,
                        (Some(value), Some(was)) => js_sys::Object::is(&value, was),
                        _ => false,
                    }
            })
        })
    }
}

/// Refuses an `update` that changes one of `keys`, which `class` is built
/// from (the same value given again changes nothing).
pub(crate) fn refuse_remount(
    options: &JsValue,
    keys: &[&str],
    built: &BuiltFrom,
    class: &str,
) -> Result<(), JsError> {
    match keys.iter().find(|key| built.changes(options, key)) {
        Some(key) => Err(JsError::new(&format!(
            "{key} cannot change on a mounted {class}: it is built from it. \
             Create a new {class} (destroy() this one) to change {key}"
        ))),
        None => Ok(()),
    }
}

/// A callback change: `None` keeps it, `Some(None)` removes it (`null`),
/// `Some(Some(f))` replaces it.
pub(crate) fn callback_change(
    options: &JsValue,
    name: &str,
) -> Result<Option<Option<Function>>, JsError> {
    match given(options, name) {
        None => Ok(None),
        Some(value) if value.is_null() => Ok(Some(None)),
        Some(value) => value
            .dyn_into::<Function>()
            .map(|callback| Some(Some(callback)))
            .map_err(|_| JsError::new(&format!("{name} must be a function or null"))),
    }
}

/// A boolean change: `null` resets it to `false`.
pub(crate) fn flag_change(options: &JsValue, name: &str) -> Result<Option<bool>, JsError> {
    match given(options, name) {
        None => Ok(None),
        Some(value) if value.is_null() => Ok(Some(false)),
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| JsError::new(&format!("{name} must be a boolean or null"))),
    }
}

/// The settings `options` changes, laid over `current`: `None` when it
/// sets none. A setting set to `null` returns to its default; `strings`
/// merges text by text and `colors` token by token (one set to `null`
/// returns to its default).
pub(crate) fn settings_change(
    current: &EditorSettings,
    options: &JsValue,
) -> Result<Option<EditorSettings>, JsError> {
    let keys = setting_keys();
    // Most updates set no setting (a host's list after each keystroke):
    // the current settings are written out only to merge one into.
    if !keys.iter().any(|key| given(options, key).is_some()) {
        return Ok(None);
    }
    let merged = settings_to_js(current);
    for key in keys {
        let Some(value) = given(options, &key) else {
            continue;
        };
        let value = if (key == "strings" || key == "colors") && is_record(&value) {
            merged_texts(&field(&merged, &key), &value)
        } else {
            value
        };
        let _ = Reflect::set(&merged, &JsValue::from_str(&key), &value);
    }
    settings_from(&merged).map(Some)
}
