//! Walking a whole document with the schema: each value with what
//! applies to it, read from its parent's, so the document costs one step
//! per value.

use super::{Entry, Here, Seen};
use crate::editor::language::SchemaPathSegment;
use serde_json::Value;

/// What a visit of one value asks next.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Next {
    /// Its children.
    Into,
    /// Its next sibling, not its children.
    Past,
    /// Nothing more.
    Stop,
}

/// Values walked into at most, so a huge document cannot stall an edit.
const MAX_DEPTH: usize = 64;

/// One value visited.
pub(crate) struct Visit<'v, 's> {
    /// Its JSON pointer.
    pub(crate) pointer: &'v str,
    /// The step from its parent (none for the document).
    pub(crate) segment: Option<&'v SchemaPathSegment>,
    /// What applies to it.
    pub(crate) here: &'v Here<'s>,
}

/// How a document is walked.
#[derive(Clone, Copy, Default)]
pub(crate) struct Walking {
    /// What applies is walked for each value (its conditionals and
    /// options decided by it), else for any value (what the schema may
    /// say there, as locks read it).
    pub(crate) decided: bool,
    /// A key naming an array is one of its items as well (a language that
    /// writes a list by repeating its key writes a one-item list as the
    /// item alone).
    pub(crate) repeats_arrays: bool,
}

/// Visits each value of `document` (parents first, in document order)
/// with what applies to it; a value no schema says anything of is not
/// visited, nor are its children.
pub(crate) fn each_value<'s>(
    document: &Value,
    root: &'s Value,
    walking: Walking,
    visit: &mut dyn FnMut(&Visit<'_, 's>) -> Next,
) {
    let mut pointer = String::new();
    walk(
        document,
        None,
        Entry::schema(root),
        root,
        &mut pointer,
        &mut Walk { walking, visit },
        0,
    );
}

#[allow(clippy::too_many_arguments)]
struct Walk<'w, 's> {
    walking: Walking,
    visit: &'w mut dyn FnMut(&Visit<'_, 's>) -> Next,
}

fn walk<'s>(
    value: &Value,
    segment: Option<&SchemaPathSegment>,
    entry: Entry<'s>,
    root: &'s Value,
    pointer: &mut String,
    walker: &mut Walk<'_, 's>,
    depth: usize,
) -> Next {
    if depth > MAX_DEPTH || entry.is_empty() {
        return Next::Past;
    }
    let seen = Seen::value(walker.walking.decided.then_some(value));
    let here = Here::walk(entry, root, seen);
    let visited = Visit {
        pointer,
        segment,
        here: &here,
    };
    match (walker.visit)(&visited) {
        Next::Into => {}
        other => return other,
    }
    let length = pointer.len();
    let mut child =
        |segment: SchemaPathSegment, step: &str, value: &Value, pointer: &mut String| {
            pointer.push('/');
            pointer.push_str(step);
            let mut entry = here.child_named(&segment);
            if walker.walking.repeats_arrays
                && matches!(segment, SchemaPathSegment::Key(_))
                && !entry.is_empty()
            {
                let seen = Seen::value(walker.walking.decided.then_some(value));
                entry = super::with_items(entry.clone(), &Here::walk(entry, root, seen));
            }
            let next = walk(
                value,
                Some(&segment),
                entry,
                root,
                pointer,
                walker,
                depth + 1,
            );
            pointer.truncate(length);
            next
        };
    match value {
        Value::Object(map) => {
            for (key, member) in map {
                let step = crate::editor::language::pointer_key(key);
                if child(SchemaPathSegment::Key(key.clone()), &step, member, pointer) == Next::Stop
                {
                    return Next::Stop;
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let step = index.to_string();
                if child(SchemaPathSegment::ArrayItem(index), &step, item, pointer) == Next::Stop {
                    return Next::Stop;
                }
            }
        }
        _ => {}
    }
    Next::Into
}
