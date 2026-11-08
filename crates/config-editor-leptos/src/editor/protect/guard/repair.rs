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
use super::super::super::language::{ConfigLanguage, SchemaPathSegment};
use super::super::super::paths::{FieldPath, key_range, path_at};
use super::super::locate::{LineIndex, extent, within};
use super::super::marks::{read_only_names, schema_marks};
use super::super::scan::{self, Written, names_word};
use super::super::texts::Reason;
use super::closers::{Part, compact};
use super::edit::Edited;
use super::{Blocked, Why};

/// Line reads (each a path read from the top of the reference text) the
/// removed lines that may hold a protected field get (those naming one,
/// and the blocks under them): as many lines as this divided by the text's
/// lines, at least `MIN_REMOVED_LINES`. Past that, the fields they held
/// are not known, and the repair is refused, naming one, rather than
/// guessed. Other removed lines are read only where a field's own lines
/// end, one past the field's.
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
    let names = names(edit);
    let removed = match &reference {
        Some(_) => Vec::new(),
        None => diff
            .removed_paths(behavior, origin, &names)
            .map_err(|unread| unchecked(edit, edited, &names, &unread))?,
    };
    let pointer_of = |line: usize| {
        removed
            .iter()
            .find(|written| written.line == line)
            .map(|written| written.pointer.clone())
            .or_else(|| diff.pointer_of(behavior, origin, line))
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
        let held =
            |line: usize| pointer_of(line).is_some_and(|path| within(behavior, &path, &pointer));
        if !diff.kept_with_closers(behavior, &after_lines, &found.ranges, value, &held) {
            return Err(refuse(Why::Changed));
        }
    }
    Ok(())
}

/// The keys of the fields that may be locked: those of the fields the
/// text before knew of, and those the schema marks read-only.
fn names(edit: &Edited<'_>) -> Vec<String> {
    let protected = edit.protected;
    let mut names: Vec<String> = edit
        .reading_schema
        .filter(|_| protected.marking)
        .map(read_only_names)
        .unwrap_or_default();
    let pointers = protected
        .locks
        .iter()
        .filter_map(|lock| lock.pointer.as_ref())
        .chain(protected.unplaced.iter().map(|(pointer, _)| pointer))
        .chain(protected.marked.iter().map(|(pointer, _)| pointer))
        .chain(protected.pins.iter().map(|(pointer, _)| pointer))
        .chain(&protected.paths);
    for pointer in pointers {
        for segment in FieldPath::from(pointer.as_str()).segments() {
            if let SchemaPathSegment::Key(key) = segment
                && !names.contains(key)
            {
                names.push(key.clone());
            }
        }
    }
    names
}

