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
use super::super::language::{ConfigLanguage, Repetition};
use super::super::parse_cache;
use super::super::paths::{field_range, written_value_span};
use super::super::text::{slice, text_size};
use super::super::work_count::{Work, note};
use super::holder::{Holder, Holders};
use super::locate::LineIndex;
use super::tokens::{Known, SecretMatcher, field_value};
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
    // The value the language finds written for the field, holding it.
    let range = written_value_span(behavior, text, pointer)
        .and_then(|span| scalar_range(behavior, text, span))
        .filter(|range| range.start <= token.start && token.end <= range.end)?;
    let written = text.get(range.clone()).unwrap_or_default();
    let token_text = text.get(token.clone()).unwrap_or_default();
    // The whitespace written right before the token inside the value's
    // written form; after it, a line break however written (an escape in
    // a quoted string), or through its line's end or all of the whitespace
    // that follows (a kept block's blank lines are past its written form).
    let before = text.get(range.start..token.start).unwrap_or_default();
    let leading = [
        "",
        before.get(before.trim_end().len()..).unwrap_or_default(),
    ];
    let after = text.get(token.end..).unwrap_or_default();
    let after = after
        .get(..after.len() - after.trim_start().len())
        .unwrap_or_default();
    let line_end = after
        .find('\n')
        .map_or(after, |at| after.get(..=at).unwrap_or_default());
    [after.get(..0).unwrap_or_default(), "\n", line_end, after]
        .into_iter()
        .flat_map(|trail| {
            leading
                .iter()
                .map(move |lead| format!("{lead}{token_text}{trail}"))
        })
        .find(|value| behavior.write_value_like(&Value::String(value.clone()), written) == written)
}

/// What the field of a token the host gave whole holds of it.
#[derive(Debug, PartialEq)]
pub(super) enum Holding<'a> {
    /// Not the token (the field is gone, holds something else, or is made
    /// a list of copies of its element, see `moved_off`).
    Not,
    /// The token alone: as the host wrote it, or bare.
    Alone(&'a str),
    /// A string holding the token with other text beside it: the field's,
    /// or the one a parent became with the field gone (see `swallowed`).
    Beside(&'a str),
    /// A list or map holding the token in one of its strings (an XML
    /// element given a child or an attribute, a value put in brackets).
    Within(&'a Value),
}

/// What `known`'s field (a token given whole) holds of it, read off the
/// scalars holding each place its token is written (`holders`, of a text
/// whose parse is `document`, read in `behavior`; see `holder`): the
/// field's own value, a copy of it (in a language that repeats a key or
/// an element for a list, each copy written where the field's element is
/// written again, its parents' too), or a string inside a list or map the
/// field became.
pub(super) fn holding<'a>(
    behavior: &dyn ConfigLanguage,
    document: &'a Value,
    holders: &Holders<'a>,
    known: &Known,
) -> Holding<'a> {
    note(Work::TokenScan);
    let Some(field) = known.field.as_deref() else {
        return Holding::Not;
    };
    let repetition = behavior.repetition();
    let places: Vec<&Option<Holder<'a>>> = holders.of(&known.token).collect();
    // A language that cannot say where a value is written: the field's own
    // value, as the parse reads it there.
    if !places.is_empty() && places.iter().all(|place| place.is_none()) {
        return match field_value(behavior, document, field) {
            Some(Value::String(text)) if known.alone_in(text) => Holding::Alone(text),
            Some(Value::String(text)) if text.contains(&known.token) => Holding::Beside(text),
            Some(_) => Holding::Not,
            None => {
                swallowed(behavior, document, field, known).map_or(Holding::Not, Holding::Beside)
            }
        };
    }
    // The strings holding it as the field's value or a copy of it (with
    // whether a list step sets it apart), and whether one is inside it.
    let mut own: Vec<(&'a str, bool)> = Vec::new();
    let mut within = false;
    // Whether the field itself became a list (an item of it holds the
    // token), not a parent written again.
    let mut listed = false;
    // Each string once: copies written where one value is read (a map
    // written twice under one key, the parse keeping the last) are held by
    // that one string, never a list the field became.
    let mut seen = std::collections::HashSet::new();
    for holder in places.iter().copied().flatten() {
        if !seen.insert(holder.pointer.as_str()) {
            continue;
        }
        match relation(&holder.pointer, field, repetition) {
            Some(Relation::Own) => own.push((holder.text, false)),
            Some(Relation::Copy) => own.push((holder.text, true)),
            Some(Relation::Item) => {
                own.push((holder.text, true));
                listed = true;
            }
            Some(Relation::Inside) => within = true,
            None => {}
        }
    }
    let container = || field_value(behavior, document, field).unwrap_or(&Value::Null);
    let copies = own.len() > 1 || own.iter().any(|(_, indexed)| *indexed);
    if within || (copies && repetition == Repetition::None) {
        return Holding::Within(container());
    }
    if copies {
        // Copies each holding the token alone: flagged and held by
        // `moved_off`.
        if own.len() > 1 && own.iter().all(|(text, _)| known.alone_in(text)) {
            return Holding::Not;
        }
        // The field made a list (a second element of its name written
        // beside its own), in any language: the token was its whole value,
        // so the list's other items are written beside it, an empty one
        // too (`<password/>`, as YAML's `[«secret», ""]`). Items holding
        // the token alone are none. In a language that repeats only a key
        // for a list (not any element), every copy is such an item.
        let other_items = || {
            matches!(container(), Value::Array(items) if items.iter().any(|item| {
                !matches!(item, Value::String(text) if known.alone_in(text))
            }))
        };
        if (listed && other_items()) || repetition != Repetition::Elements {
            return Holding::Within(container());
        }
        // One with text beside the token: glued, whichever copy it is (a
        // parent's element written again holds a copy of the field).
        if let Some((beside, _)) = own.iter().find(|(text, _)| !known.alone_in(text)) {
            return Holding::Beside(beside);
        }
        return own
            .first()
            .map_or(Holding::Not, |(text, _)| Holding::Alone(text));
    }
    match own.first() {
        Some((text, _)) if known.alone_in(text) => Holding::Alone(text),
        Some((text, _)) => Holding::Beside(text),
        None => swallowed(behavior, document, field, known).map_or(Holding::Not, Holding::Beside),
    }
}

/// The string a parent of `field` (gone from `document`) became, when it
/// holds `known`'s token with other text: an edit lines away from the token
/// that made the field's list or map one scalar running over its lines (a
/// YAML item's `- ` typed into `-=`, a `|` typed after the parent's key).
/// No reading relates that string to the field, yet the host would hear
/// the token glued in its parent's value.
fn swallowed<'a>(
    behavior: &dyn ConfigLanguage,
    document: &'a Value,
    field: &str,
    known: &Known,
) -> Option<&'a str> {
    if field_value(behavior, document, field).is_some() {
        return None;
    }
    let mut parent = field;
    while let Some(cut) = parent.rfind('/') {
        parent = parent.get(..cut).unwrap_or_default();
        match field_value(behavior, document, parent) {
            Some(Value::String(text)) => {
                return (text.contains(&known.token) && !known.alone_in(text))
                    .then_some(text.as_str());
            }
            Some(_) => return None,
            None => {}
        }
    }
    None
}

