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
    let range = own_value(behavior, text, pointer, &token)?;
    let written = &text[range.clone()];
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
    let Some(field) = known.field.as_deref() else {
        return Holding::Not;
    };
    match field_value(document, field) {
        Some(Value::String(value)) if known.alone_in(value) => Holding::Alone(value),
        Some(Value::String(value)) if value.contains(&known.token) => Holding::Beside(value),
        Some(value @ Value::Array(items)) => {
            let items: Vec<&Value> = items.iter().collect();
            copied(behavior, &items, known).unwrap_or_else(|| {
                if holds(value, &known.token) {
                    Holding::Within(value)
                } else {
                    Holding::Not
                }
            })
        }
        Some(value @ Value::Object(_)) if holds(value, &known.token) => Holding::Within(value),
        Some(_) => Holding::Not,
        // The element holding the field written more than once (an
        // attribute's element copied, in a language that repeats an
        // element for a list): the field's value in each copy.
        None => through_copies(behavior, document, field)
            .and_then(|items| copied(behavior, &items, known))
            .unwrap_or(Holding::Not),
    }
}

/// What `items`, the field's value in each copy of an element (in a
/// language that repeats an element for a list), hold of `known`'s token;
/// none when they are no such copies.
fn copied<'a>(
    behavior: &dyn ConfigLanguage,
    items: &[&'a Value],
    known: &Known,
) -> Option<Holding<'a>> {
    // Copies each holding the token alone: flagged and held by `moved_off`.
    if behavior.repetition() != Repetition::None
        && items.len() > 1
        && items
            .iter()
            .all(|item| item.as_str().is_some_and(|text| known.alone_in(text)))
    {
        return Some(Holding::Not);
    }
    // In a language that repeats any element for a list, a string item is
    // one written as an element of its own, never one written in a list.
    if behavior.repetition() != Repetition::Elements {
        return None;
    }
    let strings = || items.iter().filter_map(|item| item.as_str());
    // One with text beside the token: glued, whichever copy it is.
    if let Some(beside) =
        strings().find(|text| text.contains(&known.token) && !known.alone_in(text))
    {
        return Some(Holding::Beside(beside));
    }
    // A second element of the field's name written beside its own: a new
    // item, the token still an item's whole value.
    strings()
        .find(|text| known.alone_in(text))
        .map(Holding::Alone)
}

/// In a language that repeats any element for a list, the values at
/// `pointer` in `document` read through each list on the way whose item
/// the pointer does not name (an element holding the field written more
/// than once); none when no such list is on the way.
fn through_copies<'a>(
    behavior: &dyn ConfigLanguage,
    document: &'a Value,
    pointer: &str,
) -> Option<Vec<&'a Value>> {
    if behavior.repetition() != Repetition::Elements {
        return None;
    }
    let mut at = vec![document];
    let mut fanned = false;
    for segment in pointer.split('/').skip(1) {
        let key = segment.replace("~1", "/").replace("~0", "~");
        let mut next = Vec::with_capacity(at.len());
        for value in at {
            match value {
                Value::Object(map) => next.extend(map.get(&key)),
                Value::Array(items) => match key.parse::<usize>() {
                    Ok(index) => next.extend(items.get(index)),
                    Err(_) => {
                        fanned = true;
                        next.extend(items.iter().filter_map(|item| item.get(&key)));
                    }
                },
                _ => {}
            }
        }
        at = next;
    }
    (fanned && !at.is_empty()).then_some(at)
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
