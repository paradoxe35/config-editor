//! A placeholder the host gave as a field's whole value stands for the
//! whole secret: nothing may be written beside it (`«secret 3f2a»Z` would
//! reach the host as a new secret holding the token's text). An edit that
//! writes text beside it in its field is refused, whatever made it and in
//! whatever language the text is shown (`whole_guard`); what gets there
//! anyway (a host's own text) is flagged where the token is, and an Edit-as
//! save holding it waits.

use std::ops::Range;

use serde_json::Value;

use super::super::codec::scalar_range;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::super::paths::{field_range, written_value_span};
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
    let range = written_value_span(behavior, text, pointer)
        .and_then(|span| scalar_range(behavior, text, span))?;
    if token.start < range.start || range.end < token.end {
        return None;
    }
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

/// The first of `known`'s tokens the host gave as its field's whole value
/// that `document` holds in that field with other text beside it.
pub(crate) fn first_glued<'a>(document: &Value, known: &'a [Known]) -> Option<&'a Known> {
    known.iter().find(|known| {
        known.whole.is_some()
            && known.field.as_deref().is_some_and(|field| {
                field_value(document, field)
                    .and_then(Value::as_str)
                    .is_some_and(|value| !known.alone_in(value) && value.contains(&known.token))
            })
    })
}

/// Whether `known` (a token given whole) has text beside it in `document`.
pub(super) fn glued_in(document: &Value, known: &Known) -> bool {
    first_glued(document, std::slice::from_ref(known)).is_some()
}

/// The string `known`'s field holds in `document`.
pub(super) fn value_of<'a>(document: &'a Value, known: &Known) -> &'a str {
    known
        .field
        .as_deref()
        .and_then(|field| field_value(document, field))
        .and_then(Value::as_str)
        .unwrap_or_default()
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
        .filter(|known| glued_in(&after, known))
        .find(|known| {
            before
                .as_ref()
                .is_none_or(|before| !glued_in(before, known))
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
        .filter(|known| glued_in(document, known))
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
