//! A schema's text read into its JSON value: JSON, or YAML (feature
//! `yaml`) when it is no JSON.
//!
//! YAML holds every JSON text with the same meaning, so a JSON text is read
//! as JSON first and keeps its exact reading; only a text JSON refuses goes
//! to YAML. A map whose first key is in double quotes (`{"type"`) was
//! meant as JSON: JSON's error is the one worth showing, and YAML is not
//! asked (it would take a trailing comma or single quotes). A flow map
//! with a plain first key (`{type: object}`) is YAML, told in YAML's words. A place in the text is said as a document's
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
        // A key written twice is refused, as YAML refuses it: JSON's
        // reader would keep the last one silently.
        Ok(value) => {
            return match serde_json::from_str::<NoKeyTwice>(text) {
                Ok(NoKeyTwice) => a_schema(value),
                Err(error) => Err(json_message(&error)),
            };
        }
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
        // A map whose first key is written as JSON writes it (`{"type"`)
        // is JSON, a trailing comma or a single-quoted value in it no
        // exception; a flow map YAML reads (`{type: object}`) is YAML, as
        // it says, and YAML's own words tell what is wrong in it.
        if json_key_first(start) {
            return Err(json_message);
        }
        return read_flow_yaml(text, json_message).and_then(a_schema);
    }
    read_yaml(text, json_message)
}

/// Whether `text` (starting with `{` or `[`) opens a map whose first key
/// is in double quotes, as JSON writes every key.
fn json_key_first(text: &str) -> bool {
    text.strip_prefix('{')
        .is_some_and(|rest| rest.trim_start().starts_with('"'))
}

/// A JSON text with no map holding a key twice (read only to tell).
struct NoKeyTwice;

impl<'de> serde::Deserialize<'de> for NoKeyTwice {
    fn deserialize<D: serde::Deserializer<'de>>(reader: D) -> Result<Self, D::Error> {
        reader.deserialize_any(NoKeyTwiceVisitor)
    }
}

struct NoKeyTwiceVisitor;

impl<'de> serde::de::Visitor<'de> for NoKeyTwiceVisitor {
    type Value = NoKeyTwice;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, _: bool) -> Result<NoKeyTwice, E> {
        Ok(NoKeyTwice)
    }

    fn visit_i64<E>(self, _: i64) -> Result<NoKeyTwice, E> {
        Ok(NoKeyTwice)
    }

    fn visit_u64<E>(self, _: u64) -> Result<NoKeyTwice, E> {
        Ok(NoKeyTwice)
    }

    fn visit_f64<E>(self, _: f64) -> Result<NoKeyTwice, E> {
        Ok(NoKeyTwice)
    }

    fn visit_str<E>(self, _: &str) -> Result<NoKeyTwice, E> {
        Ok(NoKeyTwice)
    }

    fn visit_unit<E>(self) -> Result<NoKeyTwice, E> {
        Ok(NoKeyTwice)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut items: A) -> Result<NoKeyTwice, A::Error> {
        while items.next_element::<NoKeyTwice>()?.is_some() {}
        Ok(NoKeyTwice)
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<NoKeyTwice, A::Error> {
        let mut seen = std::collections::HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if seen.contains(&key) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate key {}",
                    Value::from(key)
                )));
            }
            map.next_value::<NoKeyTwice>()?;
            seen.insert(key);
        }
        Ok(NoKeyTwice)
    }
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
    let bare = plain_json_words(bare);
    if line == 0 {
        bare.to_string()
    } else {
        located(bare, line, column.max(1))
    }
}

/// The JSON reader's words for a text that ends early or goes on past
/// its end, in plain words saying what is missing.
fn plain_json_words(message: &str) -> &str {
    match message {
        "EOF while parsing a list" => "the text ends before the list is closed: a ] is missing",
        "EOF while parsing an object" => "the text ends before the map is closed: a } is missing",
        "EOF while parsing a string" => "the text ends inside a string: a closing \" is missing",
        "EOF while parsing a value" => "the text ends where a value should follow",
        "trailing characters" => "more text follows the end of the schema",
        other => other,
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

/// A text starting with `{` or `[` read as YAML: YAML's error for a map
/// whose first key is plain (`{type: object, type: string}`), else JSON's
/// (`[1, 2` or `{`, meant as JSON).
#[cfg(feature = "yaml")]
fn read_flow_yaml(text: &str, json_message: String) -> Result<Value, String> {
    let plain_key = text
        .trim_start()
        .strip_prefix('{')
        .and_then(|rest| rest.trim_start().chars().next())
        .is_some_and(|first| first.is_alphanumeric() || first == '_' || first == '$');
    yaml::read(text).map_err(|yaml_message| {
        if plain_key {
            yaml_message
        } else {
            json_message
        }
    })
}

#[cfg(not(feature = "yaml"))]
fn read_yaml(_text: &str, json_message: String) -> Result<Value, String> {
    Err(format!(
        "it is no JSON ({json_message}), and a schema written in YAML needs the editor's `yaml` feature"
    ))
}

#[cfg(not(feature = "yaml"))]
fn read_flow_yaml(_text: &str, json_message: String) -> Result<Value, String> {
    Err(json_message)
}

#[cfg(feature = "yaml")]
#[path = "schema_text_yaml.rs"]
mod yaml;
