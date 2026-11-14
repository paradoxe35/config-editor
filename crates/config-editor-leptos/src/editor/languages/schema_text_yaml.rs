//! A schema's text read as YAML (see `schema_text`).

use serde_json::Value;

use super::super::super::codec::non_finite;
use super::super::super::language::{ConfigLanguage, ParseError};
use super::super::wide_integers::unheld_all;
use super::super::yaml::YamlBehavior;

/// `text` read as the editor reads a YAML document (aliases and merge
/// keys expanded, YAML 1.2 scalars), then held as plain JSON. An integer
/// past 64 bits reads as JSON reads it (a float), not as an error.
pub(super) fn read(text: &str) -> Result<Value, String> {
    if !has_content(text) {
        return Err(format!("{}, only comments", super::NO_SCHEMA));
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let value = match YamlBehavior.parse(&normalized) {
        Ok(value) => value,
        Err(error) => match as_floats(&normalized, &error) {
            Some(widened) => YamlBehavior
                .parse(&widened)
                .map_err(|error| worded(&error))?,
            None => return Err(worded(&error)),
        },
    };
    let value = super::a_schema(value)?;
    plain(value, "", false)
}

/// `text` with every integer literal past 64 bits written as the float
/// JSON reads it as, when `error` is only about such literals; `None`
/// otherwise.
fn as_floats(text: &str, error: &ParseError) -> Option<String> {
    let ranges = unheld_all(text, error);
    if ranges.is_empty() {
        return None;
    }
    let mut widened = String::with_capacity(text.len());
    let mut at = 0;
    for range in ranges {
        let literal = &text[range.clone()];
        let digits = literal.strip_prefix('-').unwrap_or(literal);
        if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let read: Value = serde_json::from_str(literal).ok()?;
        let float = read.as_f64().filter(|float| float.is_finite())?;
        widened.push_str(&text[at..range.start]);
        widened.push_str(&format!("{float:e}"));
        at = range.end;
    }
    widened.push_str(&text[at..]);
    Some(widened)
}

/// A YAML reading's error in the editor's words, its place after it as a
/// document's problem says it, with a hint where YAML reads a schema's
/// text otherwise than meant.
fn worded(error: &ParseError) -> String {
    if error.message.contains("nest too deeply") {
        return super::TOO_DEEP.to_string();
    }
    let mut message = error.message.as_str();
    // The parser's own place (`YAML parse error at line 2, column 1: …`,
    // `… at line 2, column 1`) gives way to the editor's.
    if message.starts_with("YAML parse error at line ")
        && let Some((_, rest)) = message.split_once(": ")
    {
        message = rest;
    }
    if let Some(at) = message.find(" at line ")
        && message[at + " at line ".len()..]
            .starts_with(|character: char| character.is_ascii_digit())
    {
        message = &message[..at];
    }
    // `type: duplicate key "type"`: the key is named once.
    if let Some(at) = message.find("duplicate key ") {
        message = &message[at..];
    }
    // `*:` or `*nope`: a plain `*` starts an alias.
    let message = match message.strip_prefix("unknown anchor: ") {
        Some(name) if name.trim_matches([':', ' ']).is_empty() => {
            "a plain * starts an alias in YAML: quote a key or value that starts with *".to_string()
        }
        Some(name) => format!(
            "no anchor &{name} for the alias *{name}: a plain * starts an alias in YAML, so quote a key or value that starts with *"
        ),
        None => message.to_string(),
    };
    match (error.line, error.column) {
        (Some(line), Some(column)) => super::located(&message, line, column),
        _ => message,
    }
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
/// JSON cannot hold (`.inf`, `.nan`) refused, naming where it is; a
/// `$ref` or `type` YAML reads as null (`$ref: #/…` is a comment, `null`
/// the null value) refused with how to write it.
fn plain(value: Value, at: &str, data: bool) -> Result<Value, String> {
    Ok(match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, child)| {
                    let place = format!("{at}/{}", escape(&key));
                    if !data {
                        read_as_null(&key, &child, &place)?;
                    }
                    // A value a schema holds as data (`const: {type: ~}`)
                    // may be null anywhere.
                    let data = data || DATA_KEYWORDS.contains(&key.as_str());
                    plain(child, &place, data).map(|child| (key, child))
                })
                .collect::<Result<_, _>>()?,
        ),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| plain(item, &format!("{at}/{index}"), data))
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

/// The keywords whose values are data, not schemas.
const DATA_KEYWORDS: [&str; 5] = ["const", "default", "enum", "examples", "example"];

/// Refuses a `$ref` or a `type` (or one of its list) that YAML read as
/// null, at `place`.
fn read_as_null(key: &str, child: &Value, place: &str) -> Result<(), String> {
    let null_type = |value: &Value| match value {
        Value::Null => true,
        Value::Array(items) => items.iter().any(Value::is_null),
        _ => false,
    };
    match key {
        "$ref" if child.is_null() => Err(format!(
            "the $ref at {place} is empty: in YAML an unquoted # starts a comment, so quote the reference, as in $ref: \"#/$defs/name\""
        )),
        "type" if null_type(child) => Err(format!(
            "the type at {place} reads as YAML's null: name the null type in quotes, as \"null\""
        )),
        _ => Ok(()),
    }
}

/// A JSON pointer segment for `key`.
fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
