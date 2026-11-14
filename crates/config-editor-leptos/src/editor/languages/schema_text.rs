//! A schema's text read into its JSON value: JSON, or YAML (feature
//! `yaml`) when it is no JSON.
//!
//! YAML holds every JSON text with the same meaning, so a JSON text is read
//! as JSON first and keeps its exact reading; only a text JSON refuses goes
//! to YAML. A text that starts with `{` or `[` was meant as JSON: when YAML
//! refuses it too (or reads it as a flow map otherwise), its JSON error is
//! the one worth showing. A place in the text is said as a document's
//! problem says it: `(line 2, column 1)` after the message.

use serde_json::Value;

/// What a schema nested deeper than the reader goes is told, in both
/// readings.
const TOO_DEEP: &str =
    "it nests objects and lists more than 128 levels deep, deeper than the editor reads";

/// What an empty text (or one of spaces only) is told.
const NO_SCHEMA: &str = "the text holds no schema";

/// What a text holding a JSON comment is told.
const JSON_COMMENT: &str = "comments aren't allowed in JSON";

/// The JSON value of `text` (no byte order mark), or why it is neither
/// JSON nor YAML.
pub(crate) fn read_schema_text(text: &str) -> Result<Value, String> {
    if text.trim().is_empty() {
        return Err(NO_SCHEMA.to_string());
    }
    let json_error = match serde_json::from_str(text) {
        Ok(value) => return a_schema(value),
        Err(error) => error,
    };
    let json_message = json_message(&json_error);
    // A comment where JSON stops (`// my schema`, `/* … */`) was meant as
    // JSONC: said so, not read as YAML.
    if starts_comment(text, json_error.line(), json_error.column()) {
        return Err(located(
            JSON_COMMENT,
            json_error.line(),
            json_error.column(),
        ));
    }
    let start = text.trim_start();
    if start.starts_with('{') || start.starts_with('[') {
        // A flow map YAML reads (`{type: object}`) is YAML, as it says.
        return read_flow_yaml(text).ok_or(json_message).and_then(a_schema);
    }
    read_yaml(text, json_message)
}

/// `value` when a JSON Schema can be it: a map or a boolean.
fn a_schema(value: Value) -> Result<Value, String> {
    if value.is_object() || value.is_boolean() {
        Ok(value)
    } else {
        Err(format!(
            "the text reads as {value}, and a JSON Schema is a map or true/false"
        ))
    }
}

/// `message` with the place it is about, as a document's problem says it.
fn located(message: &str, line: usize, column: usize) -> String {
    format!("{message} (line {line}, column {column})")
}

/// The JSON reader's error, its place said as the editor says places.
fn json_message(error: &serde_json::Error) -> String {
    let message = error.to_string();
    if message.starts_with("recursion limit exceeded") {
        return TOO_DEEP.to_string();
    }
    let (line, column) = (error.line(), error.column());
    let bare = message
        .strip_suffix(&format!(" at line {line} column {column}"))
        .unwrap_or(&message);
    if line == 0 {
        bare.to_string()
    } else {
        located(bare, line, column.max(1))
    }
}

/// Whether a JSONC comment (`//`, `/*`) starts at one-based `line` and
/// `column` of `text`.
fn starts_comment(text: &str, line: usize, column: usize) -> bool {
    let Some(line) = line
        .checked_sub(1)
        .and_then(|line| text.split('\n').nth(line))
    else {
        return false;
    };
    let rest: String = line
        .chars()
        .skip(column.saturating_sub(1))
        .take(2)
        .collect();
    rest == "//" || rest == "/*"
}

#[cfg(feature = "yaml")]
fn read_yaml(text: &str, _json_message: String) -> Result<Value, String> {
    yaml::read(text)
}

#[cfg(feature = "yaml")]
fn read_flow_yaml(text: &str) -> Option<Value> {
    yaml::read(text).ok()
}

#[cfg(not(feature = "yaml"))]
fn read_yaml(_text: &str, json_message: String) -> Result<Value, String> {
    Err(format!(
        "it is no JSON ({json_message}), and a schema written in YAML needs the editor's `yaml` feature"
    ))
}

#[cfg(not(feature = "yaml"))]
fn read_flow_yaml(_text: &str) -> Option<Value> {
    None
}

#[cfg(feature = "yaml")]
#[path = "schema_text_yaml.rs"]
mod yaml;
