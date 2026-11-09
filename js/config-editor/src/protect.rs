//! Protected fields from JavaScript: `readOnlyPaths` / `readOnlyRanges`
//! (options and `update` keys) and the `secrets` option.

use config_editor_leptos::{FieldPath, SecretOptions, TextRange};
use js_sys::Array;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::options::{function_option, warn_unknown_keys};
use crate::text_lines::TextLines;
use crate::values::{field, optional_text, path_from_js, range_from_js};

/// Keys `secrets` can hold.
const SECRET_KEYS: &[&str] = &["placeholder", "isPlaceholder", "hint", "referencePrefixes"];

/// An optional list of paths (`readOnlyPaths`).
pub(crate) fn paths_from(list: &JsValue, name: &str) -> Result<Vec<FieldPath>, JsError> {
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
        .enumerate()
        .map(|(index, path)| {
            path_from_js(&path).map_err(|error| {
                let message = js_sys::Error::from(JsValue::from(error)).message();
                JsError::new(&format!("{name}[{index}]: {message}"))
            })
        })
        .collect()
}

/// An optional list of ranges of `host_text` (`readOnlyRanges`).
pub(crate) fn ranges_from(
    list: &JsValue,
    host_text: &str,
    name: &str,
) -> Result<Vec<TextRange>, JsError> {
    if list.is_undefined() || list.is_null() {
        return Ok(Vec::new());
    }
    if !Array::is_array(list) {
        return Err(JsError::new(&format!("{name} must be an array of ranges")));
    }
    let host_text = TextLines::new(host_text);
    Array::from(list)
        .iter()
        .enumerate()
        .map(|(index, range)| {
            range_from_js(&range, &host_text)
                .map_err(|error| JsError::new(&format!("{name}[{index}]: {error}")))
        })
        .collect()
}

/// The `secrets` option (absent: the defaults), named `name` in errors
/// (`secrets`, `documents[1].secrets`). Call inside the editor's owner: a
/// host check is kept there.
pub(crate) fn secrets_from(value: &JsValue, name: &str) -> Result<SecretOptions, JsError> {
    let mut options = SecretOptions::default();
    if value.is_undefined() || value.is_null() {
        return Ok(options);
    }
    if !value.is_object() || Array::is_array(value) {
        return Err(JsError::new(&format!(
            "{name} must be an object: {{ placeholder?, isPlaceholder?, hint?, referencePrefixes? }}"
        )));
    }
    warn_unknown_keys(value, SECRET_KEYS, "secrets key");
    if let Some(pattern) =
        optional_text(&field(value, "placeholder"), &format!("{name}.placeholder"))?
    {
        options = options.with_placeholder(pattern);
        config_editor_leptos::js_support::check_placeholder(&options)
            .map_err(|error| JsError::new(&format!("{name}.placeholder: {error}")))?;
    }
    if let Some(hint) = optional_text(&field(value, "hint"), &format!("{name}.hint"))? {
        options = options.with_hint(hint);
    }
    let prefixes = field(value, "referencePrefixes");
    if !prefixes.is_undefined() && !prefixes.is_null() {
        let invalid = || {
            JsError::new(&format!(
                "{name}.referencePrefixes must be an array of non-empty strings"
            ))
        };
        let list = Array::is_array(&prefixes)
            .then(|| Array::from(&prefixes))
            .ok_or_else(invalid)?;
        let prefixes: Option<Vec<String>> = list
            .iter()
            .map(|prefix| prefix.as_string().filter(|prefix| !prefix.is_empty()))
            .collect();
        options = options.with_reference_prefixes(prefixes.ok_or_else(invalid)?);
    }
    if let Some(check) = function_option(value, "isPlaceholder")
        .map_err(|_| JsError::new(&format!("{name}.isPlaceholder must be a function")))?
    {
        let check = StoredValue::new_local(Some(check));
        options = options.with_matcher(move |token: String| {
            check
                .try_with_value(|check| {
                    check.as_ref().is_some_and(|check| {
                        match check.call1(&JsValue::NULL, &JsValue::from_str(&token)) {
                            Ok(real) => real.is_truthy(),
                            // Logged as every callback's exception is.
                            Err(error) => {
                                web_sys::console::error_2(
                                    &JsValue::from_str("config-editor: a callback threw"),
                                    &error,
                                );
                                false
                            }
                        }
                    })
                })
                .unwrap_or(false)
        });
    }
    Ok(options)
}
