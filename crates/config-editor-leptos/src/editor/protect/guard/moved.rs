//! A locked field's lines moved by an edit leaving the text broken (a line
//! command, a cut and paste) stay where the field is: each line of the
//! field, and each key holding it, that the edit rewrote reads under the
//! same path afterwards (a key moved above the document's opening brace
//! holds nothing); and where indentation is structure, no line more
//! indented than the field comes to follow it (a list's items left under a
//! locked key, its own key moved or deleted), nor, where such lines followed
//! it (the other keys of the list item its line opens), comes above it; and
//! the field's lines holding code keep their order with no other line of
//! code coming between them (its key moved away from the lines under it).
//! Reorders among siblings keep all three, and go through.

use std::ops::Range;

use super::super::locate::LineIndex;
use super::super::locks::Lock;
use super::edit::Edited;
use super::joining::{LinePaths, holds_code};

/// The texts before and after the edit, with their lines' paths.
pub(super) struct Place<'p, 'e, 'o, 'n> {
    pub(super) edit: &'p Edited<'e>,
    pub(super) old: &'p LinePaths<'o>,
    pub(super) new: &'p LinePaths<'n>,
    pub(super) new_lines: &'p LineIndex<'n>,
}

impl Place<'_, '_, '_, '_> {
    /// Whether `lock` stays where it is (see the module docs).
    pub(super) fn kept(&self, lock: &Lock) -> bool {
        self.paths_kept(lock) && self.nothing_indented_under(lock) && self.lines_together(lock)
    }

    /// The lines of `lock` holding code, followed through the edit (those
    /// it rewrote found once among the lines it wrote), are still in order
    /// with no other line of code between two of them. A line not found
    /// is not followed.
    fn lines_together(&self, lock: &Lock) -> bool {
        let edit = self.edit;
        let Some((low, high)) = hull(lock) else {
            return true;
        };
        let old_lines = &edit.lines;
        let (first, last) = (
            old_lines.line_of(low),
            old_lines.line_of(high.saturating_sub(1).max(low)),
        );
        let (from, to) = (
            old_lines.line_of(edit.region.start),
            old_lines.line_of(edit.region.end),
        );
        if first == last || last < from || first > to {
            return true;
        }
        let code = |text: &str| holds_code(edit.behavior, text);
        let mut previous: Option<usize> = None;
        for line in first..=last {
            if !code(old_lines.line_text(line)) {
                continue;
            }
            let Some(now) = self.now(line) else {
                return true;
            };
            if let Some(previous) = previous
                && (now <= previous
                    || (previous + 1..now).any(|between| code(self.new_lines.line_text(between))))
            {
                return false;
            }
            previous = Some(now);
        }
        true
    }

    /// Each line of `lock` (its key and stretches) and of the keys holding
    /// it that the edit rewrote, found once among the lines it wrote, has
    /// the path it had.
    fn paths_kept(&self, lock: &Lock) -> bool {
        let old_lines = &self.edit.lines;
        let rewritten = |range: &Range<usize>| {
            range.start < self.edit.region.end && range.end > self.edit.region.start
        };
        let mut lines: Vec<usize> = lock
            .key
            .iter()
            .chain(&lock.ranges)
            .chain(&lock.guards)
            .filter(|range| rewritten(range))
            .map(|range| old_lines.line_of(range.start))
            .collect();
        lines.sort_unstable();
        lines.dedup();
        lines.into_iter().all(|line| {
            let Some(was) = self.old.path(line) else {
                return true;
            };
            match self.new_line(old_lines.line_text(line)) {
                Some(now) => self.new.path(now).as_deref() == Some(was.as_str()),
                None => true,
            }
        })
    }

    /// Where indentation is structure: the line holding code after the
    /// lock's last line is no more indented than its first line, unless it
    /// already was before the edit; then the lines it opens stay below it.
    fn nothing_indented_under(&self, lock: &Lock) -> bool {
        let edit = self.edit;
        if !edit.behavior.indent_is_structure() {
            return true;
        }
        let Some((low, high)) = hull(lock) else {
            return true;
        };
        let old_lines = &edit.lines;
        let (first, last) = (
            old_lines.line_of(low),
            old_lines.line_of(high.saturating_sub(1).max(low)),
        );
        let indent = |text: &str| text.len() - text.trim_start().len();
        let depth = indent(old_lines.line_text(first));
        let deeper_after = |lines: &LineIndex<'_>, line: usize| {
            (line + 1..lines.line_count())
                .map(|next| lines.line_text(next))
                .find(|text| holds_code(edit.behavior, text))
                .is_some_and(|text| indent(text) > depth)
        };
        if deeper_after(old_lines, last) {
            return self.followers_kept(first, last, depth);
        }
        let new_last = if high <= edit.region.start {
            Some(last)
        } else if low >= edit.region.end {
            (last + self.new_lines.line_count()).checked_sub(old_lines.line_count())
        } else {
            self.new_line(old_lines.line_text(last))
        };
        new_last.is_none_or(|line| !deeper_after(self.new_lines, line))
    }

    /// Where line `line` of the text before the edit is after it: as it was
    /// above the lines the edit rewrote, shifted below them, found once
    /// among those it wrote otherwise.
    fn now(&self, line: usize) -> Option<usize> {
        let old_lines = &self.edit.lines;
        let region = &self.edit.region;
        let (from, to) = (
            old_lines.line_of(region.start),
            old_lines.line_of(region.end),
        );
        let added = self.new_lines.line_count() as isize - old_lines.line_count() as isize;
        if line < from {
            Some(line)
        } else if line > to {
            line.checked_add_signed(added)
        } else {
            self.new_line(old_lines.line_text(line))
        }
    }

    /// The lock's first line (`first`, at indent `depth`) was followed by
    /// more indented lines up to `last`'s next line (the other keys of the
    /// list item it opens): none of those comes above it now.
    fn followers_kept(&self, first: usize, last: usize, depth: usize) -> bool {
        let old_lines = &self.edit.lines;
        let Some(lock_now) = self.now(first) else {
            return true;
        };
        let indent = |text: &str| text.len() - text.trim_start().len();
        for line in last + 1..old_lines.line_count() {
            let text = old_lines.line_text(line);
            if !holds_code(self.edit.behavior, text) {
                continue;
            }
            if indent(text) <= depth {
                break;
            }
            if self.now(line).is_some_and(|now| now < lock_now) {
                return false;
            }
        }
        true
    }

    /// The one line among those the edit wrote that reads `text`, if one
    /// does (none, or several: not followed).
    fn new_line(&self, text: &str) -> Option<usize> {
        let lines = self.new_lines;
        let region = &self.edit.new_region;
        let mut found = (lines.line_of(region.start)..=lines.line_of(region.end))
            .filter(|now| lines.line_text(*now) == text);
        match (found.next(), found.next()) {
            (Some(now), None) => Some(now),
            _ => None,
        }
    }
}

/// The first and last UTF-16 offset of `lock`'s key and stretches.
fn hull(lock: &Lock) -> Option<(usize, usize)> {
    let starts = lock.key.iter().chain(&lock.ranges).map(|range| range.start);
    let ends = lock.key.iter().chain(&lock.ranges).map(|range| range.end);
    starts.min().zip(ends.max())
}
