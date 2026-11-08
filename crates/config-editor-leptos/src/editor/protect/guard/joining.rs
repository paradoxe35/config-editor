//! Lines that would join a locked field once the text parses, read while
//! its values cannot be compared.

use std::ops::Range;

use super::super::super::language::ConfigLanguage;
use super::super::super::paths::path_at;
use super::super::locate::{LineIndex, within};
use super::super::locks::Lock;

/// Lines of an edited region checked one by one at most; a larger edit
/// leaving the text unreadable next to a lock is refused.
const MAX_REGION_LINES: usize = 2000;

/// How many lines touching `region` of `text` would join the field at
/// `pointer`: lines whose path is inside it, and lines naming a key that
/// holds it (a second `web:` above a locked `web/port`). Blank and comment
/// lines join nothing. `None` when the region is too large to check.
pub(super) fn joining_lines(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    region: &Range<usize>,
    pointer: &str,
) -> Option<usize> {
    let first = lines.line_of(region.start);
    let last = lines.line_of(region.end);
    if last - first > MAX_REGION_LINES {
        return None;
    }
    let mut count = 0;
    for line in first..=last {
        let content = lines.line_text(line);
        let indent = content.len() - content.trim_start().len();
        if content.trim().is_empty() || behavior.comment_start(content) == Some(indent) {
            continue;
        }
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

/// Whether the edit's `region` is on or next to the lines of `lock` (or
/// between it and the keys holding it), where it could move the field.
pub(super) fn is_near(lock: &Lock, region: &Range<usize>, lines: &LineIndex<'_>) -> bool {
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
    let (Some(low), Some(high)) = (starts.min(), ends.max()) else {
        return false;
    };
    let first = lines.line_of(low).saturating_sub(1);
    let last = (lines.line_of(high) + 1).min(lines.line_count() - 1);
    let low = lines.line_bounds(first).start;
    let high = lines.line_bounds(last).end + 1;
    region.start <= high && region.end >= low
}
