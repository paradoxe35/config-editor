//! Values edited while editing as a language that writes every scalar as
//! text (XML), saved in the host's types.

use super::super::schema_readings;
use crate::editor::decimal_text::decimal;
use crate::editor::language::pointer_key;
use serde_json::Value;

/// `value` (the host value with the edit merged in) with each scalar the
/// edit changed in the host's type, as a view that writes every scalar as
/// text shows it: what it reads as a number or a boolean is text where the
/// host holds a string (`"1"` made `12` stays `"12"`), and an emptied
/// element (read as `{}`) is the empty string there. `Err` names a value
/// emptied where the host holds a number or a boolean: the save waits until
/// it holds one again, as when a JSON view's value is half typed.
///
/// Where `schema` allows the view's reading of a value and not the host's
/// type of it (`8` typed over `"auto"` for "an integer ≥ 1 or `auto`"),
/// the view's reading is saved: what the view validated.
///
/// So is a value the merge kept as the host held it where the view reads
/// it otherwise (`edited`): a key renamed onto one the schema types (the
/// host's string `"true"` under a key whose schema is `boolean`) is saved
/// as the view means it.
pub(in crate::editor::bridge) fn in_host_types(
    old: &Value,
    (seen, edited): (&Value, &Value),
    value: Value,
    schema: Option<&Value>,
) -> Result<Value, String> {
    let mut retyped = Vec::new();
    let mut value = in_host_types_at(old, Some(seen), value, "", &mut retyped)?;
    if let Some(schema) = schema {
        read_otherwise(&value, (seen, edited), &mut String::new(), &mut retyped);
        schema_readings::keep(&mut value, retyped, schema);
    }
    Ok(value)
}

/// Each scalar of `value` the view reads otherwise in `edited` (at the
/// same pointer), with the view's reading, where the edit wrote it (the
/// view showed no such reading there, in `seen`): an untouched value
/// keeps the host's text.
fn read_otherwise(
    value: &Value,
    (seen, edited): (&Value, &Value),
    at: &mut String,
    out: &mut Retyped,
) {
    let length = at.len();
    match (value, edited) {
        (Value::Object(map), Value::Object(theirs)) => {
            for (key, child) in map {
                if let Some(other) = theirs.get(key) {
                    at.push('/');
                    at.push_str(&pointer_key(key));
                    read_otherwise(child, (seen, other), at, out);
                    at.truncate(length);
                }
            }
        }
        (Value::Array(items), Value::Array(theirs)) => {
            for (index, (child, other)) in items.iter().zip(theirs).enumerate() {
                at.push('/');
                at.push_str(&index.to_string());
                read_otherwise(child, (seen, other), at, out);
                at.truncate(length);
            }
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {}
        (value, other) if value != other && seen.pointer(at) != Some(other) => {
            out.push((at.clone(), other.clone()));
        }
        _ => {}
    }
}

/// Values the host's type changed: each one's pointer and the view's
/// reading.
type Retyped = Vec<(String, Value)>;

fn in_host_types_at(
    old: &Value,
    seen: Option<&Value>,
    value: Value,
    at: &str,
    retyped: &mut Retyped,
) -> Result<Value, String> {
    let child = |key: &str| format!("{at}/{}", pointer_key(key));
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
                        retyped,
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
                    retyped,
                )?);
            }
            Ok(Value::Array(out))
        }
        // A list grown or shrunk, or a scalar the view repeated into one (an
        // element copied): each item the view showed before (a copy of it)
        // keeps the type of the host's value it shows.
        (Value::Array(old), Value::Array(items)) => {
            let seen = seen.and_then(Value::as_array).map(Vec::as_slice);
            items_in_host_types(old, seen.unwrap_or_default(), items, at, retyped)
        }
        (old, Value::Array(items)) if !old.is_object() && !old.is_null() => {
            let seen = seen.map(std::slice::from_ref).unwrap_or_default();
            items_in_host_types(std::slice::from_ref(old), seen, items, at, retyped)
        }
        (Value::Object(_) | Value::Array(_) | Value::Null, value) => Ok(value),
        (old, value) if *old == value => Ok(value),
        (old, value) => {
            let read = value.clone();
            let typed = scalar_in_host_type(old, seen, value, at)?;
            if typed != read {
                retyped.push((at.to_string(), read));
            }
            Ok(typed)
        }
    }
}

/// The scalar `value` the view reads (where the host holds `old`) in the
/// host's type.
fn scalar_in_host_type(
    old: &Value,
    seen: Option<&Value>,
    value: Value,
    at: &str,
) -> Result<Value, String> {
    match (old, value) {
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
        // A hexadecimal, octal or binary integer typed where the host holds
        // an integer is that number (the view reads it as text).
        (Value::Number(number), Value::String(text))
            if !number.is_f64()
                && let Some(integer) = radix_integer(&text) =>
        {
            Ok(Value::from(integer))
        }
        // A number or boolean typed in another spelling than the view's own
        // (`2.50`, `2e3`, `007`, `+6`, `False`), which the view reads as
        // text, is that number or boolean where the host holds one.
        (Value::Number(number), Value::String(text)) => {
            Ok(decimal(&text, !number.is_f64()).unwrap_or(Value::String(text)))
        }
        (Value::Bool(_), Value::String(text)) if text.eq_ignore_ascii_case("true") => {
            Ok(Value::Bool(true))
        }
        (Value::Bool(_), Value::String(text)) if text.eq_ignore_ascii_case("false") => {
            Ok(Value::Bool(false))
        }
        // A whole number typed where the host holds a float is that float.
        (Value::Number(before), Value::Number(typed)) if before.is_f64() && !typed.is_f64() => {
            Ok(typed
                .as_f64()
                .and_then(serde_json::Number::from_f64)
                .map_or(Value::Number(typed), Value::Number))
        }
        (Value::String(_), Value::Number(number)) => Ok(Value::String(number.to_string())),
        (Value::String(_), Value::Bool(flag)) => Ok(Value::String(flag.to_string())),
        (_, value) => Ok(value),
    }
}

/// `items` (a list the edit made of the host's `old` items, which the view
/// showed as `seen`), each item equal to one the view showed in the type of
/// the host's item there; an item new to the view as the view reads it.
fn items_in_host_types(
    old: &[Value],
    seen: &[Value],
    items: Vec<Value>,
    at: &str,
    retyped: &mut Retyped,
) -> Result<Value, String> {
    items
        .into_iter()
        .enumerate()
        .map(
            |(index, item)| match seen.iter().zip(old).find(|(shown, _)| **shown == item) {
                Some((shown, before)) => {
                    in_host_types_at(before, Some(shown), item, &format!("{at}/{index}"), retyped)
                }
                None => Ok(item),
            },
        )
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

/// The integer `text` writes in base 16, 8 or 2 (`0x50`, `-0o17`, `0b11`).
fn radix_integer(text: &str) -> Option<i64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (radix, digits) = match digits.get(..2)? {
        "0x" | "0X" => (16, &digits[2..]),
        "0o" | "0O" => (8, &digits[2..]),
        "0b" | "0B" => (2, &digits[2..]),
        _ => return None,
    };
    if digits.is_empty() || digits.starts_with(['+', '-']) {
        return None;
    }
    let magnitude = i64::from_str_radix(digits, radix).ok()?;
    Some(if negative { -magnitude } else { magnitude })
}
