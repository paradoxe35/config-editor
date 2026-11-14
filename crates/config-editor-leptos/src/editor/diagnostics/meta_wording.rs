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
            let unknown: Vec<String> = match &**value {
                Value::Array(names) => names
                    .iter()
                    .filter(|name| !name.as_str().is_some_and(|name| KNOWN.contains(&name)))
                    .map(Value::to_string)
                    .collect(),
                other => vec![other.to_string()],
            };
            match unknown.split_last() {
                Some((last, rest)) if !rest.is_empty() => format!(
                    "{} and {last} are no types JSON Schema knows: use {TYPES}",
                    rest.join(", ")
                ),
                _ => format!(
                    "{} is no type JSON Schema knows: use {TYPES}",
                    unknown.concat()
                ),
            }
        }
        ValidationErrorKind::Format { format } if format == "regex" => {
            format!(
                "{value} is no regular expression the editor reads: check its brackets and escapes"
            )
        }
        // `additionalProperties: "false"`: the quotes make it text.
        ValidationErrorKind::Type { .. }
            if matches!(value.as_str(), Some("true" | "false"))
                && error.to_string().contains("\"boolean\"") =>
        {
            format!(
                "{keyword} must be true or false, and {value} in quotes reads as text: write {} without quotes",
                value.as_str().unwrap_or_default()
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
        ValidationErrorKind::Type { .. } => {
            let message = error.to_string();
            let shown = super::wording::cut(&value.to_string());
            let types = super::wording::listed_types(&message).unwrap_or_default();
            let types: Vec<&str> = types.iter().map(|name| name.trim_matches('"')).collect();
            if types == ["boolean", "object"] || types == ["object", "boolean"] {
                format!(
                    "a schema must be a map of keywords, or true or false, and this one reads as {shown}"
                )
            } else {
                let wanted: Vec<&str> = types.iter().map(|name| type_words(name)).collect();
                format!(
                    "{keyword} must be {}, and reads as {shown}",
                    wanted.join(" or ")
                )
            }
        }
        ValidationErrorKind::MinItems { limit } => format!(
            "the number of items in {keyword} must be at least {limit}, and is {}",
            value.as_array().map_or(0, Vec::len)
        ),
        ValidationErrorKind::MaxItems { limit } => format!(
            "the number of items in {keyword} must be at most {limit}, and is {}",
            value.as_array().map_or(0, Vec::len)
        ),
        ValidationErrorKind::UniqueItems => {
            let items = value.as_array().map(Vec::as_slice).unwrap_or_default();
            let repeated = items
                .iter()
                .enumerate()
                .find_map(|(index, item)| items[..index].contains(item).then_some(item));
            match repeated {
                Some(item) => format!("{keyword} lists {item} more than once"),
                None => format!("{keyword} lists an item more than once"),
            }
        }
        ValidationErrorKind::Minimum { limit } => {
            format!("{keyword} must be {limit} or more, and is {value}")
        }
        ValidationErrorKind::ExclusiveMinimum { limit } => {
            format!("{keyword} must be more than {limit}, and is {value}")
        }
        ValidationErrorKind::Enum { options } => {
            let options: Vec<String> = options
                .as_array()
                .map(|options| options.iter().map(Value::to_string).collect())
                .unwrap_or_default();
            format!(
                "{keyword} must be one of {}, and is {value}",
                options.join(", ")
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

/// What a value of JSON type `name` is called.
fn type_words(name: &str) -> &str {
    match name {
        "object" => "a map",
        "array" => "a list",
        "string" => "text",
        "boolean" => "true or false",
        "integer" => "a whole number",
        "number" => "a number",
        other => other,
    }
}
