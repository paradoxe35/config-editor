//! Lines that would join a locked field once the text parses, read while
//! its values cannot be compared.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;

use super::super::super::language::{ConfigLanguage, SchemaPathSegment};
use super::super::super::paths::{FieldPath, path_at, paths_at};
use super::super::locate::{LineIndex, plain};
use super::super::locks::Lock;
use super::super::scan::names_word;

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
        let path = path_at(self.behavior, self.text, self.first_character(line))
            .map(|path| path.to_pointer());
        self.read.borrow_mut().insert(line, path.clone());
        path
    }

    /// Reads the paths of `lines` in one pass of the text (see
    /// `paths::paths_at`), where each read alone walks it from the top.
    pub(super) fn read_all(&self, lines: &[usize]) {
        let unread: Vec<usize> = {
            let read = self.read.borrow();
            lines
                .iter()
                .copied()
                .filter(|line| !read.contains_key(line))
                .collect()
        };
        if unread.is_empty() {
            return;
        }
        let cursors: Vec<usize> = unread
            .iter()
            .map(|line| self.first_character(*line))
            .collect();
        let found = paths_at(self.behavior, self.text, &cursors);
        let mut read = self.read.borrow_mut();
        for (line, path) in unread.into_iter().zip(found) {
            read.insert(line, path.map(|path| path.to_pointer()));
        }
    }

    /// The UTF-16 offset of line `line`'s first character past its indent.
    fn first_character(&self, line: usize) -> usize {
        let content = self.lines.line_text(line);
        let indent = content.len() - content.trim_start().len();
        self.lines.line_bounds(line).start + content[..indent].encode_utf16().count()
    }

    /// The text's lines.
    pub(super) fn lines(&self) -> &LineIndex<'a> {
        self.lines
    }

    /// The language the text is written in.
    pub(super) fn behavior(&self) -> &'a dyn ConfigLanguage {
        self.behavior
    }
}

/// Whether `line`, read at `path` (a block holding a locked field), names
/// that block's key: a second key holding the field (`limits:` again). A
/// line half typed inside the block reads at the block too (`c` on its way
/// to `ca: 1`, a sibling's separator deleted), but it names another key: a
/// sibling, which joins nothing locked.
pub(super) fn names_holder(behavior: &dyn ConfigLanguage, path: &str, line: &str) -> bool {
    match FieldPath::from(plain(behavior, path).as_str())
        .segments()
        .last()
    {
        Some(SchemaPathSegment::Key(key)) => names_word(std::slice::from_ref(key), line),
        _ => false,
    }
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
