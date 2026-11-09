//! Whole floats written as floats (`1.0`): the HCL writer's numbers hold
//! no whole float, so it writes `1`. Readers still take `1.0` as the number
//! 1, but a view of a host's float shows it as the host wrote it.

use super::super::super::codec::byte_range;
use super::super::super::language::Span;
use serde_json::Value;

/// `text` (written for `value`) with each whole float of `value` that it
/// writes as an integer (`1`) written with a fraction (`1.0`); `spans`
/// finds the values by pointer.
pub(super) fn with_whole_floats(
    text: &str,
    value: &Value,
    spans: impl Fn(&str, &[String]) -> Vec<Option<Span>>,
) -> String {
    let mut pointers = Vec::new();
    whole_floats(value, String::new(), &mut pointers);
    if pointers.is_empty() {
        return text.to_string();
    }
    let integers: Vec<String> = pointers.iter().map(|(_, integer)| integer.clone()).collect();
    let paths: Vec<String> = pointers.into_iter().map(|(pointer, _)| pointer).collect();
    let mut edits: Vec<(std::ops::Range<usize>, String)> = spans(text, &paths)
        .into_iter()
        .zip(&integers)
        .filter_map(|(span, integer)| {
            let range = byte_range(text, span?)?;
            (text[range.clone()] == **integer).then(|| (range, format!("{integer}.0")))
        })
        .collect();
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut written = text.to_string();
    for (range, float) in edits {
        written.replace_range(range, &float);
    }
    written
}

/// The pointer of each whole float in `value`, with the integer it equals.
fn whole_floats(value: &Value, pointer: String, out: &mut Vec<(String, String)>) {
    match value {
        Value::Number(number) if number.is_f64() => {
            if let Some(float) = number.as_f64()
                && float.is_finite()
                && float.fract() == 0.0
                && float.abs() < 1e15
            {
                out.push((pointer, format!("{float:.0}")));
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                whole_floats(child, format!("{pointer}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                whole_floats(child, format!("{pointer}/{index}"), out);
            }
        }
        _ => {}
    }
}

/// Stands in for a whole float written with a fraction or exponent (`1.0`,
/// `1e3`) while the document is read: `hcl-rs` numbers hold no whole float,
/// so it would read as the integer 1. No document holds this string.
const WHOLE_FLOAT: &str = "\u{0}whole float\u{0}";

/// `number` (a value as the parser read it) marked when written as a float
/// that reads as a whole number, so the read value keeps it a float, as
/// every other language reads `1.0`.
pub(super) fn mark_whole_float(expression: &mut hcl_edit::expr::Expression) {
    let hcl_edit::expr::Expression::Number(number) = expression else {
        return;
    };
    let written = number.as_repr().map(|repr| repr.to_string()).unwrap_or_default();
    let value = number.value();
    if !value.is_f64() && written.contains(['.', 'e', 'E']) && let Some(float) = value.as_f64() {
        *expression = hcl_edit::expr::Expression::String(hcl_edit::Decorated::new(format!("{WHOLE_FLOAT}{float:?}")));
    }
}

/// `value` with each marked whole float read as the float it is.
pub(super) fn read_whole_floats(value: Value) -> Value {
    match value {
        Value::String(text) => match text.strip_prefix(WHOLE_FLOAT).and_then(|float| float.parse::<f64>().ok()) {
            Some(float) => serde_json::Number::from_f64(float).map_or(Value::String(text), Value::Number),
            None => Value::String(text),
        },
        Value::Object(map) => Value::Object(map.into_iter().map(|(key, child)| (key, read_whole_floats(child))).collect()),
        Value::Array(items) => Value::Array(items.into_iter().map(read_whole_floats).collect()),
        other => other,
    }
}
