//! Values crossing between JavaScript and Rust: arguments, paths, ranges,
//! plain data, and the shapes returned to JavaScript.

use config_editor_leptos::language::SchemaPathSegment;
use config_editor_leptos::{
    ChangeSummary, Diagnostic, DiagnosticOrigin, FieldPath, TextPosition, TextRange,
};
use js_sys::{Array, Object, Reflect};

use crate::text_lines::TextLines;
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
    if value.is_bigint() {
        // An integer `parse` gave as a bigint (past 2^53), exact.
        let text = String::from(
            js_sys::BigInt::unchecked_from_js_ref(value)
                .to_string(10)
                .unwrap_or_default(),
        );
        return text
            .parse::<i64>()
            .map(Value::from)
            .or_else(|_| text.parse::<u64>().map(Value::from))
            .map_err(|_| {
                format!(
                    "it holds the integer {text}, past the 64-bit integers ({} to {})",
                    i64::MIN,
                    u64::MAX
                )
            });
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
    Err("it holds a value JSON cannot (undefined, a function or a symbol)".to_string())
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
    /// Only for a marker over several lines.
    #[serde(skip_serializing_if = "Option::is_none")]
    end_line: Option<usize>,
    message: String,
    severity: &'static str,
    /// `"editor"` (syntax, schema, secrets, conversion) or `"host"`.
    origin: &'static str,
    /// Only on a deprecated key's warning (`true`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    deprecated: bool,
}

impl From<Diagnostic> for DiagnosticOut {
    fn from(problem: Diagnostic) -> Self {
        Self {
            line: problem.line,
            column: problem.column,
            end_column: problem.end_column,
            end_line: problem.end_line,
            message: problem.message,
            severity: problem.severity.as_str(),
            origin: match problem.origin {
                DiagnosticOrigin::Host => "host",
                _ => "editor",
            },
            deprecated: problem.deprecated,
        }
    }
}

