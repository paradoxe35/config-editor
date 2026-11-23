//! The field written over a place inside a container written on one line
//! (a YAML flow map or list, a TOML inline table or array), found by where
//! the parsed document writes its members: the caret's reading of a line
//! (`ConfigLanguage::completion_context`) names one field per line in some
//! languages, so it stops at the container.
//!
//! The work does not grow with a container's members: the member holding
//! the place is found by a binary search over where the members start
//! (they are written in order), so each container on the path costs a few
//! span lookups, however many members it has.

use serde_json::Value;

use super::super::paths::{self, FieldPath};
use super::{FieldHit, Part};
use crate::editor::language::{ConfigLanguage, SchemaPathSegment, Span};

/// Containers walked into at most.
const MAX_DEPTH: usize = 32;

/// A place of the text: one-based line and character column, compared as
/// spans place their starts.
type Place = (usize, usize);

/// What the member over the place is.
enum Member<'d> {
    /// Its key or scalar value is written over the place.
    Over(FieldHit),
    /// It is a container starting before the place: the place may be
    /// inside it.
    Inside(SchemaPathSegment, &'d Value),
}

/// The member written over character `column` of zero-based `line_index`
/// inside the container at `start` (or its deepest ancestor the document
/// has as a container): its key, or its scalar value, walking into the
/// containers holding the place. `None` when no member is written there or
/// the text does not parse.
pub(super) fn inline_field_at(
    source: &str,
    line_index: usize,
    column: usize,
    start: &[SchemaPathSegment],
    behavior: &dyn ConfigLanguage,
) -> Option<FieldHit> {
    let parsed = crate::editor::parse_cache::parsed(behavior, source);
    let document = parsed.as_ref().as_ref().ok()?;
    let place = (line_index + 1, column + 1);
    // One batch: the text's readings are made once for every lookup.
    paths::sharing_spans(behavior, source, || {
        let mut path = start.to_vec();
        let mut value = loop {
            let pointer = FieldPath::from(path.clone()).to_pointer();
            if let Some(value) = document.pointer(&pointer)
                && (value.is_object() || value.is_array())
            {
                break value;
            }
            path.pop()?;
        };
        for _ in 0..MAX_DEPTH {
            match member_at(source, place, &path, value, behavior)? {
                Member::Over(hit) => return Some(hit),
                Member::Inside(segment, member) => {
                    path.push(segment);
                    value = member;
                }
            }
        }
        None
    })
}

/// Whether `span` covers `place`.
fn over(span: &Span, (line, column): Place) -> bool {
    span.line == line && span.start_column <= column && column < span.end_column
}

/// The member of `value` (the container at `path`) holding `place`: the
/// last one starting at or before it, found by a binary search over the
/// members' starts; its key or scalar value when written over the place.
fn member_at<'d>(
    source: &str,
    place: Place,
    path: &[SchemaPathSegment],
    value: &'d Value,
    behavior: &dyn ConfigLanguage,
) -> Option<Member<'d>> {
    let members: Vec<(Option<&String>, &Value)> = match value {
        Value::Object(members) => members
            .iter()
            .map(|(key, member)| (Some(key), member))
            .collect(),
        Value::Array(items) => items.iter().map(|item| (None, item)).collect(),
        _ => return None,
    };
    let segment = |index: usize| match members[index].0 {
        Some(key) => SchemaPathSegment::Key(key.clone()),
        None => SchemaPathSegment::ArrayItem(index),
    };
    let pointer = |index: usize| {
        let mut at = path.to_vec();
        at.push(segment(index));
        FieldPath::from(at).to_pointer()
    };
    // Where a member starts: its key, or an item's value.
    let starts_by = |index: usize| {
        let pointer = pointer(index);
        let span = match members[index].0 {
            Some(_) => paths::written_key_span(behavior, source, &pointer),
            None => paths::written_value_span(behavior, source, &pointer),
        };
        // A member the text does not place is taken as before the place.
        span.is_none_or(|span| (span.line, span.start_column) <= place)
    };
    let (mut low, mut high) = (0, members.len());
    while low < high {
        let middle = low + (high - low) / 2;
        if starts_by(middle) {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let index = low.checked_sub(1)?;
    let (key, member) = members[index];
    let pointer = pointer(index);
    let hit = |key: String, span: Span, part: Part| FieldHit {
        path: {
            let mut at = path.to_vec();
            at.push(segment(index));
            at
        },
        key,
        start_column: span.start_column - 1,
        part,
    };
    if let Some(key) = key
        && let Some(span) = paths::written_key_span(behavior, source, &pointer)
        && over(&span, place)
    {
        return Some(Member::Over(hit(key.clone(), span, Part::Key)));
    }
    // A container starting before the place holds it, if any member does.
    if member.is_object() || member.is_array() {
        return Some(Member::Inside(segment(index), member));
    }
    let span =
        paths::written_value_span(behavior, source, &pointer).filter(|span| over(span, place))?;
    // An item is named by its list's key.
    let key = match key {
        Some(key) => key.clone(),
        None => super::list_key(path)?,
    };
    Some(Member::Over(hit(key, span, Part::Value)))
}
