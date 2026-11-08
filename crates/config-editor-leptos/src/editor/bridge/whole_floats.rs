//! Whole floats a view reads as integers (HCL reads `1.0` as `1`): a
//! number typed over one keeps the host's float type.

use serde_json::{Number, Value};

/// `value` (the host value with the edit merged in) with each integer the
/// edit typed where the host holds a whole float that the view showed as an
/// integer (`seen`) written as a float: the view could not tell them apart,
/// so the edit keeps the host's type, as an untouched value keeps its text.
pub(super) fn kept(old: &Value, seen: &Value, value: Value) -> Value {
    match (old, seen, value) {
        (Value::Object(old), Value::Object(seen), Value::Object(map)) => Value::Object(
            map.into_iter()
                .map(|(key, item)| {
                    let item = match (old.get(&key), seen.get(&key)) {
                        (Some(before), Some(shown)) => kept(before, shown, item),
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
                    .map(|((before, shown), item)| kept(before, shown, item))
                    .collect(),
            )
        }
        (Value::Number(before), Value::Number(shown), Value::Number(typed))
            if shown_as_integer(before, shown) && !typed.is_f64() =>
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
