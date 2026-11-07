//! Values crossing between JavaScript and Rust: arguments, paths, ranges,
//! plain data, and the shapes returned to JavaScript.

use config_editor_leptos::language::SchemaPathSegment;
use config_editor_leptos::{ChangeSummary, Diagnostic, FieldPath, TextPosition, TextRange};
use js_sys::{Array, Object, Reflect};
use serde::Serialize;
use serde_json::Value;
use wasm_bindgen::prelude::*;

/// A string argument, or a `TypeError`-style error naming it.
pub(crate) fn text_arg(value: &JsValue, name: &str) -> Result<String, JsError> {
    value
        .as_string()
        .ok_or_else(|| JsError::new(&format!("{name} must be a string")))
}

/// An optional string argument: absent (`undefined`/`null`) or a string.
pub(crate) fn optional_text(value: &JsValue, name: &str) -> Result<Option<String>, JsError> {
    if value.is_undefined() || value.is_null() {
        Ok(None)
    } else {
        text_arg(value, name).map(Some)
    }
}

pub(crate) fn field(object: &JsValue, name: &str) -> JsValue {
    Reflect::get(object, &JsValue::from_str(name)).unwrap_or(JsValue::UNDEFINED)
}

pub(crate) fn string_field(object: &JsValue, name: &str) -> Option<String> {
    field(object, name).as_string()
}

/// Deepest nesting accepted from a custom language's `parse`.
const MAX_DEPTH: usize = 256;