/// How a string at `holder` stands to the field at `field`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Relation {
    /// The field's value.
    Own,
    /// A copy of it: a parent's element written again, in a language
    /// that repeats any element for a list.
    Copy,
    /// An item of the list the field became (its own element, or key,
    /// written again).
    Item,
    /// A string inside a map the field became.
    Inside,
}

/// The relation of the string at `holder` to the field at `field` (see
/// [`Relation`]); `None`: another field's. Where a language repeats a key
/// or an element for a list, a lone item's `/0` may be left out; where it
/// repeats any element, a list step the field does not name may stand
/// anywhere on the way (a parent's element written again).
pub(super) fn relation(holder: &str, field: &str, repetition: Repetition) -> Option<Relation> {
    let repeats = repetition != Repetition::None;
    let index = |segment: &str| !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit());
    let (held, wanted): (Vec<&str>, Vec<&str>) = (
        holder.split('/').skip(1).collect(),
        field.split('/').skip(1).collect(),
    );
    let (mut at, mut inserted) = (0, false);
    for segment in &wanted {
        // A list step the field names is that item, or a lone item's
        // `/0` left out.
        if index(segment) {
            if held.get(at) == Some(segment) {
                at += 1;
            } else if !(repeats && *segment == "0" && !held.get(at).is_some_and(|step| index(step)))
            {
                return None;
            }
            continue;
        }
        while repetition == Repetition::Elements && held.get(at).is_some_and(|step| index(step)) {
            at += 1;
            inserted = true;
        }
        if held.get(at) != Some(segment) {
            return None;
        }
        at += 1;
    }
    let rest = &held[at..];
    Some(if rest.iter().any(|step| !index(step)) {
        Relation::Inside
    } else if !rest.is_empty() {
        Relation::Item
    } else if inserted {
        Relation::Copy
    } else {
        Relation::Own
    })
}

/// The first of `known`'s tokens the host gave as its field's whole value
/// that `document` (read in `behavior`) holds in that field with something
/// else beside it.
pub(crate) fn first_glued<'a>(
    behavior: &dyn ConfigLanguage,
    (text, document): (&str, &Value),
    known: &'a [Known],
    matcher: &SecretMatcher,
) -> Option<&'a Known> {
    if alone_in_fields(behavior, document, known, Form::Any) {
        return None;
    }
    let holders = Holders::read(behavior, text, document, matcher);
    known
        .iter()
        .find(|known| glued_in(behavior, document, &holders, known))
}

