//! Whole floats a view cannot tell from integers (XML text, HCL's one
//! number type): a number typed over one keeps the host's float type.

use super::super::language::ConfigLanguage;
use serde_json::{Number, Value};

/// `value` (the host value with the edit merged in) with each integer the
/// edit typed where the host holds a whole float that the view showed as an
/// integer (`seen`) written as a float: the view could not tell them apart,
/// so the edit keeps the host's type, as an untouched value keeps its text.
/// A view that reads no whole number as a float (`!floats`) keeps the
/// host's float type for every integer typed over a float.
pub(super) fn kept(old: &Value, seen: &Value, value: Value, floats: bool) -> Value {
    match (old, seen, value) {
        (Value::Object(old), Value::Object(seen), Value::Object(map)) => Value::Object(
            map.into_iter()
                .map(|(key, item)| {
                    let item = match (old.get(&key), seen.get(&key)) {
                        (Some(before), Some(shown)) => kept(before, shown, item, floats),
                        _ => item,
                    };
                    (key, item)
                })
                .collect(),
        ),
        (Value::Array(old), Value::Array(seen), Value::Array(items))
            if old.len() == items.len() && seen.len() == items.len() =>
        {
            Value::Array(
                old.iter()
                    .zip(seen)
                    .zip(items)
                    .map(|((before, shown), item)| kept(before, shown, item, floats))
                    .collect(),
            )
        }
        (Value::Number(before), Value::Number(shown), Value::Number(typed))
            if (shown_as_integer(before, shown) || (!floats && before.is_f64()))
                && !typed.is_f64() =>
        {
            typed
                .as_f64()
                .and_then(Number::from_f64)
                .map_or(Value::Number(typed), Value::Number)
        }
        (_, _, value) => value,
    }
}

/// Whether `before` is a whole float the view shows as the integer `shown`.
fn shown_as_integer(before: &Number, shown: &Number) -> bool {
    before.is_f64()
        && !shown.is_f64()
        && before.as_f64().is_some_and(|float| float.fract() == 0.0)
        && before.as_f64() == shown.as_f64()
}

/// Whether `language` tells a whole float from an integer: it reads a
/// whole float it writes (`1.0`) as a float, and its numbers are not one
/// type ([`ConfigLanguage::one_number_type`]: HCL's `2` is `2.0`).
pub(super) fn tells_floats(language: &dyn ConfigLanguage) -> bool {
    if language.one_number_type() {
        return false;
    }
    let document = serde_json::json!({ "k": 1.0 });
    language
        .serialize(&document, None)
        .ok()
        .and_then(|text| language.parse(&text).ok())
        .is_some_and(|read| read.pointer("/k").is_some_and(Value::is_f64))
}
