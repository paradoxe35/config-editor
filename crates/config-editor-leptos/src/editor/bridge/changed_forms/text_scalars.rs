//! Values edited while editing as a language that writes every scalar as
//! text (XML), saved in the host's types.
//!
//! Which values the user wrote is read off the view's record of its edits
//! ([`Record`], the links the editor keeps as it writes and edits a view),
//! never off the values: a scalar left as the editor wrote it already holds
//! its host value, any other one is the user's.

use super::super::schema_readings;
use crate::editor::data_reading::DataReading;
use crate::editor::decimal_text::decimal;
use crate::editor::language::pointer_key;
use serde_json::Value;
use std::collections::HashSet;

/// `value` (the host value with the edit merged in) with each scalar the
/// edit changed in the host's type, as a view that writes every scalar as
/// text shows it: what it reads as a number or a boolean is text where the
/// host holds a string (`"1"` made `12` stays `"12"`), and an emptied
/// element (read as `{}`) is the empty string there. An emptied value where
/// the host holds a number or a boolean waits ([`Unsaved::Emptied`]) until
/// it holds one again, as when a JSON view's value is half typed.
///
/// Where `schema` allows the view's reading of a value and not the host's
/// type of it (`8` typed over `"auto"` for "an integer ≥ 1 or `auto`"),
/// the view's reading is saved: what the view validated (see
/// `schema_readings`).
///
/// So is a value the merge kept as the host held it where the view reads
/// it otherwise (`edited`) and the user wrote it (`record`): a key renamed
/// onto one the schema types (the host's string `"true"` under a key whose
/// schema is `boolean`), a number typed in another spelling (`08` over the
/// host's `"8"`, which the view showed as 8), a host scalar the view reads
/// as a list's one item whose item the user wrote (`8` typed over the
/// host's `"08"` where the schema wants a list of numbers).
pub(in crate::editor::bridge) fn in_host_types(
    old: &Value,
    (seen, edited): (&Value, &Value),
    value: Value,
    schema: Option<&Value>,
    record: &Record<'_>,
) -> Result<Value, Unsaved> {
    let mut retyped = Vec::new();
    let mut value = in_host_types_at(old, Some(seen), value, "", &mut retyped, record)?;
    if let Some(schema) = schema {
        read_otherwise(&value, edited, record, &mut String::new(), &mut retyped)?;
        schema_readings::keep(&mut value, retyped, schema);
    }
    Ok(value)
}

/// Why a value edited in a view that writes every scalar as text is not
/// saved yet, at the pointer each names.
#[derive(Debug)]
pub(in crate::editor::bridge) enum Unsaved {
    /// Emptied where the host holds a number or a boolean.
    Emptied(String),
    /// Read otherwise than the host's type where the text came with no
    /// record of its edits ([`Record::None`]): whether the user wrote it,
    /// and so which type it is saved in, is not known.
    Unrecorded(String),
}

/// What the view's record of its edits tells of its scalars (see
/// `links::untouched`).
pub(in crate::editor::bridge) enum Record<'a> {
    /// The scalars the user left as the editor wrote them, by their
    /// pointers as the view's language reads the text, and how the view
    /// reads it under the schema (which names a lone value its list's
    /// `/0`); the user wrote every other scalar.
    Kept {
        untouched: HashSet<&'a str>,
        reading: &'a DataReading,
    },
    /// A text no view of the editor's was made into (a save asked for a
    /// text alone): nothing tells which values were edited, and where that
    /// decides the save, it waits.
    None,
}

impl Record<'_> {
    /// Whether the user left the scalar at `at` (a pointer as the view
    /// reads it, a list the merge made of a lone value named through its
    /// `/0`) as the editor wrote it; `None` without a record.
    fn left(&self, at: &str) -> Option<bool> {
        let Record::Kept { untouched, reading } = self else {
            return None;
        };
        let written = reading.written(at);
        let written = written.as_deref().unwrap_or(at);
        // A lone value is its list's only item: the view writes the scalar
        // at the list's own pointer.
        let lone = written.strip_suffix("/0");
        Some(untouched.contains(written) || lone.is_some_and(|lone| untouched.contains(lone)))
    }

    /// Whether the user wrote the scalar at `at`, or why that is not known.
    fn wrote(&self, at: &str) -> Result<bool, Unsaved> {
        self.left(at)
            .map(|left| !left)
            .ok_or_else(|| Unsaved::Unrecorded(shown_pointer(at)))
    }
}

/// `at` as a save's message names it (the document as `/`).
fn shown_pointer(at: &str) -> String {
    if at.is_empty() {
        "/".to_string()
    } else {
        at.to_string()
    }
}

