//! Values written so that equal ones are written the same: keys for
//! looking values up in a map instead of comparing each with all.

use serde_json::Value;

/// `value` written so that equal values (`==`, whatever their maps' key
/// order) are written the same.
pub(in crate::editor) fn canonical(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, String)> = map
                .iter()
                .map(|(key, item)| (key, canonical(item)))
                .collect();
            entries.sort_unstable();
            let body: Vec<String> = entries
                .into_iter()
                .map(|(key, item)| format!("{}:{item}", Value::String(key.clone())))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            format!(
                "[{}]",
                items.iter().map(canonical).collect::<Vec<_>>().join(",")
            )
        }
        other => other.to_string(),
    }
}
