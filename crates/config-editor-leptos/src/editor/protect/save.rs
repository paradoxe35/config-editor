//! The last check before an Edit-as save reaches the host: the converted
//! text must hold every read-only value exactly as the host's last text
//! did, whatever route the edit took to get there.

use std::ops::Range;
use std::sync::Arc;

use serde_json::Value;

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::super::paths::field_range;
use super::locate::LineIndex;
use super::locks::{Sources, protect};
use super::marks::escape;

/// A save that would change the read-only value at `pointer`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReadOnlyChange {
    pub(crate) pointer: String,
}

/// The pointers of the fields `ranges` (UTF-16, in `text`) cover: each
/// field written there whose range overlaps one, outermost first.
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
    let mut found: Vec<String> = Vec::new();
    let mut stack: Vec<(String, &Value)> = vec![(String::new(), document)];
    while let Some((pointer, value)) = stack.pop() {
        let children: Vec<(String, &Value)> = match value {
            Value::Object(map) => map
                .iter()
                .map(|(key, child)| (format!("{pointer}/{}", escape(key)), child))
                .collect(),
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(index, child)| (format!("{pointer}/{index}"), child))
                .collect(),
            _ => Vec::new(),
        };
        for (child_pointer, child) in children {
            let covered = field_range(behavior, text, &child_pointer)
                .map(|range| lines.range(range))
                .is_some_and(|field| {
                    ranges
                        .iter()
                        .any(|range| field.start < range.end && range.start < field.end)
                });
            if covered {
                found.push(child_pointer);
            } else {
                stack.push((child_pointer, child));
            }
        }
    }
    found.retain(|pointer| !pointer.is_empty());
    found
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
