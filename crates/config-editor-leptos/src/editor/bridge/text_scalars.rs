//! Values edited while editing as a language that writes every scalar as
//! text (XML), saved in the host's types.

use serde_json::Value;

/// `value` (the host value with the edit merged in) with each scalar the
/// edit changed in the host's type, as a view that writes every scalar as
/// text shows it: what it reads as a number or a boolean is text where the
/// host holds a string (`"1"` made `12` stays `"12"`), and an emptied
/// element (read as `{}`) is the empty string there. `Err` names a value
/// emptied where the host holds a number or a boolean: the save waits until
/// it holds one again, as when a JSON view's value is half typed.
pub(super) fn in_host_types(old: &Value, seen: &Value, value: Value) -> Result<Value, String> {
    in_host_types_at(old, Some(seen), value, "")
}

fn in_host_types_at(
    old: &Value,
    seen: Option<&Value>,
    value: Value,
    at: &str,
) -> Result<Value, String> {
    let child = |key: &str| format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
    match (old, value) {
        (Value::Object(old), Value::Object(map)) if !map.is_empty() => {
            let seen = seen.and_then(Value::as_object);
            let mut entries = serde_json::Map::with_capacity(map.len());
            for (key, item) in map {
                let item = match old.get(&key) {
                    Some(before) => in_host_types_at(
                        before,
                        seen.and_then(|seen| seen.get(&key)),
                        item,
                        &child(&key),
                    )?,
                    None => item,
                };
                entries.insert(key, item);
            }
            Ok(Value::Object(entries))
        }
        (Value::Array(old), Value::Array(items)) if old.len() == items.len() => {
            let seen = seen.and_then(Value::as_array);
            let mut out = Vec::with_capacity(items.len());
            for (index, (before, item)) in old.iter().zip(items).enumerate() {
                out.push(in_host_types_at(
                    before,
                    seen.and_then(|seen| seen.get(index)),
                    item,
                    &child(&index.to_string()),
                )?);
            }
            Ok(Value::Array(out))
        }
        (Value::Object(_) | Value::Array(_) | Value::Null, value) => Ok(value),
        (old, value) if *old == value => Ok(value),
        // An element emptied where the view showed a scalar.
        (old, Value::Object(map)) if map.is_empty() && !seen.is_some_and(Value::is_object) => {
            match old {
                Value::String(_) => Ok(Value::String(String::new())),
                _ => Err(if at.is_empty() {
                    "/".to_string()
                } else {
                    at.to_string()
                }),
            }
        }
        (Value::String(_), Value::Number(number)) => Ok(Value::String(number.to_string())),
        (Value::String(_), Value::Bool(flag)) => Ok(Value::String(flag.to_string())),
        (_, value) => Ok(value),
    }
}
