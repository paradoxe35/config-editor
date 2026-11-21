//! Protected fields from JavaScript: `readOnlyPaths` / `readOnlyRanges`
//! (options and `update` keys) and the `secrets` option.

use js_sys::Array;
use leptos::prelude::*;
use leptos_config_editor::{FieldPath, SecretOptions, TextRange};
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

/// An optional list of ranges given with `host_text`, read in it; but read
/// in the first of `before` (the texts the last ranges came with, see
/// `HostLocks::came_with`) they all fit, and moved from it to `host_text`,
/// when they name places `host_text` lacks (a line or index past its end,
/// a column past its line's end): a framework's render lists a newer text
/// with ranges a host worked out on the text it rendered before (read in
/// the newer text, a lock fell on the line above its field, or the list
/// was refused). The text from before a reset (`before[1]`) is taken only
/// for ranges past `host_text`'s end: a column past a line's end there is
/// read in `host_text`, where the host gave it.
pub(crate) fn ranges_in(
    list: &JsValue,
    host_text: &str,
    before: &[&str],
    name: &str,
) -> Result<Vec<TextRange>, JsError> {
    let fitted = || {
        before
            .iter()
            .enumerate()
            .find(|(at, before)| {
                **before != host_text
                    && (*at == 0 || past_end(list, host_text))
                    && fits(list, before)
            })
            .map(|(_, before)| before)
    };
    if !fits(list, host_text)
        && let Some(before) = fitted()
    {
        let ranges = ranges_from(list, before, name)?;
        return Ok(
            crate::remap::moved_between(before, host_text, Some(ranges), None)
                .0
                .unwrap_or_default(),
        );
    }
    ranges_from(list, host_text, name)
}

/// Whether every range of `list` names a place in `text`: no line, column
/// or index past its end (a list that is no array of ranges fits nowhere).
fn fits(list: &JsValue, text: &str) -> bool {
    if list.is_undefined() || list.is_null() {
        return true;
    }
    if !Array::is_array(list) {
        return false;
    }
    let lines = TextLines::new(text);
    let number = |range: &JsValue, name: &str| field(range, name).as_f64().map(|n| n as usize);
    Array::from(list).iter().all(|range| {
        let index = number(&range, "end").is_none_or(|end| end <= lines.units())
            && number(&range, "start").is_none_or(|start| start <= lines.units());
        let line = |line_key: &str, column_key: &str| match number(&range, line_key) {
            None => true,
            Some(line) => {
                line >= 1
                    && line <= lines.count()
                    && number(&range, column_key)
                        .is_none_or(|column| column <= lines.width(line) + 1)
            }
        };
        let end_line = if field(&range, "endLine").is_undefined() {
            "startLine"
        } else {
            "endLine"
        };
        index && line("startLine", "startColumn") && line(end_line, "endColumn")
    })
}

/// Whether a range of `list` names a line or index past `text`'s end.
fn past_end(list: &JsValue, text: &str) -> bool {
    if !Array::is_array(list) {
        return false;
    }
    let lines = TextLines::new(text);
    let number = |range: &JsValue, name: &str| field(range, name).as_f64().map(|n| n as usize);
    Array::from(list).iter().any(|range| {
        ["start", "end"]
            .iter()
            .any(|key| number(&range, key).is_some_and(|at| at > lines.units()))
            || ["startLine", "endLine"]
                .iter()
                .any(|key| number(&range, key).is_some_and(|line| line > lines.count()))
    })
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
        leptos_config_editor::js_support::check_placeholder(&options)
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
