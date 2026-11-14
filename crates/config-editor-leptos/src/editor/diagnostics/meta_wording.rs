//! Why a schema text is no JSON Schema, when the JSON Schema rules for
//! schemas refuse a value in it: in plain words, with how to write it
//! where YAML reads it otherwise than meant (`pattern: [a-z]+` is a
//! list), not the validator's own (`… is not valid under any of the
//! schemas listed in the 'anyOf' keyword`).

use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::Value;

/// The types JSON Schema knows, as `type` names them.
const TYPES: &str =
    "\"array\", \"boolean\", \"integer\", \"null\", \"number\", \"object\" or \"string\"";

/// The type names JSON Schema knows.
const KNOWN: [&str; 7] = [
    "array", "boolean", "integer", "null", "number", "object", "string",
];

/// Words YAML 1.1 read as a boolean, and YAML 1.2 (as the editor reads
/// it) reads as text.
const YES_NO: [&str; 6] = ["yes", "no", "on", "off", "y", "n"];

/// `error`, the rules for schemas refusing a value of the schema, worded.
pub(super) fn message(error: &ValidationError<'_>) -> String {
    let value = error.instance();
    let at = error.instance_path().to_string();
    let keyword = at.rsplit('/').next().unwrap_or_default();
    let worded = match error.kind() {
        ValidationErrorKind::AnyOf { .. } if keyword == "type" => {
            // A list of types: the names in it JSON Schema does not know.
            let unknown = match &**value {
                Value::Array(names) => names
                    .iter()
                    .filter(|name| !name.as_str().is_some_and(|name| KNOWN.contains(&name)))
                    .map(Value::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                other => other.to_string(),
            };
            format!("{unknown} is no type JSON Schema knows: use {TYPES}")
        }
        ValidationErrorKind::Format { format } if format == "regex" => {
            format!(
                "{value} is no regular expression the editor reads: check its brackets and escapes"
            )
        }
        // `additionalProperties: no`: YAML 1.2 reads `no` as text.
        ValidationErrorKind::Type { .. }
            if value
                .as_str()
                .is_some_and(|text| YES_NO.contains(&text.to_lowercase().as_str()))
                && error.to_string().contains("\"boolean\"") =>
        {
            format!(
                "{keyword} must be true or false, and {value} reads as text in YAML: write {}",
                if matches!(
                    value.as_str().map(str::to_lowercase).as_deref(),
                    Some("yes" | "y" | "on")
                ) {
                    "true"
                } else {
                    "false"
                }
            )
        }
        ValidationErrorKind::AnyOf { .. } | ValidationErrorKind::OneOfNotValid { .. } => {
            format!("{value} is not a value {keyword} takes")
        }
        ValidationErrorKind::Type { .. }
            if matches!(**value, Value::Object(_) | Value::Array(_))
                && error.to_string().ends_with("is not of type \"string\"") =>
        {
            format!(
                "{keyword} must be text, and reads as {value}: in YAML a value starting with {{ or [ is a map or list, so quote it, as in {keyword}: '…'"
            )
        }
        _ => error.to_string(),
    };
    if at.is_empty() {
        worded
    } else {
        format!("{worded} (at {at})")
    }
}
