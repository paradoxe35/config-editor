//! The edit that makes a text parse while the text before did not: the one
//! moment a locked field has a value again, so the one place its value is
//! decided (see `locks` for the rule). Each locked field must read as its
//! reference: the value pinned for it, the value of the reference text
//! when that parses, else the reference text's own lines for it, kept, with
//! only closers added. A field the reference text never wrote has nothing
//! to keep.

use std::ops::Range;

use serde_json::Value;

use super::super::super::codec;
use super::super::super::diff::{DiffEdit, myers_edits};
use super::super::super::language::ConfigLanguage;
use super::super::super::paths::{key_range, path_at};
use super::super::locate::{LineIndex, extent, within};
use super::super::marks::schema_marks;
use super::super::scan::{self, Written};
use super::super::texts::Reason;
use super::edit::Edited;
use super::{Blocked, Why};

/// Line reads (each a path read from the top of the reference text) the
/// lines the edits removed from it get: as many lines as this divided by
/// the text's lines, at least `MIN_REMOVED_LINES`. Past that, the fields
/// they held are not known, and the repair is refused rather than guessed.
const REMOVED_READS: usize = 250_000;
const MIN_REMOVED_LINES: usize = 64;

/// Checks `edit`, whose text before does not parse, and whose text after
/// reads as `edited`.
pub(super) fn repair_keeps_locks(edit: &Edited<'_>, edited: &Value) -> Result<(), Blocked> {
    let protected = edit.protected;
    let Some(origin) = protected.origin.as_deref() else {
        return Ok(());
    };
    let behavior = edit.behavior;
    let reference = codec::read(behavior, origin, edit.reading_schema).ok();
    let diff = LineDiff::new(origin, edit.after);
    let removed = match &reference {
        Some(_) => Vec::new(),
        None => diff.removed_paths(behavior, origin).ok_or(Blocked {
            reason: Reason::Field,
            why: Why::Unread,
            pointer: None,
        })?,
    };
    let after_lines = LineIndex::new(edit.after);
    for (pointer, reason) in fields(edit, edited, &removed) {
        let refuse = |why: Why| Blocked {
            reason: reason.clone(),
            why,
            pointer: Some(pointer.clone()),
        };
        let now = edited.pointer(&pointer);
        if let Some((_, pinned)) = protected.pins.iter().find(|(known, _)| *known == pointer) {
            if now != Some(pinned) {
                return Err(refuse(Why::Changed));
            }
            continue;
        }
        if let Some(reference) = &reference {
            if now != reference.pointer(&pointer) {
                return Err(refuse(Why::Changed));
            }
            continue;
        }
        // Not in the edited text: refused when the reference text wrote it
        // (a lock found in it, a line of it removed, its key).
        let Some(value) = now else {
            let written = protected
                .locks
                .iter()
                .any(|lock| lock.pointer.as_deref() == Some(&pointer))
                || protected
                    .unplaced
                    .iter()
                    .any(|(known, _)| *known == pointer)
                || removed
                    .iter()
                    .any(|line| within(behavior, &line.pointer, &pointer))
                || key_range(behavior, origin, &pointer).is_some();
            if written {
                return Err(refuse(Why::Changed));
            }
            continue;
        };
        let Some(found) = extent(
            behavior,
            edit.after,
            &after_lines,
            Some(value),
            None,
            &pointer,
        ) else {
            return Err(refuse(Why::Changed));
        };
        let held = |line: &Written| within(behavior, &line.pointer, &pointer);
        if !diff.kept_with_closers(behavior, &after_lines, &found.ranges, &removed, &held) {
            return Err(refuse(Why::Changed));
        }
    }
    Ok(())
}

