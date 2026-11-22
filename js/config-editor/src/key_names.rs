//! Names for fields that name a key defined elsewhere (a string schema's
//! `x-config-editor-keys`), from and to JavaScript: the `keyNames` option
//! (an editor's, a documents view's), what `onGoToDefinition` is called
//! with, and the `keysAt` tool.

use js_sys::{Array, Object, Reflect};
use leptos_config_editor::{FindUses, GoToDefinition, GoToUse, KeyName, KeyNames, Use};
use wasm_bindgen::prelude::*;

use crate::languages::language_named;
use crate::options::warn_unknown_keys;
use crate::typings::{FoundUsesResult, KeysResult};
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

/// Keys a use from `onFindUses` can hold (`range`, ignored: a `usesAt`
/// answer spread into a use as it is).
const USE_KEYS: &[&str] = &["source", "path", "line", "text", "version", "range"];

/// Asks `callback` (`onFindUses`) for the uses of `request`'s name in the
/// files that are not open: with `{ schema, pointer, name }`; it returns
/// the uses, or a promise of them. They join the list when they come; a
/// throw, a rejection or an answer that is no list of uses ends the
/// search with none (the console says why).
pub(crate) fn find_uses_from_host(callback: &js_sys::Function, request: FindUses) {
    let argument = Object::new();
    let text = |value: Option<&str>| value.map_or(JsValue::NULL, JsValue::from_str);
    for (key, value) in [
        ("schema", text(request.schema.as_deref())),
        ("pointer", JsValue::from_str(&request.pointer)),
        ("name", JsValue::from_str(&request.name)),
    ] {
        let _ = Reflect::set(&argument, &JsValue::from_str(key), &value);
    }
    let Some(answer) = crate::mount::ask_host(callback, &argument) else {
        request.reply(Vec::new());
        return;
    };
    // Nothing returned: the host searches no other files (a framework
    // binding forwarding a handler its user did not set).
    if answer.is_undefined() {
        leptos_config_editor::js_support::decline_find_uses(&request);
        return;
    }
    // One function for both outcomes: called once, then freed.
    let settle = Closure::once_into_js(move |found: JsValue| {
        request.reply(uses_from(&found).unwrap_or_else(|error| {
            web_sys::console::error_2(
                &JsValue::from_str(&format!("config-editor: onFindUses: {error}")),
                &found,
            );
            Vec::new()
        }));
    });
    let promise = js_sys::Promise::resolve(&answer);
    if let Ok(then) = Reflect::get(&promise, &JsValue::from_str("then")) {
        let then: &js_sys::Function = then.unchecked_ref();
        let _ = then.call2(&promise, &settle, &settle);
    }
}

/// The uses a host answered with: `[{ source, path, line?, text?,
/// version? }]` (`version` a string or a number, kept as text).
fn uses_from(found: &JsValue) -> Result<Vec<Use>, String> {
    if !Array::is_array(found) {
        return Err(
            "the answer must be an array of { source, path, line?, text?, version? }".into(),
        );
    }
    Array::from(found)
        .iter()
        .enumerate()
        .map(|(index, item)| use_from(&item).map_err(|error| format!("use {index}: {error}")))
        .collect()
}

fn use_from(item: &JsValue) -> Result<Use, String> {
    if !item.is_object() || Array::is_array(item) {
        return Err("must be { source, path, line?, text?, version? }".into());
    }
    warn_unknown_keys(item, USE_KEYS, "onFindUses use key");
    let source = field(item, "source")
        .as_string()
        .ok_or("source must be a string")?;
    let path = crate::values::path_from_js(&field(item, "path"))
        .map_err(|_| "path must be a JSON pointer or an array of keys and indices".to_string())?;
    let mut found = Use::new(source, path);
    if let Some(line) = field(item, "line")
        .as_f64()
        .filter(|line| *line >= 1.0 && line.fract() == 0.0 && line.is_finite())
    {
        found = found.with_line(line as usize);
    }
    if let Some(text) = field(item, "text").as_string() {
        found = found.with_text(text);
    }
    let version = field(item, "version");
    if let Some(version) = version
        .as_string()
        .or_else(|| version.as_f64().map(|number| number.to_string()))
    {
        found = found.with_version(version);
    }
    Ok(found)
}

/// What `onGoToUse` is called with: `{ source, path, name }`, `path` a
/// JSON pointer.
pub(crate) fn go_to_use_to_js(target: &GoToUse) -> JsValue {
    let object = Object::new();
    for (key, value) in [
        ("source", target.source.as_str()),
        ("path", &target.path.to_pointer()),
        ("name", target.name.as_str()),
    ] {
        let _ = Reflect::set(&object, &JsValue::from_str(key), &JsValue::from_str(value));
    }
    object.into()
}

/// The uses of `name` in `text` (read as `language`, its schema `schema`,
/// JSON Schema text): every value, and every key an object's
/// `propertyNames` reads, whose `x-config-editor-keys` names the map at
/// `pointer` and that is `name`, in the order written, as
/// `[{ path, line, text, range }]` (`path` a JSON pointer, `line`
/// one-based). For answering `onFindUses` about a file that is not open.
/// `of` is the `$id` the request names (its `schema`); left out, the
/// schema's own. `null` when the text does not parse.
#[wasm_bindgen(js_name = usesAt)]
pub fn uses_at(
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] schema: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] pointer: JsValue,
    #[wasm_bindgen(unchecked_param_type = "string")] name: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "string | null")] of: JsValue,
) -> Result<FoundUsesResult, JsError> {
    let language = language_named(&text_arg(&language, "language")?)?;
    let text = text_arg(&text, "text")?;
    let schema = crate::headless::schema_from(&text_arg(&schema, "schema")?, &JsValue::UNDEFINED)?;
    let pointer = text_arg(&pointer, "pointer")?;
    let name = text_arg(&name, "name")?;
    let of = optional_text(&of, "of")?;
    let text = document(&text);
    let found = leptos_config_editor::tools::uses_at(
        text,
        language,
        &schema,
        &pointer,
        &name,
        of.as_deref(),
    );
    Ok(found
        .map_or(JsValue::NULL, |found| {
            found
                .into_iter()
                .map(|found| {
                    let object = Object::new();
                    let range = Object::new();
                    for (key, position) in [("start", found.range.start), ("end", found.range.end)]
                    {
                        let place = Object::new();
                        let _ =
                            Reflect::set(&place, &"line".into(), &(position.line as f64).into());
                        let _ = Reflect::set(
                            &place,
                            &"column".into(),
                            &(position.column as f64).into(),
                        );
                        let _ = Reflect::set(&range, &key.into(), &place);
                    }
                    for (key, value) in [
                        ("path", JsValue::from_str(&found.path.to_pointer())),
                        ("line", JsValue::from_f64(found.line as f64)),
                        ("text", JsValue::from_str(&found.text)),
                        ("range", range.into()),
                    ] {
                        let _ = Reflect::set(&object, &JsValue::from_str(key), &value);
                    }
                    JsValue::from(object)
                })
                .collect::<Array>()
                .into()
        })
        .unchecked_into())
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