/// The refusal of a repair after more lines that may hold a protected field
/// were removed than can be read (`unread`, lines of the reference text):
/// naming the field one of them names.
fn unchecked(edit: &Edited<'_>, edited: &Value, names: &[String], unread: &[usize]) -> Blocked {
    let origin = edit.protected.origin.as_deref().unwrap_or_default();
    let lines = LineIndex::new(origin);
    let named: Vec<&String> = names
        .iter()
        .filter(|name| {
            unread
                .iter()
                .any(|line| names_word(std::slice::from_ref(*name), lines.line_text(*line)))
        })
        .collect();
    let fields = fields(edit, edited, &[]);
    let field = fields.iter().find(|(pointer, _)| {
        FieldPath::from(pointer.as_str())
            .segments()
            .iter()
            .any(|segment| matches!(segment, SchemaPathSegment::Key(key) if named.contains(&key)))
    });
    let (pointer, reason) = match (field, named.first()) {
        (Some((pointer, reason)), _) => (pointer.clone(), reason.clone()),
        (None, Some(name)) => (format!("/{name}"), Reason::Field),
        (None, None) => (String::new(), Reason::Field),
    };
    Blocked {
        reason,
        why: Why::Unread,
        pointer: Some(pointer),
    }
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

    /// The paths, read in the reference text, of the removed lines holding
    /// code that may hold a protected field: those naming one of `names`,
    /// and the blocks under the lines naming one; the lines when there are
    /// too many to read.
    fn removed_paths(
        &self,
        behavior: &dyn ConfigLanguage,
        origin: &str,
        names: &[String],
    ) -> Result<Vec<Written>, Vec<usize>> {
        let count = self.origin.line_count();
        let mut named = vec![false; count];
        if !self.removed.is_empty() {
            let accept = |line: &str| names_word(names, line);
            for line in scan::candidates(&self.origin, &accept, usize::MAX) {
                named[line] = true;
            }
        }
        let lines: Vec<usize> = self
            .removed
            .iter()
            .map(|(line, _)| *line)
            .filter(|line| named[*line] && holds_code(behavior, self.origin.line_text(*line)))
            .collect();
        let most = (REMOVED_READS / count.max(1)).max(MIN_REMOVED_LINES);
        if lines.len() > most {
            return Err(lines);
        }
        Ok(lines
            .into_iter()
            .filter_map(|line| {
                let pointer = self.pointer_of(behavior, origin, line)?;
                Some(Written { line, pointer })
            })
            .collect())
    }

    /// The path at the start of the reference text's `line`.
    fn pointer_of(
        &self,
        behavior: &dyn ConfigLanguage,
        origin: &str,
        line: usize,
    ) -> Option<String> {
        let content = self.origin.line_text(line);
        let indent = content.len() - content.trim_start().len();
        let at = self.origin.line_bounds(line).start + content[..indent].encode_utf16().count();
        path_at(behavior, origin, at).map(|path| path.to_pointer())
    }

    /// Whether the field covering `ranges` of the edited text is made of
    /// the reference text's lines, kept, with only closers added (a line
    /// changed by adding `}`, a new line holding only `]`, a line completed
    /// by the quote, `>` or separator it lacked, the strings of `value` then
    /// all written in the reference lines), and none of the
    /// reference text's lines inside it removed: those between its lines,
    /// and those `held` by it just around it.
    fn kept_with_closers(
        &self,
        behavior: &dyn ConfigLanguage,
        lines: &LineIndex<'_>,
        ranges: &[Range<usize>],
        value: &Value,
        held: &dyn Fn(usize) -> bool,
    ) -> bool {
        let (Some(first), Some(last)) = (ranges.first(), ranges.last()) else {
            return true;
        };
        let first_line = lines.line_of(first.start);
        let last_line = lines.line_of(last.end.saturating_sub(1).max(last.start));
        let mut changed: Vec<Part> = Vec::new();
        for range in ranges {
            let (start, end) = (lines.line_of(range.start), lines.line_of(range.end));
            for line in start..=end {
                if self.kept.get(line).copied().flatten().is_some() {
                    continue;
                }
                let bounds = lines.line_bounds(line);
                let from = range.start.max(bounds.start);
                let to = range.end.min(bounds.end).max(from);
                let text = lines.line_text(line);
                let chars = |units: usize| {
                    let units = units - bounds.start;
                    let mut seen = 0;
                    text.chars()
                        .take_while(|character| {
                            seen += character.len_utf16();
                            seen <= units
                        })
                        .count()
                };
                changed.push(Part::new(text, chars(from), chars(to)));
            }
        }
        // The reference text's lines removed inside the field or just around
        // it, which its changed lines may come from; those inside it, and
        // those around it that were the field's own, must reappear in it.
        // A field's lines are one stretch: around it, the lines nearest it
        // that it held, up to the first it did not.
        let code = |line: &usize| holds_code(behavior, self.origin.line_text(*line));
        let removed_before = |at: usize| -> Vec<usize> {
            self.removed
                .iter()
                .filter(|(_, before)| *before == at)
                .map(|(line, _)| *line)
                .filter(code)
                .collect()
        };
        let mut candidates: Vec<(usize, bool)> = self
            .removed
            .iter()
            .filter(|(line, before)| *before > first_line && *before <= last_line && code(line))
            .map(|(line, _)| (*line, true))
            .collect();
        let above = removed_before(first_line);
        let below = if last_line + 1 == first_line {
            Vec::new()
        } else {
            removed_before(last_line + 1)
        };
        for run in [above.into_iter().rev().collect::<Vec<_>>(), below] {
            let mut own = true;
            for line in run {
                own = own && held(line);
                candidates.push((line, own));
            }
        }
        let mut matched = vec![false; candidates.len()];
        let mut completed = false;
        for part in &changed {
            if part.only_closes() {
                continue;
            }
            let mut found = false;
            for (index, (line, _)) in candidates.iter().enumerate() {
                if let Some(completes) = part.closes(&compact(self.origin.line_text(*line))) {
                    found = true;
                    completed |= completes;
                    matched[index] = true;
                }
            }
            if !found {
                return false;
            }
        }
        let origin_lines: Vec<&str> = candidates
            .iter()
            .map(|(line, _)| self.origin.line_text(*line))
            .collect();
        if completed && !strings_written(value, &origin_lines) {
            return false;
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

/// Whether every string in `value` is written in `lines` as it reads.
fn strings_written(value: &Value, lines: &[&str]) -> bool {
    match value {
        Value::String(text) => lines.iter().any(|line| line.contains(text.as_str())),
        Value::Array(items) => items.iter().all(|item| strings_written(item, lines)),
        Value::Object(fields) => fields.iter().all(|(key, item)| {
            strings_written(&Value::String(key.clone()), lines) && strings_written(item, lines)
        }),
        _ => true,
    }
}
