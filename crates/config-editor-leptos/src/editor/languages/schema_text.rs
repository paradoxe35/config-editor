//! A schema's text read into its JSON value: JSON, or YAML (feature
//! `yaml`) when it is no JSON and does not start as JSON does.
//!
//! YAML holds every JSON text with the same meaning, so a JSON text is read
//! as JSON first and keeps its exact reading; only a text JSON refuses goes
//! to YAML. A text that starts with `{` or `[` was meant as JSON: its JSON
//! error is the one worth showing.

use serde_json::Value;

/// What a schema nested deeper than the reader goes is told, in both
/// readings.
const TOO_DEEP: &str =
    "it nests objects and lists more than 128 levels deep, deeper than the editor reads";

/// The JSON value of `text` (no byte order mark), or why it is neither
/// JSON nor YAML.
pub(crate) fn read_schema_text(text: &str) -> Result<Value, String> {
    let json_error = match serde_json::from_str(text) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    let json_message = if json_error
        .to_string()
        .starts_with("recursion limit exceeded")
    {
        TOO_DEEP.to_string()
    } else {
        json_error.to_string()
    };
    let start = text.trim_start();
    if start.is_empty() || start.starts_with('{') || start.starts_with('[') {
        return Err(json_message);
    }
    read_yaml(text, json_message)
}

#[cfg(feature = "yaml")]
fn read_yaml(text: &str, _json_message: String) -> Result<Value, String> {
    yaml::read(text)
}

#[cfg(not(feature = "yaml"))]
fn read_yaml(_text: &str, json_message: String) -> Result<Value, String> {
    Err(format!(
        "it is no JSON ({json_message}), and a schema written in YAML needs the editor's `yaml` feature"
    ))
}

#[cfg(feature = "yaml")]
mod yaml {
    use serde_json::Value;

    use super::super::super::codec::non_finite;
    use super::super::super::language::ConfigLanguage;
    use super::super::yaml::YamlBehavior;

    /// `text` read as the editor reads a YAML document (aliases and merge
    /// keys expanded, YAML 1.2 scalars), then held as plain JSON.
    pub(super) fn read(text: &str) -> Result<Value, String> {
        if !has_content(text) {
            return Err("the YAML text holds no schema, only comments".to_string());
        }
        let value = YamlBehavior.parse(text).map_err(|error| {
            if error.message.contains("nest too deeply") {
                return super::TOO_DEEP.to_string();
            }
            match (error.line, error.column) {
                (Some(line), Some(column)) if !error.message.contains(" line ") => {
                    format!("{} at line {line} column {column}", error.message)
                }
                _ => error.message,
            }
        })?;
        plain(value, "")
    }

    /// Whether `text` holds more than comments, blank lines and document
    /// markers.
    fn has_content(text: &str) -> bool {
        text.lines().any(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('#') && line != "---" && line != "..."
        })
    }

    /// `value` as JSON holds it: a string kept as written, and a number
    /// JSON cannot hold (`.inf`, `.nan`) refused, naming where it is.
    fn plain(value: Value, at: &str) -> Result<Value, String> {
        Ok(match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, child)| {
                        let place = format!("{at}/{}", escape(&key));
                        plain(child, &place).map(|child| (key, child))
                    })
                    .collect::<Result<_, _>>()?,
            ),
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| plain(item, &format!("{at}/{index}")))
                    .collect::<Result<_, _>>()?,
            ),
            Value::String(_) if non_finite::name(&value).is_some() => {
                let name = match non_finite::name(&value) {
                    Some("-inf") => "-.inf",
                    Some("nan") => ".nan",
                    _ => ".inf",
                };
                let at = if at.is_empty() { "/" } else { at };
                return Err(format!(
                    "the value at {at} is {name}, a number JSON cannot hold: a JSON Schema holds finite numbers only"
                ));
            }
            Value::String(text) => Value::String(non_finite::written(&text).to_string()),
            other => other,
        })
    }

    /// A JSON pointer segment for `key`.
    fn escape(key: &str) -> String {
        key.replace('~', "~0").replace('/', "~1")
    }
}
