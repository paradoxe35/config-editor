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

/// Visits each value of `document` (parents first, in document order)
/// with what applies to it; a value no schema says anything of is not
/// visited, nor are its children. `decided`: what applies is walked for
/// each value (its conditionals and options decided by it), else for any
/// value (what the schema may say there, as locks read it).
pub(crate) fn each_value<'s>(
    document: &Value,
    root: &'s Value,
    decided: bool,
    visit: &mut dyn FnMut(&Visit<'_, 's>) -> Next,
) {
    let mut pointer = String::new();
    walk(
        document,
        None,
        Entry::schema(root),
        root,
        &mut pointer,
        &mut Walk { decided, visit },
        0,
    );
}

#[allow(clippy::too_many_arguments)]
struct Walk<'w, 's> {
    decided: bool,
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
    let seen = Seen::value(walker.decided.then_some(value));
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
            let next = walk(
                value,
                Some(&segment),
                here.child_named(&segment),
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
                let step = key.replace('~', "~0").replace('/', "~1");
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
