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
use super::super::parse_cache::line_path;
use super::super::paths::{field_range, path_at, written_value_span};
use super::locate::within;
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
fn glued_in(document: &Value, known: &Known) -> bool {
    first_glued(document, std::slice::from_ref(known)).is_some()
}

/// The field of the token an edit from `before` to `after` (texts in
/// `behavior`, `after` parsing) writes text beside: one the host gave
/// whole, alone in its field before, or with text beside it already that
/// the edit adds to (it may only take text away). Read only when the edit
/// changes a line holding one of `tokens` (UTF-16 ranges of the real
/// placeholders in `before`), or a line of a field holding one given whole
/// (a line typed or copied inside a value written over lines). An edit
/// there making the text parse joins what it holds onto the token (a word
/// typed below it, the line break deleted): it is refused too. One made
/// elsewhere is let through, what the text holds beside a token flagged.
pub(crate) fn glued_by_edit(
    behavior: &dyn ConfigLanguage,
    known: &[Known],
    tokens: &[Range<usize>],
    before: &str,
    after: &str,
) -> Option<String> {
    if tokens.is_empty() || !known.iter().any(|known| known.whole.is_some()) {
        return None;
    }
    let lines = changed_lines(before, after);
    let on_token = || {
        tokens
            .iter()
            .any(|token| token.start <= lines.end && lines.start <= token.end)
    };
    if !on_token() && !in_whole_field(behavior, known, before, after) {
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
    let earlier = earlier.as_ref().as_ref().ok();
    glued
        .into_iter()
        .find(|known| match earlier {
            Some(earlier) => {
                !glued_in(earlier, known)
                    || !only_taken_from(value_of(earlier, known), value_of(document, known))
            }
            None => joins(behavior, known, before, after),
        })
        .and_then(|known| known.field.clone())
}

/// Whether the edit from `before` (which does not parse) to `after` is
/// made in the value of `known`'s field as `after` writes it (a line break
/// deleted between the token and a word): it joins the two. A repair made
/// elsewhere, even on the same line, leaves the text beside the token to be
/// flagged. Where the language cannot say where the value is written, any
/// edit the check reads is taken as joining.
fn joins(behavior: &dyn ConfigLanguage, known: &Known, before: &str, after: &str) -> bool {
    use super::super::text_change::changed_bytes;
    let (start, _, end) = changed_bytes(before, after);
    let written = known.field.as_deref().and_then(|field| {
        written_value_span(behavior, after, field)
            .and_then(|span| scalar_range(behavior, after, span))
    });
    written.is_none_or(|range| start <= range.end && range.start <= end)
}

/// Whether the first line the edit from `before` to `after` changes is,
/// in `after`, inside the field of a token given whole: a line of its value
/// written over lines. Its path is read once for the edit's check and the
/// locks' (see `parse_cache::line_path`).
fn in_whole_field(
    behavior: &dyn ConfigLanguage,
    known: &[Known],
    before: &str,
    after: &str,
) -> bool {
    use super::super::text_change::{changed_bytes, utf16_len};
    let (start, _, _) = changed_bytes(before, after);
    let line_start = after[..start].rfind('\n').map_or(0, |at| at + 1);
    let line = after[line_start..].split('\n').next().unwrap_or_default();
    let first = utf16_len(&after[..line_start]);
    let code = first + utf16_len(&line[..line.len() - line.trim_start().len()]);
    let read = || path_at(behavior, after, code).map(|path| path.to_pointer());
    let Some(path) = line_path(behavior, after, first, read) else {
        return false;
    };
    known
        .iter()
        .filter(|known| known.whole.is_some())
        .filter_map(|known| known.field.as_deref())
        .any(|field| within(behavior, &path, field))
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
    use super::super::text_change::{changed_bytes, utf16_len};
    // Read off the bytes: no copy of either text.
    let (start, end, _) = changed_bytes(before, after);
    let start = before[..start].rfind('\n').map_or(0, |at| at + 1);
    let end = before[end..].find('\n').map_or(before.len(), |at| end + at);
    let units = utf16_len(&before[..start]);
    units..units + utf16_len(&before[start..end])
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
