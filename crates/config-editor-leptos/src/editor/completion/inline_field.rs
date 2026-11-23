//! The field written over a place inside a container written on one line
//! (a YAML flow map or list, a TOML inline table or array), found by where
//! the parsed document writes its members: the caret's reading of a line
//! (`ConfigLanguage::completion_context`) names one field per line in some
//! languages, so it stops at the container.

use serde_json::Value;

use super::super::paths::{self, FieldPath};
use super::{FieldHit, Part};
use crate::editor::language::{ConfigLanguage, SchemaPathSegment, Span};

/// Containers walked into at most, one level per container on the line.
const MAX_DEPTH: usize = 32;

/// What a container's member written on the line is to the place.
enum Member<'d> {
    /// Its key or scalar value is written over the place.
    Over(FieldHit),
    /// It is a container written on the line, starting at or before the
    /// place: the place may be inside it.
    Inside(SchemaPathSegment, &'d Value),
}

/// The member written over character `column` of zero-based `line_index`
/// of the container at `start`, or of its deepest ancestor the document
/// has as a container: its key, or its scalar value, walking into the
/// containers written on that line. `None` when no member is written there
/// or the text does not parse.
pub(super) fn inline_field_at(
    source: &str,
    line_index: usize,
    column: usize,
    start: &[SchemaPathSegment],
    behavior: &dyn ConfigLanguage,
) -> Option<FieldHit> {
    let parsed = crate::editor::parse_cache::parsed(behavior, source);
    let document = parsed.as_ref().as_ref().ok()?;
    let mut path = start.to_vec();
    let mut value = loop {
        if let Some(value) = document.pointer(&FieldPath::from(path.clone()).to_pointer())
            && (value.is_object() || value.is_array())
        {
            break value;
        }
        path.pop()?;
    };
    for _ in 0..MAX_DEPTH {
        match member_at(source, line_index, column, &path, value, behavior)? {
            Member::Over(hit) => return Some(hit),
            Member::Inside(segment, member) => {
                path.push(segment);
                value = member;
            }
        }
    }
    None
}

/// The member of `value` (at `path`) written over the place, or the
/// container member the place may be inside (the last starting at or
/// before it on the line).
fn member_at<'d>(
    source: &str,
    line_index: usize,
    column: usize,
    path: &[SchemaPathSegment],
    value: &'d Value,
    behavior: &dyn ConfigLanguage,
) -> Option<Member<'d>> {
    let members: Vec<(SchemaPathSegment, &Value)> = match value {
        Value::Object(members) => members
            .iter()
            .map(|(key, member)| (SchemaPathSegment::Key(key.clone()), member))
            .collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (SchemaPathSegment::ArrayItem(index), item))
            .collect(),
        _ => return None,
    };
    let child = |segment: &SchemaPathSegment| {
        let mut at = path.to_vec();
        at.push(segment.clone());
        at
    };
    let pointers: Vec<String> = members
        .iter()
        .map(|(segment, _)| FieldPath::from(child(segment)).to_pointer())
        .collect();
    let on_line = |span: &Span| span.line == line_index + 1;
    let over = |span: &Span| {
        on_line(span) && span.start_column <= column + 1 && column + 1 < span.end_column
    };
    let hit = |segment: &SchemaPathSegment, key: String, span: Span, part: Part| {
        Some(FieldHit {
            path: child(segment),
            key,
            start_column: span.start_column - 1,
            part,
        })
    };
    // Every member's spans read at once: a container of many members costs
    // one read of the text, not one per member.
    paths::with_spans(behavior, source, &pointers, || {
        let mut inside = None;
        for ((segment, member), pointer) in members.iter().zip(&pointers) {
            if let SchemaPathSegment::Key(key) = segment
                && let Some(span) = paths::written_key_span(behavior, source, pointer).filter(over)
            {
                return hit(segment, key.clone(), span, Part::Key).map(Member::Over);
            }
            let Some(span) = paths::written_value_span(behavior, source, pointer).filter(on_line)
            else {
                continue;
            };
            if member.is_object() || member.is_array() {
                if span.start_column <= column + 1 {
                    inside = Some(Member::Inside(segment.clone(), member));
                }
            } else if over(&span) {
                // An item is named by its list's key.
                let key = match segment {
                    SchemaPathSegment::Key(key) => key.clone(),
                    SchemaPathSegment::ArrayItem(_) => super::list_key(path)?,
                };
                return hit(segment, key, span, Part::Value).map(Member::Over);
            }
        }
        inside
    })
}
