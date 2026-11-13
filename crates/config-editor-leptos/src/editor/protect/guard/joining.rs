//! Lines that would join a locked field once the text parses, read while
//! its values cannot be compared.

use std::cell::RefCell;
use std::collections::HashMap;
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

/// The paths of the lines of one text, each read once however many locks
/// ask for it (a read is a path read from the top of the text).
pub(super) struct LinePaths<'a> {
    behavior: &'a dyn ConfigLanguage,
    text: &'a str,
    lines: &'a LineIndex<'a>,
    read: RefCell<HashMap<usize, Option<String>>>,
}

impl<'a> LinePaths<'a> {
    pub(super) fn new(
        behavior: &'a dyn ConfigLanguage,
        text: &'a str,
        lines: &'a LineIndex<'a>,
    ) -> Self {
        Self {
            behavior,
            text,
            lines,
            read: RefCell::new(HashMap::new()),
        }
    }

    /// The pointer of what line `line` writes, read at its first character.
    pub(super) fn path(&self, line: usize) -> Option<String> {
        if let Some(known) = self.read.borrow().get(&line) {
            return known.clone();
        }
        let content = self.lines.line_text(line);
        let indent = content.len() - content.trim_start().len();
        let at = self.lines.line_bounds(line).start + content[..indent].encode_utf16().count();
        let path = path_at(self.behavior, self.text, at).map(|path| path.to_pointer());
        self.read.borrow_mut().insert(line, path.clone());
        path
    }
}

/// How many lines touching `region` of the text `paths` reads would join the field at
/// `pointer`: lines whose path is inside it, and lines naming a key that
/// holds it (a second `web:` above a locked `web/port`). Blank and comment
/// lines join nothing. Only lines that could are read: those `near` the
/// field (its lines, and the next line holding code on each side, where a
/// line joins a block) and those naming one of its keys (a duplicate, a
/// TOML table or dotted key anywhere). `None` when they are too many to
/// read.
pub(super) fn joining_lines(
    paths: &LinePaths<'_>,
    region: &Range<usize>,
    near: Range<usize>,
    pointer: &str,
) -> Option<usize> {
    let (behavior, lines) = (paths.behavior, paths.lines);
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
        let Some(path) = paths.path(line) else {
            continue;
        };
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
pub(super) fn holds_code(behavior: &dyn ConfigLanguage, line: &str) -> bool {
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