/// Each scalar of `value` the view reads otherwise in `edited` (at the
/// same pointer), with the view's reading, where the user wrote it: an
/// untouched value keeps the host's text.
fn read_otherwise(
    value: &Value,
    edited: &Value,
    record: &Record<'_>,
    at: &mut String,
    out: &mut Retyped,
) -> Result<(), Unsaved> {
    let length = at.len();
    match (value, edited) {
        (Value::Object(map), Value::Object(theirs)) => {
            for (key, child) in map {
                if let Some(other) = theirs.get(key) {
                    at.push('/');
                    at.push_str(&pointer_key(key));
                    read_otherwise(child, other, record, at, out)?;
                    at.truncate(length);
                }
            }
        }
        (Value::Array(items), Value::Array(theirs)) => {
            for (index, (child, other)) in items.iter().zip(theirs).enumerate() {
                at.push('/');
                at.push_str(&index.to_string());
                read_otherwise(child, other, record, at, out)?;
                at.truncate(length);
            }
        }
        // A host scalar the view reads as a list's one item (a lone element
        // where the schema wants a list), whose item the user wrote: the
        // view's list, which the host holds no other way.
        (scalar, Value::Array(items))
            if !scalar.is_object()
                && !scalar.is_array()
                && matches!(items.as_slice(), [item] if !item.is_object() && !item.is_array()) =>
        {
            if record.wrote(&format!("{at}/0"))? {
                out.push((at.clone(), edited.clone()));
            }
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {}
        (value, other) if value != other && record.wrote(at)? => {
            out.push((at.clone(), other.clone()));
        }
        _ => {}
    }
    Ok(())
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
    record: &Record<'_>,
) -> Result<Value, Unsaved> {
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
                        record,
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
                    record,
                )?);
            }
            Ok(Value::Array(out))
        }
        // A list grown or shrunk, or a scalar the view repeated into one (an
        // element copied): what each item is, the record tells.
        (Value::Array(old), Value::Array(items)) => items_in_host_types(old, items, at, record),
        (old, Value::Array(items)) if !old.is_object() && !old.is_null() => {
            items_in_host_types(std::slice::from_ref(old), items, at, record)
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

/// `items`, a list the edit made of the host's `old` items, grown, cut or
/// moved: each scalar the user left as the editor wrote it already holds
/// the value of the host item it stands for (the record names its source,
/// see `links::untouched`), and every other value in them is new to the
/// view, as the view reads it.
///
/// Without a record, an item may be a host item moved or copied (to be
/// saved in its type) or typed anew (saved as read), which nothing tells
/// apart: unless every item is text both ways, the save waits.
fn items_in_host_types(
    old: &[Value],
    items: Vec<Value>,
    at: &str,
    record: &Record<'_>,
) -> Result<Value, Unsaved> {
    if matches!(record, Record::None) && !old.iter().chain(&items).all(holds_text_only) {
        return Err(Unsaved::Unrecorded(shown_pointer(at)));
    }
    Ok(Value::Array(items))
}

/// Whether every scalar of `value` is a string.
fn holds_text_only(value: &Value) -> bool {
    match value {
        Value::String(_) => true,
        Value::Object(map) => map.values().all(holds_text_only),
        Value::Array(items) => items.iter().all(holds_text_only),
        _ => false,
    }
}

/// The scalar `value` the view reads (where the host holds `old`) in the
/// host's type.
fn scalar_in_host_type(
    old: &Value,
    seen: Option<&Value>,
    value: Value,
    at: &str,
) -> Result<Value, Unsaved> {
    match (old, value) {
        // An element emptied where the view showed a scalar.
        (old, Value::Object(map)) if map.is_empty() && !seen.is_some_and(Value::is_object) => {
            match old {
                Value::String(_) => Ok(Value::String(String::new())),
                _ => Err(Unsaved::Emptied(shown_pointer(at))),
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

/// The integer `text` writes in base 16, 8 or 2 (`0x50`, `-0o17`, `0b11`).
fn radix_integer(text: &str) -> Option<i64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (radix, digits) = match digits.get(..2)? {
        "0x" | "0X" => (16, digits.get(2..)?),
        "0o" | "0O" => (8, digits.get(2..)?),
        "0b" | "0B" => (2, digits.get(2..)?),
        _ => return None,
    };
    if digits.is_empty() || digits.starts_with(['+', '-']) {
        return None;
    }
    let magnitude = i64::from_str_radix(digits, radix).ok()?;
    Some(if negative { -magnitude } else { magnitude })
}
