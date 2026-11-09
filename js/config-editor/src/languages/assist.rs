//! Schema completion and hover for a JavaScript language that places the
//! caret itself: its `contextAt`, `insertKey` and `keyAt` callbacks, read
//! into the [`ConfigLanguage`] hooks the built-in languages implement, so the
//! same engine suggests and documents. A result of the wrong shape is warned
//! about and read as `null`.

use std::collections::HashSet;

use config_editor_leptos::language::{
    CompletionContext, ConfigLanguage, InsertContext, SchemaPathSegment, array_branch,
    object_branch, utf16_to_byte_index,
};
use js_sys::Array;
use serde_json::Value;
use wasm_bindgen::prelude::*;

use super::js_language::JsLanguage;
use crate::values::field;

const CONTEXT_SHAPE: &str =
    "{ path: (string | number)[], at: \"key\" | \"value\", prefix: string }";
const INSERT_SHAPE: &str = "{ text: string, caret: number }";
const KEY_SHAPE: &str = "{ key: string, path: (string | number)[], start: number }";

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `value` as a whole number at least 0.
fn index(value: &JsValue) -> Option<usize> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && number.fract() == 0.0 && *number >= 0.0)
        .map(|number| number as usize)
}

/// A path of keys (strings) and array indices (whole numbers).
fn path(value: &JsValue) -> Option<Vec<SchemaPathSegment>> {
    if !Array::is_array(value) {
        return None;
    }
    Array::from(value)
        .iter()
        .map(|segment| match segment.as_string() {
            Some(key) => Some(SchemaPathSegment::Key(key)),
            None => index(&segment).map(SchemaPathSegment::ArrayItem),
        })
        .collect()
}

/// `null` / `undefined` as `None`, an object as itself, anything else as a
/// wrong shape.
fn object(value: &JsValue) -> Result<Option<&JsValue>, ()> {
    if value.is_null() || value.is_undefined() {
        Ok(None)
    } else if value.is_object() && !Array::is_array(value) {
        Ok(Some(value))
    } else {
        Err(())
    }
}

/// [`ConfigLanguage::completion_context`] from `contextAt(text, offset)`;
/// `null` (or a wrong shape) offers nothing.
pub(super) fn context_at(language: &JsLanguage, source: &str, cursor: usize) -> CompletionContext {
    let nothing = CompletionContext {
        no_properties: true,
        replace_start: cursor,
        replace_end: cursor,
        ..CompletionContext::default()
    };
    let Some(result) = language.call(
        "contextAt",
        &[JsValue::from_str(source), JsValue::from_f64(cursor as f64)],
    ) else {
        return nothing;
    };
    match read_context(language, &result, source, cursor) {
        Ok(Some(context)) => context,
        Ok(None) => nothing,
        Err(problem) => {
            language.warn("contextAt", &problem);
            nothing
        }
    }
}

fn read_context(
    language: &JsLanguage,
    result: &JsValue,
    source: &str,
    cursor: usize,
) -> Result<Option<CompletionContext>, String> {
    let wrong = || format!("must return {CONTEXT_SHAPE} or null");
    let Some(result) = object(result).map_err(|()| wrong())? else {
        return Ok(None);
    };
    let mut path = path(&field(result, "path")).ok_or_else(wrong)?;
    let at = field(result, "at").as_string();
    let prefix = field(result, "prefix");
    let prefix = if prefix.is_undefined() {
        String::new()
    } else {
        prefix.as_string().ok_or_else(wrong)?
    };
    let cursor = cursor.min(units(source));
    let before = &source[..utf16_to_byte_index(source, cursor)];
    if !before.ends_with(&prefix) {
        return Err(format!(
            "returned a prefix ({prefix:?}) that is not the text before the offset"
        ));
    }
    let line = &before[before.rfind('\n').map_or(0, |at| at + 1)..];
    let mut context = CompletionContext {
        typed: prefix.clone(),
        replace_start: cursor - units(&prefix),
        replace_end: cursor,
        base_indent: line[..line.len() - line.trim_start().len()].to_string(),
        ..CompletionContext::default()
    };
    match at.as_deref() {
        Some("key") => {}
        Some("value") => match path.last().cloned() {
            Some(SchemaPathSegment::Key(key)) => {
                path.pop();
                context.value_key = Some(key);
                // `key:|` takes a space before the value, as `key: ` is written.
                context.value_needs_space = prefix.is_empty()
                    && language.config().separators.contains(&":")
                    && before.ends_with(':');
            }
            Some(SchemaPathSegment::ArrayItem(_)) => context.is_array_item = true,
            None => return Err("returned at: \"value\" with an empty path".to_string()),
        },
        _ => return Err(wrong()),
    }
    context.path = path;
    Ok(Some(context))
}