/// Problems as JavaScript `Diagnostic`s.
pub(crate) fn diagnostics_to_js(problems: Vec<Diagnostic>) -> JsValue {
    let out: Vec<DiagnosticOut> = problems.into_iter().map(DiagnosticOut::from).collect();
    to_js(&out)
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

/// A plain object with only the `keys` of `object` it has.
fn pick(object: &JsValue, keys: &[&str]) -> JsValue {
    let picked = js_sys::Object::new();
    for key in keys {
        let value = field(object, key);
        if !value.is_undefined() {
            let _ = js_sys::Reflect::set(&picked, &JsValue::from_str(key), &value);
        }
    }
    picked.into()
}

/// What a range from JavaScript must be.
const RANGE_SHAPE: &str = "range must be { start, end } (string indices) or { startLine, startColumn, endLine?, endColumn? } (one-based)";

/// A range from JavaScript: string indices `{ start, end }` into
/// `host_text` (split into its lines once for all its ranges), or one-based
/// `{ startLine, startColumn, endLine?, endColumn? }`. A range past the
/// text's end, ending before it starts, or with a number that is not a
/// whole one is an error.
pub(crate) fn range_from_js(range: &JsValue, host_text: &TextLines) -> Result<TextRange, String> {
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
    let by_index = ["start", "end"]
        .iter()
        .any(|name| !field(range, name).is_undefined());
    let by_line = ["startLine", "startColumn", "endLine", "endColumn"]
        .iter()
        .any(|name| !field(range, name).is_undefined());
    if by_index && by_line {
        // Both, as `rangeOf` returns them: they must name one stretch.
        let indices = range_from_js(&pick(range, &["start", "end"]), host_text)?;
        let lines = range_from_js(
            &pick(range, &["startLine", "startColumn", "endLine", "endColumn"]),
            host_text,
        )?;
        // The editor's own `rangeOf` counts its indices in the text on
        // screen, the host's with LF breaks: the same stretch there.
        let on_screen = |name: &str| {
            field(range, name)
                .as_f64()
                .and_then(|offset| host_text.position_in_lf(offset as usize))
        };
        let same_on_screen =
            on_screen("start") == Some(lines.start) && on_screen("end") == Some(lines.end);
        if indices != lines && !same_on_screen {
            return Err(
                "range gives { start, end } and { startLine, startColumn, … } for different stretches: give one"
                    .to_string(),
            );
        }
        return Ok(lines);
    }
    if by_index {
        let (Some(start), Some(end)) = (number("start", 0.0)?, number("end", 0.0)?) else {
            return Err(RANGE_SHAPE.to_string());
        };
        let length = host_text.units();
        if end < start {
            return Err(format!("end ({end}) is before start ({start})"));
        }
        if end > length {
            return Err(format!("end ({end}) is past the text's end ({length})"));
        }
        return Ok(TextRange::new(
            host_text.position(start),
            host_text.position(end),
        ));
    }
    let (Some(line), Some(column)) = (number("startLine", 1.0)?, number("startColumn", 1.0)?)
    else {
        return Err(RANGE_SHAPE.to_string());
    };
    let end_line = number("endLine", 1.0)?.unwrap_or(line);
    let width = |line: usize| host_text.width(line);
    let column = clamped_column("start", line, column, host_text)?;
    // Left out, the end is one character on, at most the line's end (an
    // empty line's range is its start).
    let end_column = match number("endColumn", 1.0)? {
        Some(end_column) => end_column,
        None if end_line == line => (column + 1).min(width(line) + 1).max(column),
        None => (column + 1).min(width(end_line) + 1),
    };
    if end_line < line || (end_line == line && end_column < column) {
        return Err(format!(
            "the end ({end_line}:{end_column}) is before the start ({line}:{column})"
        ));
    }
    let end_column = clamped_column("end", end_line, end_column, host_text)?;
    Ok(TextRange::new(
        TextPosition::new(line, column),
        TextPosition::new(end_line, end_column),
    ))
}

/// `column` of one-based `line` of `lines`: a line past the text is an
/// error; a column past the line's end (counted in another unit, or for a
/// line since shortened) is the line's end, with a warning: one range is
/// not worth refusing the whole update for.
fn clamped_column(
    name: &str,
    line: usize,
    column: usize,
    lines: &TextLines,
) -> Result<usize, String> {
    if line > lines.count() {
        return Err(format!(
            "{name} line {line} is past the text's last line ({})",
            lines.count()
        ));
    }
    let width = lines.width(line);
    if column > width + 1 {
        warn(&format!(
            "config-editor: a range's {name} column {column} is past the end of line {line} ({width} characters); it is read as the line's end (columns count characters, code points)"
        ));
        return Ok(width + 1);
    }
    Ok(column)
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
        // Inside a segment `~` escapes only `~0` (`~`) and `~1` (`/`).
        if let Some(at) = pointer
            .match_indices('~')
            .map(|(at, _)| at)
            .find(|at| !matches!(pointer.as_bytes().get(at + 1), Some(b'0' | b'1')))
        {
            return Err(JsError::new(&format!(
                "a JSON pointer escapes only ~0 (~) and ~1 (/) (got {pointer:?}, at {at})"
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

/// `range` with the string indices `start` and `end` its ends are at.
pub(crate) fn range_out_to_js(range: TextRange, start: usize, end: usize) -> JsValue {
    to_js(&RangeOut {
        start,
        end,
        start_line: range.start.line,
        start_column: range.start.column,
        end_line: range.end.line,
        end_column: range.end.column,
    })
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

/// Each of `ranges` (`null` for none) as [`range_to_js`] writes it, the
/// string indexes found from the text's lines split once (not by reading
/// the text up to each).
pub(crate) fn ranges_to_js(text: &str, ranges: &[Option<TextRange>]) -> JsValue {
    let lines = TextLines::new(text);
    let offset = |position: TextPosition| lines.offset(position);
    let out = Array::new();
    for range in ranges {
        out.push(&range.map_or(JsValue::NULL, |range| {
            to_js(&RangeOut {
                start: offset(range.start),
                end: offset(range.end),
                start_line: range.start.line,
                start_column: range.start.column,
                end_line: range.end.line,
                end_column: range.end.column,
            })
        }));
    }
    out.into()
}

pub(crate) fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

/// A data model value as plain JavaScript data, as `JSON.parse` reads it,
/// with infinities and NaN as `Infinity`, `-Infinity` and `NaN`, and an
/// integer past 2^53 (TOML and YAML hold 64-bit ones) as a `bigint`, exact.
pub(crate) fn json_to_js(value: &Value) -> JsValue {
    // The noncharacter that starts a non-finite number, or a large integer
    // (rare: walk only then).
    if needs_walk(value) {
        return data_to_js(value);
    }
    js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL)
}

/// The largest integer a JavaScript number holds exactly (2^53 - 1).
const SAFE_INTEGER: u64 = (1 << 53) - 1;

/// Whether `value` holds a large integer or a non-finite number.
fn needs_walk(value: &Value) -> bool {
    match value {
        Value::Number(number) => large_integer(number).is_some(),
        Value::String(text) => text.starts_with('\u{FDD0}'),
        Value::Array(items) => items.iter().any(needs_walk),
        Value::Object(map) => map.values().any(needs_walk),
        _ => false,
    }
}

/// `number` as a bigint, when it is an integer a JavaScript number cannot
/// hold exactly.
fn large_integer(number: &serde_json::Number) -> Option<JsValue> {
    if let Some(integer) = number.as_i64() {
        return (integer.unsigned_abs() > SAFE_INTEGER)
            .then(|| js_sys::BigInt::from(integer).into());
    }
    let integer = number.as_u64()?;
    (integer > SAFE_INTEGER).then(|| js_sys::BigInt::from(integer).into())
}

fn data_to_js(value: &Value) -> JsValue {
    if let Value::Number(number) = value
        && let Some(big) = large_integer(number)
    {
        return big;
    }
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

/// A console warning (none in native tests).
fn warn(message: &str) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::warn_1(&JsValue::from_str(message));
    #[cfg(not(target_arch = "wasm32"))]
    let _ = message;
}
