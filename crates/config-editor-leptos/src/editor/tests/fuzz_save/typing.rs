//! A schema for a session's document: each value typed as the document
//! holds it, by one of a few schemas a seed picks per key, so that an XML
//! view reads its text by the schema (`8080` a number or text) and a value
//! typed there may be read otherwise than the host holds it (an integer
//! typed over a string the schema also lists), which the save keeps or
//! puts in the host's type by the editor's record of the edits.

use serde_json::{Map, Value, json};

/// The schema of `value` under `seed`.
pub(super) fn schema(value: &Value, seed: u64) -> Value {
    at(value, seed, &mut String::new())
}

/// The schema of `value` at `path` (its keys, list items read as one).
fn at(value: &Value, seed: u64, path: &mut String) -> Value {
    match value {
        Value::Object(map) => {
            let properties: Map<String, Value> = map
                .iter()
                .map(|(key, child)| {
                    let length = path.len();
                    path.push('/');
                    path.push_str(key);
                    let schema = at(child, seed, path);
                    path.truncate(length);
                    (key.clone(), schema)
                })
                .collect();
            json!({"type": "object", "properties": properties})
        }
        Value::Array(items) => {
            let length = path.len();
            path.push_str("/[]");
            // Items of one kind share a schema; a mixed list takes any.
            let same = items
                .windows(2)
                .all(|pair| kind(&pair[0]) == kind(&pair[1]));
            let items_schema = match items.first() {
                Some(first) if same => at(first, seed, path),
                _ => json!({}),
            };
            path.truncate(length);
            json!({"type": "array", "items": items_schema})
        }
        scalar => scalar_schema(scalar, pick(seed, path)),
    }
}

/// The schema of a scalar: its own type; its type or the other reading
/// XML text has (a string or an integer); the string listed beside
/// integers; anything.
fn scalar_schema(scalar: &Value, pick: u64) -> Value {
    let own = kind(scalar);
    match (pick % 4, scalar) {
        (0, _) => json!({"type": own}),
        (1, Value::String(_)) => json!({"type": ["integer", "string"]}),
        (1, _) => json!({"type": [own, "string"]}),
        (2, Value::String(text)) => json!({"anyOf": [{"type": "integer"}, {"enum": [text]}]}),
        (2, _) => json!({"anyOf": [{"type": own}, {"enum": ["auto"]}]}),
        _ => json!({}),
    }
}

/// The JSON Schema type of `value` (a whole number an integer).
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// A number from `seed` and `path` (FNV-1a).
fn pick(seed: u64, path: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ seed;
    for byte in path.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash >> 7
}
