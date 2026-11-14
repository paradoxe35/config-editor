//! A placeholder the host gave as a field's whole value stands for the
//! whole secret: nothing may be written beside it (`«secret 3f2a»Z` would
//! reach the host as a new secret holding the token's text). An edit that
//! writes text beside it in its field is refused, whatever made it and in
//! whatever language the text is shown (`whole_guard`); what gets there
//! anyway (a host's own text) is flagged where the token is, and an Edit-as
//! save holding it waits.

use std::ops::Range;

use serde_json::Value;

use super::super::diagnostics::{Diagnostic, Severity};
use super::super::language::{ConfigLanguage, Repetition};
use super::super::parse_cache;
use super::super::paths::field_range;
use super::super::work_count::{Work, note};
use super::own_value::own_value;
use super::tokens::{Known, field_value};
use crate::EditorStrings;
use crate::strings::fill;

/// The value of the field at `pointer` when the host's `text` (written in
/// `behavior`), which does not parse, gives the token at bytes `token` as
/// that field's whole value: the language finds the value written in the
/// text around the token, and writes the token, alone or with whitespace
/// around it a multi-line form holds (a block's last line break, XML text's
/// indentation), just so there. The host's own characters decide, never
/// what is typed on screen later; a value the language cannot find is not
/// taken as whole.
pub(crate) fn written_alone(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    token: Range<usize>,
) -> Option<String> {
    // The value the token is written in, also where the field's key is
    // written twice (a line moved into a block holding it).
    let (read, range) = own_value(behavior, text, pointer, &token)?;
    let written = &read[range.clone()];
    let token_text = &text[token.clone()];
    // The whitespace written right before the token inside the value's
    // written form; after it, a line break however written (an escape in
    // a quoted string), or through its line's end or all of the whitespace
    // that follows (a kept block's blank lines are past its written form).
    let before = &text[range.start..token.start];
    let leading = ["", &before[before.trim_end().len()..]];
    let after = &text[token.end..];
    let after = &after[..after.len() - after.trim_start().len()];
    let line_end = after.find('\n').map_or(after, |at| &after[..=at]);
    [&after[..0], "\n", line_end, after]
        .into_iter()
        .flat_map(|trail| {
            leading
                .iter()
                .map(move |lead| format!("{lead}{token_text}{trail}"))
        })
        .find(|value| behavior.format_value_like(&Value::String(value.clone()), written) == written)
}

/// What the field of a token the host gave whole holds of it.
#[derive(Debug, PartialEq)]
pub(super) enum Holding<'a> {
    /// Not the token (the field is gone, holds something else, or is made
    /// a list of copies of its element, see `moved_off`).
    Not,
    /// The token alone: as the host wrote it, or bare.
    Alone(&'a str),
    /// A string holding the token with other text beside it.
    Beside(&'a str),
    /// A list or map holding the token in one of its strings (an XML
    /// element given a child or an attribute, a value put in brackets).
    Within(&'a Value),
}

/// What `known`'s field (a token given whole) holds of it in `document`,
/// read in `behavior`.
pub(super) fn holding<'a>(
    behavior: &dyn ConfigLanguage,
    document: &'a Value,
    known: &Known,
) -> Holding<'a> {
    note(Work::TokenScan);
    let value = known
        .field
        .as_deref()
        .and_then(|field| field_value(document, field));
    match value {
        Some(Value::String(value)) if known.alone_in(value) => Holding::Alone(value),
        Some(Value::String(value)) if value.contains(&known.token) => Holding::Beside(value),
        Some(Value::Array(items)) if copies(behavior, items, known) => Holding::Not,
        Some(value @ (Value::Array(_) | Value::Object(_))) if holds(value, &known.token) => {
            Holding::Within(value)
        }
        _ => Holding::Not,
    }
}

/// Whether `items`, a field's value in a language that repeats an element
/// for a list, are copies of the field's element, each holding `known`'s
/// token alone (flagged and held by `moved_off`).
fn copies(behavior: &dyn ConfigLanguage, items: &[Value], known: &Known) -> bool {
    behavior.repetition() != Repetition::None
        && items.len() > 1
        && items
            .iter()
            .all(|item| item.as_str().is_some_and(|text| known.alone_in(text)))
}

