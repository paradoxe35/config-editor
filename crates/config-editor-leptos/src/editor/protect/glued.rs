//! A placeholder the host gave as a field's whole value stands for the
//! whole secret: nothing may be written beside it (`«secret 3f2a»Z` would
//! reach the host as a new secret holding the token's text). An edit that
//! writes text beside it in its field is refused, whatever made it and in
//! whatever language the text is shown; what gets there anyway (the text
//! did not parse while it was written) is flagged where the token is, and
//! an Edit-as save holding it waits.

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

/// Whether the host's `text` (written in `behavior`), which does not parse,
/// gives the token at bytes `token` as the whole value of the field at
/// `pointer`: the language finds that value written in the text around
/// the token, and writes the token alone just so there. The host's own
/// characters decide, never what is typed on screen later; a value the
/// language cannot find is not taken as whole.
pub(crate) fn written_alone(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    token: Range<usize>,
) -> bool {
    let Some(range) = written_value_span(behavior, text, pointer)
        .and_then(|span| scalar_range(behavior, text, span))
    else {
        return false;
    };
    let written = &text[range.clone()];
    range.start <= token.start
        && token.end <= range.end
        && behavior.format_value_like(&Value::String(text[token].to_string()), written) == written
}

/// The first of `known`'s tokens the host gave as its field's whole value
/// that `document` holds in that field with other text beside it.
pub(crate) fn first_glued<'a>(document: &Value, known: &'a [Known]) -> Option<&'a Known> {
    known.iter().find(|known| {
        known.whole
            && known.field.as_deref().is_some_and(|field| {
                field_value(document, field)
                    .and_then(Value::as_str)
                    .is_some_and(|value| value != known.token && value.contains(&known.token))
            })
    })
}

/// Whether `known` (a token given whole) has text beside it in `document`.
fn glued_in(document: &Value, known: &Known) -> bool {
    first_glued(document, std::slice::from_ref(known)).is_some()
}

/// The field of the token an edit from `before` to `after` (texts in
/// `behavior`, both parsing) writes text beside: one the host gave whole,
/// alone in its field before, or with text beside it already that the edit
/// adds to (it may only take text away). An edit that makes the text parse
/// is let through: what it holds beside a token is flagged instead. Read
/// only when the edit changes a line holding one of `tokens` (UTF-16
/// ranges of the real placeholders in `before`); a value written over
/// several lines is left to the problems and the save.
pub(crate) fn glued_by_edit(
    behavior: &dyn ConfigLanguage,
    known: &[Known],
    tokens: &[Range<usize>],
    before: &str,
    after: &str,
) -> Option<String> {
    if tokens.is_empty() || !known.iter().any(|known| known.whole) {
        return None;
    }
    let lines = changed_lines(before, after);
    if !tokens
        .iter()
        .any(|token| token.start <= lines.end && lines.start <= token.end)
    {
        return None;
    }
    let parsed = parse_cache::parsed(behavior, after);
    let document = parsed.as_ref().as_ref().ok()?;
    let glued: Vec<&Known> = known
        .iter()
        .filter(|known| glued_in(document, known))
        .collect();
    if glued.is_empty() {
        return None;
    }
    let earlier = parse_cache::parsed(behavior, before);
    let earlier = earlier.as_ref().as_ref().ok()?;
    glued
        .into_iter()
        .find(|known| {
            !glued_in(earlier, known)
                || !only_taken_from(value_of(earlier, known), value_of(document, known))
        })
        .and_then(|known| known.field.clone())
}

/// The string `known`'s field holds in `document`.
fn value_of<'a>(document: &'a Value, known: &Known) -> &'a str {
    known
        .field
        .as_deref()
        .and_then(|field| field_value(document, field))
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// Whether `after` is `before` with characters taken out, or the same.
fn only_taken_from(before: &str, after: &str) -> bool {
    let mut left = before.chars();
    after
        .chars()
        .all(|character| left.any(|kept| kept == character))
}

/// The UTF-16 range of `before`, from the start of the first line an edit
/// to `after` changes to the end of the last.
fn changed_lines(before: &str, after: &str) -> Range<usize> {
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let end = old.len() - suffix;
    let newline = u16::from(b'\n');
    let start = old[..prefix]
        .iter()
        .rposition(|unit| *unit == newline)
        .map_or(0, |at| at + 1);
    let end = old[end..]
        .iter()
        .position(|unit| *unit == newline)
        .map_or(old.len(), |at| end + at);
    start..end
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
    if !known.iter().any(|known| known.whole) {
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
    if !known.iter().any(|known| known.whole) {
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
            let range = field_range(behavior, text, field)?;
            let units: Vec<u16> = text.encode_utf16().collect();
            let (from, to) = (range.start.to_offset(text), range.end.to_offset(text));
            let written =
                String::from_utf16_lossy(&units[from.min(units.len())..to.min(units.len())]);
            let byte_from = String::from_utf16_lossy(&units[..from.min(units.len())]).len();
            let at = byte_from + written.find(&known.token).unwrap_or(0);
            let line_start = text[..at].rfind('\n').map_or(0, |found| found + 1);
            let column = text[line_start..at].chars().count() + 1;
            Some(Diagnostic {
                line: Some(text[..at].matches('\n').count() + 1),
                column: Some(column),
                end_column: Some(column + known.token.chars().count()),
                message: glued_message(strings, field),
                severity: Severity::Warning,
                ..Diagnostic::default()
            })
        })
        .collect()
}

/// What a refusal, a warning and a waiting save say about the token in
/// the field at `pointer`.
pub(crate) fn glued_message(strings: &EditorStrings, pointer: &str) -> String {
    let place = if pointer.is_empty() { "/" } else { pointer };
    fill(&strings.glued_placeholder, &[("pointer", &place)])
}
