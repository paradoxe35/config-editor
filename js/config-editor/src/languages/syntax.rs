//! The members of a JavaScript language definition that describe values
//! written on one line and list items of their own lines (`mapBrackets`,
//! `listBrackets`, `itemSeparator`, `itemMarker`), strings (`literalQuotes`)
//! and how a new entry writes its separator (`writtenSeparator`).

use js_sys::Array;
use wasm_bindgen::{JsError, JsValue};

use super::{field, interned};

/// The one character `text` is.
fn single(text: Option<String>) -> Option<char> {
    let text = text?;
    let mut characters = text.chars();
    let character = characters.next()?;
    characters.next().is_none().then_some(character)
}

/// `[open, close]`, two single characters.
pub(super) fn pair(value: &JsValue) -> Option<(char, char)> {
    if !Array::is_array(value) {
        return None;
    }
    let pair = Array::from(value);
    if pair.length() != 2 {
        return None;
    }
    Some((
        single(pair.get(0).as_string())?,
        single(pair.get(1).as_string())?,
    ))
}

fn absent(value: &JsValue) -> bool {
    value.is_undefined() || value.is_null()
}

/// `definition[name]` as an `[open, close]` pair, if given.
pub(super) fn bracket_member(
    definition: &JsValue,
    name: &str,
) -> Result<Option<(char, char)>, JsError> {
    let value = field(definition, name);
    if absent(&value) {
        return Ok(None);
    }
    pair(&value).map(Some).ok_or_else(|| {
        JsError::new(&format!(
            "{name} must be an [open, close] pair of single characters, e.g. [\"[\", \"]\"]"
        ))
    })
}

/// `itemSeparator`, one character, if given.
pub(super) fn item_separator(definition: &JsValue) -> Result<Option<char>, JsError> {
    let value = field(definition, "itemSeparator");
    if absent(&value) {
        return Ok(None);
    }
    single(value.as_string())
        .map(Some)
        .ok_or_else(|| JsError::new("itemSeparator must be one character, e.g. \",\""))
}

/// `itemMarker`, a non-empty string without whitespace, if given.
pub(super) fn item_marker(definition: &JsValue) -> Result<Option<&'static str>, JsError> {
    let value = field(definition, "itemMarker");
    if absent(&value) {
        return Ok(None);
    }
    match value.as_string() {
        Some(marker) if !marker.is_empty() && !marker.contains(char::is_whitespace) => {
            Ok(Some(interned::text(&marker)))
        }
        _ => Err(JsError::new(
            "itemMarker must be a non-empty string without spaces, e.g. \"-\"",
        )),
    }
}

/// `writtenSeparator`: a key's separator as a new entry writes it, holding
/// one of `separators`, if given.
pub(super) fn written_separator(
    definition: &JsValue,
    separators: &[&str],
) -> Result<Option<&'static str>, JsError> {
    let value = field(definition, "writtenSeparator");
    if absent(&value) {
        return Ok(None);
    }
    match value.as_string() {
        Some(written)
            if separators
                .iter()
                .any(|separator| written.trim() == *separator) =>
        {
            Ok(Some(interned::text(&written)))
        }
        _ => Err(JsError::new(
            "writtenSeparator must be one of separators with its spacing, e.g. \": \"",
        )),
    }
}

/// `literalQuotes`: some of `quotes`, the ones a backslash does not escape in.
pub(super) fn literal_quotes(
    definition: &JsValue,
    quotes: &'static [&'static str],
) -> Result<&'static [&'static str], JsError> {
    let value = field(definition, "literalQuotes");
    if absent(&value) {
        return Ok(&[]);
    }
    let problem = || {
        JsError::new(&format!(
            "literalQuotes must be an array of the quotes {}, e.g. [\"'\"]",
            quotes.join(" ")
        ))
    };
    if !Array::is_array(&value) {
        return Err(problem());
    }
    let mut literal = Vec::new();
    for item in Array::from(&value).iter() {
        let text = item.as_string().ok_or_else(problem)?;
        let known = quotes
            .iter()
            .find(|quote| **quote == text)
            .ok_or_else(problem)?;
        literal.push(*known);
    }
    Ok(interned::texts(&literal))
}
