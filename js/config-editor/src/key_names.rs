//! Names for fields that name a key defined elsewhere (a string schema's
//! `x-config-editor-keys`), from and to JavaScript: the `keyNames` option
//! (an editor's, a documents view's), what `onGoToDefinition` is called
//! with, and the `keysAt` tool.

use js_sys::{Array, Object, Reflect};
use leptos_config_editor::{GoToDefinition, KeyName, KeyNames};
use wasm_bindgen::prelude::*;

use crate::languages::language_named;
use crate::options::warn_unknown_keys;
use crate::typings::KeysResult;
use crate::values::{field, optional_text, text_arg};
use leptos_config_editor::js_support::document;

/// Keys an entry of `keyNames` can hold.
const ENTRY_KEYS: &[&str] = &["schema", "pointer", "names"];

/// Keys a name given as an object can hold.
const NAME_KEYS: &[&str] = &["name", "source", "detail"];

/// The `keyNames` option (absent: none), named `name` in errors
/// (`keyNames`, `documents[1].keyNames`): `[{ schema?, pointer, names }]`,
/// each name a string or `{ name, source?, detail? }`.
pub(crate) fn key_names_from(value: &JsValue, name: &str) -> Result<Vec<KeyNames>, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(Vec::new());
    }
    if !Array::is_array(value) {
        return Err(JsError::new(&format!(
            "{name} must be an array of {{ schema?, pointer, names }}"
        )));
    }
    Array::from(value)
        .iter()
        .enumerate()
        .map(|(index, entry)| entry_from(&entry, &format!("{name}[{index}]")))
        .collect()
}

/// One entry of `keyNames`.
fn entry_from(entry: &JsValue, name: &str) -> Result<KeyNames, JsError> {
    if !entry.is_object() || Array::is_array(entry) {
        return Err(JsError::new(&format!(
            "{name} must be an object: {{ schema?, pointer, names }}"
        )));
    }
    warn_unknown_keys(entry, ENTRY_KEYS, "keyNames key");
    let pointer = text_arg(&field(entry, "pointer"), &format!("{name}.pointer"))?;
    if !pointer.is_empty() && !pointer.starts_with('/') {
        return Err(JsError::new(&format!(
            "{name}.pointer must be a JSON pointer (\"/clients\"), not {pointer:?}"
        )));
    }
    let schema = optional_text(&field(entry, "schema"), &format!("{name}.schema"))?;
    let list = field(entry, "names");
    if !Array::is_array(&list) {
        return Err(JsError::new(&format!(
            "{name}.names must be an array of names (strings or {{ name, source?, detail? }})"
        )));
    }
    let names = Array::from(&list)
        .iter()
        .enumerate()
        .map(|(index, item)| name_from(&item, &format!("{name}.names[{index}]")))
        .collect::<Result<Vec<_>, _>>()?;
    let names = KeyNames::new(pointer, names);
    Ok(match schema {
        Some(id) => names.for_schema(id),
        None => names,
    })
}

/// One name: a string, or `{ name, source?, detail? }`.
fn name_from(item: &JsValue, name: &str) -> Result<KeyName, JsError> {
    if let Some(text) = item.as_string() {
        return Ok(KeyName::new(text));
    }
    if !item.is_object() || Array::is_array(item) {
        return Err(JsError::new(&format!(
            "{name} must be a string or {{ name, source?, detail? }}"
        )));
    }
    warn_unknown_keys(item, NAME_KEYS, "keyNames name key");
    let mut key = KeyName::new(text_arg(&field(item, "name"), &format!("{name}.name"))?);
    if let Some(source) = optional_text(&field(item, "source"), &format!("{name}.source"))? {
        key = key.with_source(source);
    }
    if let Some(detail) = optional_text(&field(item, "detail"), &format!("{name}.detail"))? {
        key = key.with_detail(detail);
    }
    Ok(key)
}

/// What `onGoToDefinition` is called with: `{ schema, pointer, name,
/// source }`, `schema` and `source` `null` when not known.
pub(crate) fn go_to_definition_to_js(target: &GoToDefinition) -> JsValue {
    let object = Object::new();
    let text = |value: Option<&str>| value.map_or(JsValue::NULL, JsValue::from_str);
    for (key, value) in [
        ("schema", text(target.schema.as_deref())),
        ("pointer", JsValue::from_str(&target.pointer)),
        ("name", JsValue::from_str(&target.name)),
        ("source", text(target.source.as_deref())),
    ] {
        let _ = Reflect::set(&object, &JsValue::from_str(key), &value);
    }
    object.into()
}

/// The keys of the map at the JSON pointer `pointer` of `text`, read as
/// `language`, in the order written: the names to give in `keyNames` for
/// a file that is not open. `null` when the text does not parse (keep the
/// last good names); an empty list when no map is at `pointer`.
#[wasm_bindgen(js_name = keysAt)]
pub fn keys_at(
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] pointer: JsValue,
) -> Result<KeysResult, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let text = text_arg(&text, "text")?;
    let pointer = text_arg(&pointer, "pointer")?;
    let keys = leptos_config_editor::tools::keys_at(document(&text), language, &pointer);
    Ok(keys
        .map_or(JsValue::NULL, |keys| {
            keys.into_iter()
                .map(|key| JsValue::from_str(&key))
                .collect::<Array>()
                .into()
        })
        .unchecked_into())
}
