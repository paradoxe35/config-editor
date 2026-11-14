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

/// `error`, the rules for schemas refusing a value of the schema, worded.
pub(super) fn message(error: &ValidationError<'_>) -> String {
    let value = error.instance();
    let at = error.instance_path().to_string();
    let keyword = at.rsplit('/').next().unwrap_or_default();
    let worded = match error.kind() {
        ValidationErrorKind::AnyOf { .. } if keyword == "type" => {
            format!("{value} is no type JSON Schema knows: use {TYPES}")
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