/// The fields to check, with their reasons: those locked or held in the
/// text before, the values pinned, the host's paths, and the fields the
/// schema marks read-only in the edited text and on the lines the edit
/// removed from the reference text.
fn fields(edit: &Edited<'_>, edited: &Value, removed: &[Written]) -> Vec<(String, Reason)> {
    let protected = edit.protected;
    let mut fields: Vec<(String, Reason)> = Vec::new();
    let mut add = |pointer: &str, reason: Reason| {
        if !fields.iter().any(|(known, _)| known == pointer) {
            fields.push((pointer.to_string(), reason));
        }
    };
    for lock in &protected.locks {
        if let Some(pointer) = &lock.pointer {
            add(pointer, lock.reason.clone());
        }
    }
    for (pointer, reason) in protected.unplaced.iter().chain(&protected.marked) {
        add(pointer, reason.clone());
    }
    for (pointer, _) in &protected.pins {
        add(pointer, Reason::Field);
    }
    for pointer in &protected.paths {
        add(pointer, Reason::Field);
    }
    if let Some(schema) = edit.reading_schema.filter(|_| protected.marking) {
        for (pointer, reason) in schema_marks(edited, schema).read_only {
            add(&pointer, reason.map_or(Reason::Field, Reason::Given));
        }
        for (pointer, reason) in scan::marked(edit.behavior, schema, removed) {
            add(&pointer, reason);
        }
    }
    fields
}

/// The line diff from the reference text to the edited text.
struct LineDiff<'a> {
    origin: LineIndex<'a>,
    /// For each line of the edited text, the reference text's line it
    /// keeps whole, if any.
    kept: Vec<Option<usize>>,
    /// The reference text's lines the edit removed (or changed), each with
    /// the line of the edited text it was removed before.
    removed: Vec<(usize, usize)>,
}

impl<'a> LineDiff<'a> {
    fn new(origin: &'a str, after: &str) -> Self {
        let old: Vec<&str> = origin.split('\n').collect();
        let new: Vec<&str> = after.split('\n').collect();
        let mut kept = Vec::with_capacity(new.len());
        let mut removed = Vec::new();
        match myers_edits(&old, &new) {
            Some(edits) => {
                let (mut at_old, mut at_new) = (0, 0);
                for edit in edits {
                    match edit {
                        DiffEdit::Equal => {
                            kept.push(Some(at_old));
                            at_old += 1;
                            at_new += 1;
                        }
                        DiffEdit::Delete => {
                            removed.push((at_old, at_new));
                            at_old += 1;
                        }
                        DiffEdit::Insert => {
                            kept.push(None);
                            at_new += 1;
                        }
                    }
                }
            }
            // Too different to diff: nothing is known to be kept.
            None => {
                kept = vec![None; new.len()];
                removed = (0..old.len()).map(|line| (line, 0)).collect();
            }
        }
        LineDiff {
            origin: LineIndex::new(origin),
            kept,
            removed,
        }
    }

    /// The paths of the removed lines holding code, read in the reference
    /// text; `None` when there are too many to read.
    fn removed_paths(&self, behavior: &dyn ConfigLanguage, origin: &str) -> Option<Vec<Written>> {
        let lines: Vec<usize> = self
            .removed
            .iter()
            .map(|(line, _)| *line)
            .filter(|line| holds_code(behavior, self.origin.line_text(*line)))
            .collect();
        let most = (REMOVED_READS / self.origin.line_count().max(1)).max(MIN_REMOVED_LINES);
        if lines.len() > most {
            return None;
        }
        let mut found = Vec::new();
        for line in lines {
            let content = self.origin.line_text(line);
            let indent = content.len() - content.trim_start().len();
            let at = self.origin.line_bounds(line).start + content[..indent].encode_utf16().count();
            if let Some(path) = path_at(behavior, origin, at) {
                found.push(Written {
                    line,
                    pointer: path.to_pointer(),
                });
            }
        }
        Some(found)
    }

