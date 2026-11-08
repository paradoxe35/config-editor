//! Strings a host that writes every scalar as text (XML) reads back as
//! something else: `"true"`, `"null"` and `"1"` are its boolean, null and
//! number, `""` its empty element. Such an edit waits with the reason
//! instead of being saved as another value.

use serde_json::Value;

/// The first string of `value` that `read` (the host's reading of it as
/// written) holds as another value: its pointer, and what the host reads.
pub(super) fn first_read_otherwise(value: &Value, read: &Value) -> Option<(String, Value)> {
    match (value, read) {
        (Value::String(_), _) if value != read => Some((String::new(), read.clone())),
        (Value::Object(map), Value::Object(read)) => map.iter().find_map(|(key, child)| {
            let (rest, found) = first_read_otherwise(child, read.get(key)?)?;
            Some((
                format!("/{}{rest}", key.replace('~', "~0").replace('/', "~1")),
                found,
            ))
        }),
        (Value::Array(items), Value::Array(read)) => {
            items
                .iter()
                .zip(read)
                .enumerate()
                .find_map(|(index, (child, read))| {
                    let (rest, found) = first_read_otherwise(child, read)?;
                    Some((format!("/{index}{rest}"), found))
                })
        }
        _ => None,
    }
}

/// Why the save waits: the string at `pointer` reads back as `read`.
pub(super) fn reason(host: &str, string: &str, read: &Value, pointer: &str) -> String {
    match read {
        Value::Object(map) if map.is_empty() && string.is_empty() => format!(
            "{host} reads an empty element as an empty object, not as the empty string: the edit is saved once it holds text (at {pointer})"
        ),
        Value::Null | Value::Bool(_) | Value::Number(_) => {
            let kind = match read {
                Value::Null => "null",
                Value::Bool(_) => "the boolean",
                _ => "the number",
            };
            let read = if read.is_null() {
                String::new()
            } else {
                format!(" {read}")
            };
            format!(
                "{host} reads the text {string} as {kind}{read}, not as a string: the edit is saved once it is written without quotes or holds other text (at {pointer})"
            )
        }
        _ => format!(
            "{host} reads the string {} back as {read} (at {pointer})",
            Value::String(string.to_string())
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::first_read_otherwise;
    use serde_json::json;

    #[test]
    fn a_string_read_back_as_another_value_is_found() {
        let value = json!({"r": {"a": "x", "l": ["1", "true"], "e": ""}});
        let read = json!({"r": {"a": "x", "l": ["1", true], "e": {}}});
        assert_eq!(
            first_read_otherwise(&value, &read),
            Some(("/r/l/1".to_string(), json!(true)))
        );
        assert_eq!(first_read_otherwise(&value, &value), None);
        // Not strings: numbers and lists are others' business.
        assert_eq!(
            first_read_otherwise(&json!({"n": 1}), &json!({"n": "1"})),
            None
        );
    }
}
