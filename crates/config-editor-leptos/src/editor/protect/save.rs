//! The last check before an Edit-as save reaches the host: the converted
//! text must hold every read-only value exactly as the host's last text
//! did, whatever route the edit took to get there.

use std::ops::Range;
use std::sync::Arc;

use serde_json::Value;

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::super::paths::{field_range, with_spans};
use super::locate::LineIndex;
use super::locks::{Sources, protect};
use super::marks::escape;

/// A save that would change the read-only value at `pointer`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReadOnlyChange {
    pub(crate) pointer: String,
}

/// The pointers of the fields `ranges` (UTF-16, in `text`) cover: each
/// field written there whose range overlaps one, outermost first. Every
/// field's span is read in one pass of the text (a read each made every
/// keystroke of an Edit-as view with read-only ranges cost fields × text).
pub(crate) fn range_pointers(
    behavior: &dyn ConfigLanguage,
    text: &str,
    document: &Value,
    ranges: &[Range<usize>],
) -> Vec<String> {
    if ranges.is_empty() {
        return Vec::new();
    }
    let lines = LineIndex::new(text);
    let overlaps = Overlaps::new(ranges);
    let mut all = Vec::new();
    every_field(document, "", &mut all);
    with_spans(behavior, text, &all, || {
        let mut found: Vec<String> = Vec::new();
        let mut stack: Vec<(String, &Value)> = vec![(String::new(), document)];
        while let Some((pointer, value)) = stack.pop() {
            let mut inside = Vec::new();
            children(value, &pointer, &mut |pointer, child| {
                inside.push((pointer, child));
            });
            for (child_pointer, child) in inside {
                let covered = field_range(behavior, text, &child_pointer)
                    .is_some_and(|range| overlaps.any(lines.range(range)));
                if covered {
                    found.push(child_pointer);
                } else {
                    stack.push((child_pointer, child));
                }
            }
        }
        found.retain(|pointer| !pointer.is_empty());
        found
    })
}

/// The pointer of every field inside `value` (at `pointer`).
fn every_field(value: &Value, pointer: &str, out: &mut Vec<String>) {
    children(value, pointer, &mut |child_pointer, child| {
        every_field(child, &child_pointer, out);
        out.push(child_pointer);
    });
}

/// Each child of `value` (at `pointer`) with its pointer, in order.
fn children<'v>(value: &'v Value, pointer: &str, every: &mut dyn FnMut(String, &'v Value)) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                every(format!("{pointer}/{}", escape(key)), child);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                every(format!("{pointer}/{index}"), child);
            }
        }
        _ => {}
    }
}

/// Ranges sorted by their start, with the furthest end so far: whether a
/// field overlaps any is a binary search, not a look at each.
struct Overlaps {
    starts: Vec<usize>,
    furthest: Vec<usize>,
}

impl Overlaps {
    fn new(ranges: &[Range<usize>]) -> Self {
        let mut sorted = ranges.to_vec();
        sorted.sort_by_key(|range| range.start);
        let mut furthest = Vec::with_capacity(sorted.len());
        let mut end = 0;
        for range in &sorted {
            end = end.max(range.end);
            furthest.push(end);
        }
        Overlaps {
            starts: sorted.iter().map(|range| range.start).collect(),
            furthest,
        }
    }

    /// Whether `field` overlaps a range: one starting before its end ends
    /// after its start.
    fn any(&self, field: Range<usize>) -> bool {
        let before = self.starts.partition_point(|start| *start < field.end);
        before > 0 && self.furthest[before - 1] > field.start
    }
}

/// Every read-only pointer of the host's `text`.
pub(crate) fn locked_pointers(
    behavior: &dyn ConfigLanguage,
    text: &str,
    sources: Sources<'_>,
) -> Vec<String> {
    let protected = protect(
        behavior,
        text,
        Sources {
            ranges: &[],
            ..sources
        },
    );
    let mut pointers: Vec<String> = protected
        .locks
        .iter()
        .filter_map(|lock| lock.pointer.clone())
        .chain(
            protected
                .unplaced
                .iter()
                .map(|(pointer, _)| pointer.clone()),
        )
        .collect();
    if !sources.ranges.is_empty() {
        let document = match protected.document {
            Some(document) => Some(document),
            None => codec::read(behavior, text, sources.reading_schema)
                .ok()
                .map(Arc::new),
        };
        if let Some(document) = document {
            pointers.extend(range_pointers(behavior, text, &document, sources.ranges));
        }
    }
    pointers
}

/// Refuses a save from `old` to `new` (host texts) that changes the value
/// at one of `pointers`, with the message to show.
pub(crate) fn check_save(
    behavior: &dyn ConfigLanguage,
    reading_schema: Option<&Value>,
    old: &str,
    new: &str,
    pointers: &[String],
) -> Result<(), ReadOnlyChange> {
    if pointers.is_empty() {
        return Ok(());
    }
    let Ok(before) = codec::read(behavior, old, reading_schema) else {
        return Ok(());
    };
    let after = codec::read(behavior, new, reading_schema).ok();
    for pointer in pointers {
        let changed = match &after {
            Some(after) => before.pointer(pointer) != after.pointer(pointer),
            None => true,
        };
        if changed {
            return Err(ReadOnlyChange {
                pointer: pointer.clone(),
            });
        }
    }
    Ok(())
}