    /// Whether the field covering `ranges` of the edited text is made of
    /// the reference text's lines, kept, with only closers added (a line
    /// changed by adding `}`, a new line holding only `]`), and none of the
    /// reference text's lines inside it removed: those between its lines,
    /// and those `held` by it just around it.
    fn kept_with_closers(
        &self,
        behavior: &dyn ConfigLanguage,
        lines: &LineIndex<'_>,
        ranges: &[Range<usize>],
        removed: &[Written],
        held: &dyn Fn(&Written) -> bool,
    ) -> bool {
        let (Some(first), Some(last)) = (ranges.first(), ranges.last()) else {
            return true;
        };
        let first_line = lines.line_of(first.start);
        let last_line = lines.line_of(last.end.saturating_sub(1).max(last.start));
        let mut changed: Vec<String> = Vec::new();
        for range in ranges {
            let (start, end) = (lines.line_of(range.start), lines.line_of(range.end));
            for line in start..=end {
                if self.kept.get(line).copied().flatten().is_some() {
                    continue;
                }
                let bounds = lines.line_bounds(line);
                let from = range.start.max(bounds.start);
                let to = range.end.min(bounds.end).max(from);
                let units: Vec<u16> = lines.line_text(line).encode_utf16().collect();
                let part = String::from_utf16_lossy(
                    &units[(from - bounds.start).min(units.len())
                        ..(to - bounds.start).min(units.len())],
                );
                changed.push(part);
            }
        }
        // The reference text's lines removed inside the field or just around
        // it, which its changed lines may come from; those inside it, and
        // those around it that were the field's own, must reappear in it.
        let candidates: Vec<(usize, bool)> = self
            .removed
            .iter()
            .filter(|(line, _)| holds_code(behavior, self.origin.line_text(*line)))
            .filter_map(|(line, before)| {
                let inside = *before > first_line && *before <= last_line;
                let around = *before == first_line || *before == last_line + 1;
                let own = || {
                    removed
                        .iter()
                        .any(|written| written.line == *line && held(written))
                };
                (inside || around).then(|| (*line, inside || own()))
            })
            .collect();
        let mut matched = vec![false; candidates.len()];
        for part in &changed {
            let part = compact(part);
            if without_closers(&part).is_empty() {
                continue;
            }
            let found: Vec<usize> = candidates
                .iter()
                .enumerate()
                .filter(|(_, (line, _))| {
                    closers_added(&compact(self.origin.line_text(*line)), &part)
                })
                .map(|(index, _)| index)
                .collect();
            if found.is_empty() {
                return false;
            }
            for index in found {
                matched[index] = true;
            }
        }
        candidates
            .iter()
            .zip(&matched)
            .all(|((_, required), done)| *done || !required)
    }
}

/// Whether `line` holds code: not blank, not only a comment.
fn holds_code(behavior: &dyn ConfigLanguage, line: &str) -> bool {
    let indent = line.len() - line.trim_start().len();
    !line.trim().is_empty() && behavior.comment_start(line) != Some(indent)
}

/// `text` without whitespace and without a comma ending it (a separator
/// that moves to a closer added after it: `"b": 2,` → `"b": 2` + `},`).
fn compact(text: &str) -> Vec<char> {
    let mut chars: Vec<char> = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    while chars.last() == Some(&',') {
        chars.pop();
    }
    chars
}

/// Whether `character` only closes: adding it adds no data.
fn is_closer(character: char) -> bool {
    matches!(character, '}' | ']' | ')')
}

/// `compacted` without closers and markup end tags (`</web>`).
fn without_closers(compacted: &[char]) -> Vec<char> {
    let mut kept = Vec::new();
    let mut at = 0;
    while at < compacted.len() {
        if let Some(length) = end_tag(&compacted[at..]) {
            at += length;
            continue;
        }
        if !is_closer(compacted[at]) {
            kept.push(compacted[at]);
        }
        at += 1;
    }
    kept
}

/// The length of the markup end tag `</name>` starting `chars`, if one does.
fn end_tag(chars: &[char]) -> Option<usize> {
    if chars.first() != Some(&'<') || chars.get(1) != Some(&'/') {
        return None;
    }
    let close = chars.iter().position(|character| *character == '>')?;
    chars[2..close]
        .iter()
        .all(|character| character.is_alphanumeric() || matches!(character, '_' | '-' | '.' | ':'))
        .then_some(close + 1)
}

/// Whether `part` (of the edited text) is a stretch of `line` (of the
/// reference text) with only closers added: its data is a contiguous
/// stretch of the line's, and the line's closers in that stretch are all
/// still there, in order (none removed or moved).
fn closers_added(line: &[char], part: &[char]) -> bool {
    let core = without_closers(part);
    for start in 0..line.len() {
        // The stretch of `line` from `start` whose data is `core`.
        let mut data = 0;
        let mut end = start;
        while end < line.len() && data < core.len() {
            if let Some(length) = end_tag(&line[end..]) {
                end += length;
                continue;
            }
            if is_closer(line[end]) {
                end += 1;
                continue;
            }
            if line[end] != core[data] {
                break;
            }
            data += 1;
            end += 1;
        }
        if data == core.len() && is_subsequence(&line[start..end], part) {
            return true;
        }
    }
    false
}

fn is_subsequence(short: &[char], long: &[char]) -> bool {
    let mut rest = long.iter();
    short
        .iter()
        .all(|character| rest.any(|other| other == character))
}