/// Whether one of the strings `value` holds holds `token`.
fn holds(value: &Value, token: &str) -> bool {
    match value {
        Value::String(text) => text.contains(token),
        Value::Array(items) => items.iter().any(|item| holds(item, token)),
        Value::Object(map) => map.values().any(|item| holds(item, token)),
        _ => false,
    }
}

/// The first of `known`'s tokens the host gave as its field's whole value
/// that `document` (read in `behavior`) holds in that field with something
/// else beside it.
pub(crate) fn first_glued<'a>(
    behavior: &dyn ConfigLanguage,
    document: &Value,
    known: &'a [Known],
) -> Option<&'a Known> {
    known
        .iter()
        .find(|known| glued_in(behavior, document, known))
}

/// Whether `known` (a token given whole) has something beside it in its
/// field in `document` (read in `behavior`): text, or the list or map the
/// field became.
pub(super) fn glued_in(behavior: &dyn ConfigLanguage, document: &Value, known: &Known) -> bool {
    known.whole.is_some()
        && matches!(
            holding(behavior, document, known),
            Holding::Beside(_) | Holding::Within(_)
        )
}

/// Whether `after` is `before` with characters taken out, or the same.
pub(super) fn only_taken_from(before: &str, after: &str) -> bool {
    let mut left = before.chars();
    after
        .chars()
        .all(|character| left.any(|kept| kept == character))
}

/// The field of a token the host gave whole that the host text `new` holds
/// with other text beside it, where `old` did not: a save waiting.
pub(crate) fn glued_in_save(
    behavior: &dyn ConfigLanguage,
    known: &[Known],
    old: &str,
    new: &str,
    reading: Option<&Value>,
) -> Option<String> {
    if !known.iter().any(|known| known.whole.is_some()) {
        return None;
    }
    let after = super::super::codec::read(behavior, new, reading).ok()?;
    let before = super::super::codec::read(behavior, old, reading).ok();
    known
        .iter()
        .filter(|known| glued_in(behavior, &after, known))
        .find(|known| {
            before
                .as_ref()
                .is_none_or(|before| !glued_in(behavior, before, known))
        })
        .and_then(|known| known.field.clone())
}

/// A warning on each token of `text` the host gave whole that has text
/// beside it in its field.
pub(crate) fn glued_placeholders(
    behavior: &dyn ConfigLanguage,
    text: &str,
    known: &[Known],
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    if !known.iter().any(|known| known.whole.is_some()) {
        return Vec::new();
    }
    let parsed = parse_cache::parsed(behavior, text);
    let Some(document) = parsed.as_ref().as_ref().ok() else {
        return Vec::new();
    };
    known
        .iter()
        .filter(|known| glued_in(behavior, document, known))
        .filter_map(|known| {
            let field = known.field.as_deref()?;
            let message = glued_message(strings, field);
            Some(at_token(behavior, text, field, &known.token, message))
        })
        .collect()
}

/// A warning `message` on `token` in the field at `field` of `text`; on the
/// whole text where the language cannot say where the field is written
/// (the save waits for it all the same).
fn at_token(
    behavior: &dyn ConfigLanguage,
    text: &str,
    field: &str,
    token: &str,
    message: String,
) -> Diagnostic {
    let warning = Diagnostic {
        message,
        severity: Severity::Warning,
        ..Diagnostic::default()
    };
    let Some(range) = field_range(behavior, text, field) else {
        return warning;
    };
    let units: Vec<u16> = text.encode_utf16().collect();
    let (from, to) = (range.start.to_offset(text), range.end.to_offset(text));
    let written = String::from_utf16_lossy(&units[from.min(units.len())..to.min(units.len())]);
    let byte_from = String::from_utf16_lossy(&units[..from.min(units.len())]).len();
    let at = byte_from + written.find(token).unwrap_or(0);
    let line_start = text[..at].rfind('\n').map_or(0, |found| found + 1);
    let column = text[line_start..at].chars().count() + 1;
    Diagnostic {
        line: Some(text[..at].matches('\n').count() + 1),
        column: Some(column),
        end_column: Some(column + token.chars().count()),
        ..warning
    }
}

/// What a refusal, a warning and a waiting save say about the token in
/// the field at `pointer`.
pub(crate) fn glued_message(strings: &EditorStrings, pointer: &str) -> String {
    let place = if pointer.is_empty() { "/" } else { pointer };
    fill(&strings.glued_placeholder, &[("pointer", &place)])
}
