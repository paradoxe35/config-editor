//! Lines that would join a locked field once the text parses, read while
//! its values cannot be compared.

use std::ops::Range;

use super::super::super::language::{ConfigLanguage, SchemaPathSegment};
use super::super::super::paths::{FieldPath, path_at};
use super::super::locate::{LineIndex, within};
use super::super::locks::Lock;
use super::super::scan::names_word;

/// Lines of an edited region read for their paths at most (each a read
/// from the top of the text); an edit needing more, leaving the text
/// unreadable next to a lock, is refused.
const MAX_REGION_LINES: usize = 2000;

/// How many lines touching `region` of `text` would join the field at
/// `pointer`: lines whose path is inside it, and lines naming a key that
/// holds it (a second `web:` above a locked `web/port`). Blank and comment
/// lines join nothing. Only lines that could are read: those `near` the
/// field (its lines, and the next line holding code on each side, where a
/// line joins a block) and those naming one of its keys (a duplicate, a
/// TOML table or dotted key anywhere). `None` when they are too many to
/// read.
pub(super) fn joining_lines(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    region: &Range<usize>,
    near: Range<usize>,
    pointer: &str,
) -> Option<usize> {
    let first = lines.line_of(region.start);
    let last = lines.line_of(region.end);
    let keys: Vec<String> = FieldPath::from(pointer)
        .segments()
        .iter()
        .filter_map(|segment| match segment {
            SchemaPathSegment::Key(key) => Some(key.clone()),
            _ => None,
        })
        .collect();
    let mut read = 0;
    let mut count = 0;
    for line in first..=last {
        let content = lines.line_text(line);
        if !holds_code(behavior, content) || !(near.contains(&line) || names_word(&keys, content)) {
            continue;
        }
        read += 1;
        if read > MAX_REGION_LINES {
            return None;
        }
        let indent = content.len() - content.trim_start().len();
        let at = lines.line_bounds(line).start + content[..indent].encode_utf16().count();
        let Some(path) = path_at(behavior, text, at) else {
            continue;
        };
        let path = path.to_pointer();
        let holds = !path.is_empty()
            && within(behavior, pointer, &path)
            && !within(behavior, &path, pointer);
        if holds || within(behavior, &path, pointer) {
            count += 1;
        }
    }
    Some(count)
}

/// The lines near the field covering UTF-16 `low..high` of a text: its
/// lines, and past blank and comment lines, the next line on each side.
pub(super) fn near_lines(
    behavior: &dyn ConfigLanguage,
    lines: &LineIndex<'_>,
    low: usize,
    high: usize,
) -> Range<usize> {
    let mut first = lines.line_of(low);
    let mut last = lines.line_of(high.max(low));
    while first > 0 {
        first -= 1;
        if holds_code(behavior, lines.line_text(first)) {
            break;
        }
    }
    while last + 1 < lines.line_count() {
        last += 1;
        if holds_code(behavior, lines.line_text(last)) {
            break;
        }
    }
    first..last + 1
}

/// Whether `line` holds code: not blank, not only a comment.
fn holds_code(behavior: &dyn ConfigLanguage, line: &str) -> bool {
    let indent = line.len() - line.trim_start().len();
    !line.trim().is_empty() && behavior.comment_start(line) != Some(indent)
}

/// Whether the edit's `region` is on or next to the lines of `lock` (or
/// between it and the keys holding it), where it could move the field.
pub(super) fn is_near(lock: &Lock, region: &Range<usize>, lines: &LineIndex<'_>) -> bool {
    let Some((low, high)) = span(lock) else {
        return false;
    };
    let first = lines.line_of(low).saturating_sub(1);
    let last = (lines.line_of(high) + 1).min(lines.line_count() - 1);
    let low = lines.line_bounds(first).start;
    let high = lines.line_bounds(last).end + 1;
    region.start <= high && region.end >= low
}

/// Where `lock` starts and ends, the keys holding it included.
pub(super) fn span(lock: &Lock) -> Option<(usize, usize)> {
    let starts = lock
        .ranges
        .iter()
        .chain(&lock.guards)
        .map(|range| range.start);
    let ends = lock
        .ranges
        .iter()
        .chain(&lock.guards)
        .map(|range| range.end);
    Some((starts.min()?, ends.max()?))
}
