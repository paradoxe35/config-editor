//! What `update` on both classes shares: reading a partial options object.
//! A key left out or set to `undefined` stays as it is; `null` removes a
//! callback, clears a list or resets a value; settings merge key by key
//! (and `strings` text by text); a key the component is built from is
//! refused.

use config_editor_leptos::EditorSettings;
use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::prelude::*;

use crate::settings::{setting_keys, settings_from, settings_to_js};
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

/// An error for the first of `keys` that `options` sets: the component is
/// built from them, so changing one needs a new instance.
pub(crate) fn refuse_remount(options: &JsValue, keys: &[&str], class: &str) -> Result<(), JsError> {
    match keys.iter().find(|key| given(options, key).is_some()) {
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
    let merged = settings_to_js(current);
    let mut changed = false;
    for key in setting_keys() {
        let Some(value) = given(options, &key) else {
            continue;
        };
        changed = true;
        let value = if (key == "strings" || key == "colors") && is_record(&value) {
            merged_texts(&field(&merged, &key), &value)
        } else {
            value
        };
        let _ = Reflect::set(&merged, &JsValue::from_str(&key), &value);
    }
    if !changed {
        return Ok(None);
    }
    settings_from(&merged).map(Some)
}

fn is_record(value: &JsValue) -> bool {
    value.is_object() && !value.is_function() && !Array::is_array(value)
}

/// `texts` over `current` (both `strings` or both `colors` objects); a
/// text set to `null` is dropped, so it takes its default.
fn merged_texts(current: &JsValue, texts: &JsValue) -> JsValue {
    let merged = if is_record(current) {
        Object::assign(&Object::new(), current.unchecked_ref())
    } else {
        Object::new()
    };
    let merged = Object::assign(&merged, texts.unchecked_ref());
    for key in Object::keys(&merged).iter() {
        let value = Reflect::get(&merged, &key).unwrap_or(JsValue::UNDEFINED);
        if value.is_null() || value.is_undefined() {
            let _ = Reflect::delete_property(&merged, &key);
        }
    }
    merged.into()
}