/// A JavaScript value as JSON: plain data only, bounded in depth so a cyclic
/// or runaway result is an error instead of a crash.
pub(crate) fn json_from_js(value: &JsValue, depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "it nests deeper than {MAX_DEPTH} levels (or is cyclic)"
        ));
    }
    if value.is_null() {
        return Ok(Value::Null);
    }
    if let Some(flag) = value.as_bool() {
        return Ok(Value::Bool(flag));
    }
    if let Some(number) = value.as_f64() {
        return Ok(number_from_js(number));
    }
    if let Some(text) = value.as_string() {
        return Ok(config_editor_leptos::string_value(text));
    }
    if Array::is_array(value) {
        return Array::from(value)
            .iter()
            .map(|item| json_from_js(&item, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    if value.is_object() && !value.is_function() {
        if !is_plain_object(value) {
            let name = Reflect::get(value, &JsValue::from_str("constructor"))
                .ok()
                .and_then(|constructor| string_field(&constructor, "name"))
                .unwrap_or_else(|| "an object".to_string());
            return Err(format!(
                "it holds a {name}, which is not plain data (pass a plain object, an array, or a string such as a date's toISOString())"
            ));
        }
        let mut map = serde_json::Map::new();
        for entry in Object::entries(value.unchecked_ref()).iter() {
            let pair: Array = entry.unchecked_into();
            let key = pair.get(0).as_string().unwrap_or_default();
            let item = pair.get(1);
            if item.is_undefined() {
                continue;
            }
            map.insert(key, json_from_js(&item, depth + 1)?);
        }
        return Ok(Value::Object(map));
    }
    Err("it holds a value JSON cannot (undefined, a function, a symbol, or a bigint)".to_string())
}

/// A JavaScript number in the data model. A whole number is an integer
/// (5, not 5.0), as `JSON.parse` reads it, also past 2^53 where the number
/// is still exactly that integer (`1e16` is 10000000000000000); infinities
/// and `NaN` are the numbers YAML and TOML write (`.inf`, `nan`).
fn number_from_js(number: f64) -> Value {
    /// 2^63 and 2^64, the ends of `i64` and `u64`.
    const I64_END: f64 = 9_223_372_036_854_775_808.0;
    const U64_END: f64 = 18_446_744_073_709_551_616.0;
    if number.fract() == 0.0 {
        if (-I64_END..I64_END).contains(&number) {
            return Value::from(number as i64);
        }
        if (0.0..U64_END).contains(&number) {
            return Value::from(number as u64);
        }
    }
    config_editor_leptos::float_value(number)
}

/// Whether `value` is a plain object (`{…}` or `Object.create(null)`), not
/// a `Date`, `Map`, `Set` or class instance, whose own fields are not its
/// data.
fn is_plain_object(value: &JsValue) -> bool {
    let prototype = Object::get_prototype_of(value);
    prototype.is_null() || Object::get_prototype_of(&prototype).is_null()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticOut {
    // Absent, not `null`, without a position (as the typings say).
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_column: Option<usize>,
    message: String,
    severity: &'static str,
}

impl From<Diagnostic> for DiagnosticOut {
    fn from(problem: Diagnostic) -> Self {
        Self {
            line: problem.line,
            column: problem.column,
            end_column: problem.end_column,
            message: problem.message,
            severity: problem.severity.as_str(),
        }
    }
}

#[derive(Serialize)]
pub(crate) struct ChangeSummaryOut {
    added: usize,
    removed: usize,
    modified: usize,
    hunks: usize,
}

impl From<ChangeSummary> for ChangeSummaryOut {
    fn from(summary: ChangeSummary) -> Self {
        Self {
            added: summary.added,
            removed: summary.removed,
            modified: summary.modified,
            hunks: summary.hunks,
        }
    }
}

/// Unchanged lines around each change in a unified diff: a whole number,
/// 3 when absent.
pub(crate) fn context_arg(value: &JsValue) -> Result<usize, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(3);
    }
    value
        .as_f64()
        .filter(|context| context.fract() == 0.0 && *context >= 0.0)
        .map(|context| context.min(u32::MAX.into()) as usize)
        .ok_or_else(|| JsError::new("context must be a whole number of lines, 0 or more"))
}

/// What a range from JavaScript must be.
const RANGE_SHAPE: &str = "range must be { start, end } (string indices) or { startLine, startColumn, endLine?, endColumn? } (one-based)";

/// A range from JavaScript: string indices `{ start, end }` into
/// `host_text`, or one-based `{ startLine, startColumn, endLine?, endColumn? }`.
/// A range past the text's end, ending before it starts, or with a number
/// that is not a whole one is an error.
pub(crate) fn range_from_js(range: &JsValue, host_text: &str) -> Result<TextRange, String> {
    if !range.is_object() || Array::is_array(range) {
        return Err(RANGE_SHAPE.to_string());
    }
    let number = |name: &str, least: f64| -> Result<Option<usize>, String> {
        let value = field(range, name);
        if value.is_undefined() || value.is_null() {
            return Ok(None);
        }
        value
            .as_f64()
            .filter(|number| number.is_finite() && number.fract() == 0.0 && *number >= least)
            .map(|number| Some(number as usize))
            .ok_or_else(|| format!("{name} must be a whole number, {least} or more"))
    };
    if let Some(start) = number("start", 0.0)? {
        let end = number("end", 0.0)?.unwrap_or(start);
        let length = host_text.encode_utf16().count();
        if end < start {
            return Err(format!("end ({end}) is before start ({start})"));
        }
        if end > length {
            return Err(format!("end ({end}) is past the text's end ({length})"));
        }
        return Ok(TextRange::new(
            TextPosition::from_offset(host_text, start),
            TextPosition::from_offset(host_text, end),
        ));
    }
    let (Some(line), Some(column)) = (number("startLine", 1.0)?, number("startColumn", 1.0)?)
    else {
        return Err(RANGE_SHAPE.to_string());
    };
    let end_line = number("endLine", 1.0)?.unwrap_or(line);
    let end_column = number("endColumn", 1.0)?.unwrap_or(column + 1);
    let lines: Vec<&str> = host_text.split('\n').collect();
    let width = |line: usize| lines.get(line - 1).map_or(0, |text| text.chars().count());
    if end_line < line || (end_line == line && end_column < column) {
        return Err(format!(
            "the end ({end_line}:{end_column}) is before the start ({line}:{column})"
        ));
    }
    for (name, line, column) in [("start", line, column), ("end", end_line, end_column)] {
        if line > lines.len() {
            return Err(format!(
                "{name} line {line} is past the text's last line ({})",
                lines.len()
            ));
        }
        if column > width(line) + 1 {
            return Err(format!(
                "{name} column {column} is past the end of line {line} ({} characters)",
                width(line)
            ));
        }
    }
    Ok(TextRange::new(
        TextPosition::new(line, column),
        TextPosition::new(end_line, end_column),
    ))
}

/// A path from JavaScript: a JSON pointer, or keys (strings) and indices
/// (non-negative integers).
pub(crate) fn path_from_js(value: &JsValue) -> Result<FieldPath, JsError> {
    if let Some(pointer) = value.as_string() {
        // A JSON pointer starts with `/` (`""` is the whole document):
        // `web/http` is a mistake, not a path.
        if !pointer.is_empty() && !pointer.starts_with('/') {
            return Err(JsError::new(&format!(
                "a JSON pointer starts with \"/\" (got {pointer:?}; \"\" is the whole document)"
            )));
        }
        return Ok(FieldPath::from(pointer));
    }
    if !Array::is_array(value) {
        return Err(JsError::new(
            "a path is a JSON pointer (\"/web/hosts/0\") or an array of keys and indices ([\"web\", \"hosts\", 0])",
        ));
    }
    let mut path = FieldPath::root();
    for segment in Array::from(value).iter() {
        path = if let Some(key) = segment.as_string() {
            path.key(key)
        } else if let Some(index) = segment
            .as_f64()
            .filter(|index| *index >= 0.0 && index.fract() == 0.0 && index.is_finite())
        {
            path.index(index as usize)
        } else {
            return Err(JsError::new(
                "a path segment is a key (string) or an index (non-negative integer)",
            ));
        };
    }
    Ok(path)
}

#[derive(Serialize)]
#[serde(untagged)]
enum SegmentOut {
    Key(String),
    Index(usize),
}

#[derive(Serialize)]
struct PathOut {
    pointer: String,
    segments: Vec<SegmentOut>,
}

pub(crate) fn path_to_js(path: &FieldPath) -> JsValue {
    to_js(&PathOut {
        pointer: path.to_pointer(),
        segments: path
            .segments()
            .iter()
            .map(|segment| match segment {
                SchemaPathSegment::Key(key) => SegmentOut::Key(key.clone()),
                SchemaPathSegment::ArrayItem(index) => SegmentOut::Index(*index),
            })
            .collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeOut {
    start: usize,
    end: usize,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

pub(crate) fn range_to_js(text: &str, range: TextRange) -> JsValue {
    to_js(&RangeOut {
        start: range.start.to_offset(text),
        end: range.end.to_offset(text),
        start_line: range.start.line,
        start_column: range.start.column,
        end_line: range.end.line,
        end_column: range.end.column,
    })
}

pub(crate) fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

/// A data model value as plain JavaScript data, as `JSON.parse` reads it
/// (integers past 2^53 lose precision there too), with infinities and NaN
/// as `Infinity`, `-Infinity` and `NaN`.
pub(crate) fn json_to_js(value: &Value) -> JsValue {
    let text = value.to_string();
    // The noncharacter that starts a non-finite number (rare: walk only then).
    if text.contains('\u{FDD0}') {
        return data_to_js(value);
    }
    js_sys::JSON::parse(&text).unwrap_or(JsValue::NULL)
}

fn data_to_js(value: &Value) -> JsValue {
    if let Some(number) = config_editor_leptos::float_of(value) {
        return JsValue::from_f64(number);
    }
    match value {
        Value::Null => JsValue::NULL,
        Value::Bool(flag) => JsValue::from_bool(*flag),
        Value::String(text) => {
            JsValue::from_str(config_editor_leptos::string_of(value).unwrap_or(text))
        }
        Value::Array(items) => items.iter().map(data_to_js).collect::<Array>().into(),
        Value::Object(map) => {
            let object = Object::new();
            for (key, item) in map {
                let _ = Reflect::set(&object, &JsValue::from_str(key), &data_to_js(item));
            }
            object.into()
        }
        // Numbers are taken by `float_of` above.
        Value::Number(_) => JsValue::NULL,
    }
}

/// A string index argument: a non-negative integer.
pub(crate) fn index_arg(value: &JsValue, name: &str) -> Result<usize, JsError> {
    value
        .as_f64()
        .filter(|index| index.is_finite() && *index >= 0.0 && index.fract() == 0.0)
        .map(|index| index as usize)
        .ok_or_else(|| JsError::new(&format!("{name} must be a non-negative integer")))
}

/// The path of what `text` holds at UTF-16 index `index`, or `None` past its
/// end: what `pathAt` answers, headless and on an editor.
pub(crate) fn path_at_index(
    text: &str,
    index: usize,
    path_at: impl FnOnce(TextPosition) -> Option<FieldPath>,
) -> Option<FieldPath> {
    if index > text.encode_utf16().count() {
        return None;
    }
    path_at(TextPosition::from_offset(text, index))
}

/// One-based lines `startLine..=endLine`: a fold keeps `startLine` visible
/// and hides the rest.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LineRangeOut {
    start_line: usize,
    end_line: usize,
}

/// One-based `(first, last)` line pairs as `LineRange[]`.
pub(crate) fn line_ranges_to_js(ranges: impl IntoIterator<Item = (usize, usize)>) -> JsValue {
    let ranges: Vec<LineRangeOut> = ranges
        .into_iter()
        .map(|(start_line, end_line)| LineRangeOut {
            start_line,
            end_line,
        })
        .collect();
    to_js(&ranges)
}