/// The first token the host gave whole (among `known`) that `document`
/// writes with other text in a value outside its own field: the key
/// holding it renamed (it is plain text there) while a key or a mark typed
/// beside it in a text that did not parse made the parse read them into
/// one value. What reaches the host would read the placeholder glued.
pub(crate) fn glued_elsewhere<'a>(document: &Value, known: &'a [Known]) -> Option<&'a Known> {
    let whole: Vec<&Known> = known.iter().filter(|known| known.whole.is_some()).collect();
    if whole.is_empty() {
        return None;
    }
    let mut found = None;
    walk_strings(document, &mut |leaf| {
        if found.is_none() {
            found = whole
                .iter()
                .find(|known| leaf.contains(known.token.as_str()) && !known.alone_in(leaf))
                .copied();
        }
    });
    found
}

/// Calls `visit` with every string of `value`, keys left out.
fn walk_strings(value: &Value, visit: &mut dyn FnMut(&str)) {
    match value {
        Value::String(text) => visit(text),
        Value::Array(items) => items.iter().for_each(|item| walk_strings(item, visit)),
        Value::Object(map) => map.values().for_each(|item| walk_strings(item, visit)),
        _ => {}
    }
}

/// How a token given whole stands alone in its field, for
/// [`alone_in_fields`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Form {
    /// As the host wrote it, or bare.
    Any,
    /// As the host wrote it.
    Host,
}

/// Whether each token given whole among `known` (with a field) is the
/// string `document` holds at its field, alone (in `form`). Then
/// [`holding`] reads it as alone in that string or as not held, never as
/// glued, so the holder query (a read of the span of every string holding
/// a placeholder) is not needed to tell that nothing is glued. A string at
/// the field's own pointer has no value under it, and no other string of
/// the document relates to the field (a copy or an item of it would make
/// the field, or a parent, a list), so it is the only holder [`holding`]
/// can find for the field. One lookup for each token, no read of the text:
/// an edit leaving every field as it was (typing in a comment) and a check
/// of a text gluing nothing stay cheap.
pub(super) fn alone_in_fields<'k>(
    behavior: &dyn ConfigLanguage,
    document: &Value,
    known: impl IntoIterator<Item = &'k Known>,
    form: Form,
) -> bool {
    known
        .into_iter()
        .filter(|known| known.whole.is_some())
        .all(|known| {
            note(Work::TokenScan);
            let Some(field) = known.field.as_deref() else {
                return true;
            };
            match field_value(behavior, document, field) {
                Some(Value::String(text)) => {
                    known.alone_in(text)
                        && (form == Form::Any || known.whole.as_deref() == Some(text.as_str()))
                }
                _ => false,
            }
        })
}

/// Whether `known` (a token given whole) has something beside it in its
/// field in `document` (read in `behavior`): text, or the list or map the
/// field became.
pub(super) fn glued_in(
    behavior: &dyn ConfigLanguage,
    document: &Value,
    holders: &Holders<'_>,
    known: &Known,
) -> bool {
    known.whole.is_some()
        && matches!(
            holding(behavior, document, holders, known),
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
    (known, matcher): (&[Known], &SecretMatcher),
    old: &str,
    new: &str,
    reading: Option<&Value>,
) -> Option<String> {
    if !known.iter().any(|known| known.whole.is_some()) {
        return None;
    }
    let after = super::super::codec::read(behavior, new, reading).ok()?;
    if alone_in_fields(behavior, &after, known, Form::Any) {
        return None;
    }
    let before = super::super::codec::read(behavior, old, reading).ok();
    let holders_after = Holders::read(behavior, new, &after, matcher);
    let holders_before = before
        .as_ref()
        .map(|before| Holders::read(behavior, old, before, matcher));
    known
        .iter()
        .filter(|known| glued_in(behavior, &after, &holders_after, known))
        .find(|known| {
            before
                .as_ref()
                .zip(holders_before.as_ref())
                .is_none_or(|(before, holders)| !glued_in(behavior, before, holders, known))
        })
        .and_then(|known| known.field.clone())
}

/// A warning on each token of `text` the host gave whole that has text
/// beside it in its field.
pub(crate) fn glued_placeholders(
    behavior: &dyn ConfigLanguage,
    text: &str,
    (known, matcher): (&[Known], &SecretMatcher),
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    if !known.iter().any(|known| known.whole.is_some()) {
        return Vec::new();
    }
    let parsed = parse_cache::parsed(behavior, text);
    let Some(document) = parsed.as_ref().as_ref().ok() else {
        return Vec::new();
    };
    if alone_in_fields(behavior, document, known, Form::Any) {
        return Vec::new();
    }
    let holders = Holders::read(behavior, text, document, matcher);
    known
        .iter()
        .filter(|known| glued_in(behavior, document, &holders, known))
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
    let range = LineIndex::new(text).range(range);
    let found = slice::slice(text, range)
        .and_then(|written| written.find(token))
        .unwrap_or(0);
    let head = slice::before(text, range.start() + text_size(found)).unwrap_or_default();
    let line_start = head.rfind('\n').map_or(0, |found| found + 1);
    let column = head.get(line_start..).unwrap_or_default().chars().count() + 1;
    Diagnostic {
        line: Some(head.matches('\n').count() + 1),
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