/// [`ConfigLanguage::existing_keys`] for a language with `contextAt`: the
/// keys of the object at `path` in the document parsed without the caret's
/// line (so a half-typed key hides nothing). `None` while that does not
/// parse.
pub(super) fn parsed_keys(
    language: &JsLanguage,
    source: &str,
    cursor_byte: usize,
    path: &[SchemaPathSegment],
) -> Option<HashSet<String>> {
    let cursor_byte = cursor_byte.min(source.len());
    let line_start = source[..cursor_byte].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[cursor_byte..]
        .find('\n')
        .map_or(source.len(), |at| cursor_byte + at);
    let without = format!("{}{}", &source[..line_start], &source[line_end..]);
    let document = language.parse(&without).ok()?;
    let mut value = &document;
    for segment in path {
        value = match segment {
            SchemaPathSegment::Key(key) => value.get(key)?,
            SchemaPathSegment::ArrayItem(index) => value.get(index)?,
        };
    }
    Some(value.as_object()?.keys().cloned().collect())
}

/// [`ConfigLanguage::property_insert`] from `insertKey(key, kind)`; `null`
/// leaves the key out. Lines after the first are indented as the caret's
/// line.
/// Whether UTF-16 `caret` falls between the two halves of a character of
/// `text` (a surrogate pair).
fn splits_character(text: &str, caret: usize) -> bool {
    text.encode_utf16()
        .nth(caret.wrapping_sub(1))
        .is_some_and(|unit| (0xD800..0xDC00).contains(&unit))
        && caret < units(text)
}

pub(super) fn insert_key(
    language: &JsLanguage,
    key: &str,
    property: &Value,
    root: &Value,
    context: &InsertContext<'_>,
) -> Option<(String, usize)> {
    let kind = if object_branch(property, root).is_some() {
        "object"
    } else if array_branch(property, root).is_some() {
        "array"
    } else {
        "scalar"
    };
    let result = language.call(
        "insertKey",
        &[JsValue::from_str(key), JsValue::from_str(kind)],
    )?;
    let read = || -> Result<Option<(String, usize)>, ()> {
        let Some(result) = object(&result)? else {
            return Ok(None);
        };
        let text = field(result, "text").as_string().ok_or(())?;
        let caret = index(&field(result, "caret"))
            .filter(|caret| *caret <= units(&text) && !splits_character(&text, *caret))
            .ok_or(())?;
        Ok(Some((text, caret)))
    };
    let (text, caret) = match read() {
        Ok(found) => found?,
        Err(()) => {
            language.warn(
                "insertKey",
                &format!(
                    "must return {INSERT_SHAPE} (caret within text, between characters) or null"
                ),
            );
            return None;
        }
    };
    let caret_byte = utf16_to_byte_index(&text, caret);
    let indent = |part: &str| {
        let mut out = String::with_capacity(part.len());
        let mut lines = part.split('\n');
        out.push_str(lines.next().unwrap_or_default());
        for line in lines {
            out.push('\n');
            if !line.is_empty() {
                out.push_str(context.base_indent);
            }
            out.push_str(line);
        }
        out
    };
    let before = indent(&text[..caret_byte]);
    let after = indent(&text[caret_byte..]);
    let caret = units(&before);
    Some((format!("{before}{after}"), caret))
}

/// [`ConfigLanguage::hover_path`] from `keyAt(text, offset)`.
pub(super) fn key_at(
    language: &JsLanguage,
    source: &str,
    line_index: usize,
    column: usize,
) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
    let line_start: usize = source
        .split('\n')
        .take(line_index)
        .map(|line| line.len() + 1)
        .sum();
    let line = source.get(line_start..)?.split('\n').next()?;
    let column_byte = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(at, _)| at);
    let offset = units(&source[..line_start + column_byte]);
    let result = language.call(
        "keyAt",
        &[JsValue::from_str(source), JsValue::from_f64(offset as f64)],
    )?;
    let read = || -> Result<Option<(Vec<SchemaPathSegment>, String, usize)>, String> {
        let wrong = || format!("must return {KEY_SHAPE} or null");
        let Some(result) = object(&result).map_err(|()| wrong())? else {
            return Ok(None);
        };
        let key = field(result, "key")
            .as_string()
            .filter(|key| !key.is_empty())
            .ok_or_else(wrong)?;
        let path = path(&field(result, "path")).ok_or_else(wrong)?;
        if path.last() != Some(&SchemaPathSegment::Key(key.clone())) {
            return Err("returned a path that does not end with its key".to_string());
        }
        let start = index(&field(result, "start")).ok_or_else(wrong)?;
        if start < units(&source[..line_start]) || start > offset {
            return Err("returned a start that is not on the line before the offset".to_string());
        }
        let start_byte = utf16_to_byte_index(source, start);
        let start_column = source[line_start..start_byte].chars().count();
        Ok(Some((path, key, start_column)))
    };
    match read() {
        Ok(found) => found,
        Err(problem) => {
            language.warn("keyAt", &problem);
            None
        }
    }
}
