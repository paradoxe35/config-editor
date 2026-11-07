//! Whether an edit keeps every lock: one check for every way text changes
//! (typing, paste, cut, delete, replace, completions, Revert, comment
//! toggling, undo), given the text before and after.
//!
//! An edit is refused when
//! - it changes a locked stretch's text, or a key holding a locked field;
//! - it glues text onto a locked stretch's ends (`port: 80` → `port: 808`,
//!   `xport: 80`); whitespace and punctuation next to it are free;
//! - the document still parses afterwards and a locked field's value there
//!   differs (it moved under another parent, a comment took it, a line
//!   added below joined a locked block).
//!
//! Formatting around a locked field stays free: blank lines, trailing
//! spaces, comments after the value, and re-indenting where indentation
//! carries no structure (JSON, TOML, HCL, XML). In YAML a changed indent
//! moves the field, which the parsed check refuses.

use std::ops::Range;

use serde_json::Value;

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::locate::LineIndex;
use super::locks::{Lock, Protected};

/// An edit that would change a locked field, and why it is locked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Blocked {
    pub(crate) reason: String,
}

/// Checks the edit turning `before` (whose locks are `protected`) into
/// `after`.
pub(crate) fn check(
    behavior: &dyn ConfigLanguage,
    reading_schema: Option<&Value>,
    protected: &Protected,
    before: &str,
    after: &str,
) -> Result<(), Blocked> {
    if protected.is_empty() || before == after {
        return Ok(());
    }
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let lines = LineIndex::new(before);
    // An edit changing several places (Replace All, a comment toggle over
    // lines): a stretch on lines it left whole is kept too.
    let unchanged = std::cell::OnceCell::new();
    let kept = |range: &Range<usize>| {
        kept(range, &old, &new, prefix, suffix)
            || unchanged
                .get_or_init(|| unchanged_lines(before, after))
                .as_ref()
                .is_some_and(|same: &Vec<bool>| {
                    (lines.line_of(range.start)..=lines.line_of(range.end.saturating_sub(1)))
                        .all(|line| same.get(line).copied().unwrap_or(false))
                })
    };
    for lock in &protected.locks {
        if !lock.ranges.iter().all(kept) {
            return Err(Blocked {
                reason: lock.reason.clone(),
            });
        }
        if !lock.guards.iter().all(kept) {
            return Err(Blocked {
                reason: super::texts::holds_locked(&lock.reason),
            });
        }
    }
    // The region the edit replaced, read as narrowly as the texts allow.
    let tail = suffix
        .min(old.len() - prefix.min(old.len()))
        .min(new.len() - prefix.min(new.len()));
    let region = prefix.min(old.len())..old.len() - tail;
    let near: Vec<&Lock> = protected
        .locks
        .iter()
        .filter(|lock| lock.pointer.is_some() && is_near(lock, &region, &lines))
        .collect();
    if near.is_empty() {
        return Ok(());
    }
    let compared = protected.document.as_ref().and_then(|document| {
        let edited = codec::read(behavior, after, reading_schema).ok()?;
        Some((document, edited))
    });
    for lock in near {
        let unchanged = match &compared {
            Some((document, edited)) => {
                let pointer = lock.pointer.as_deref().unwrap_or_default();
                document.pointer(pointer) == edited.pointer(pointer)
            }
            // Without a parse to compare, a locked line keeps its indent:
            // in YAML it is structure.
            None => lock
                .ranges
                .iter()
                .all(|range| indent_kept(range, &old, &new, prefix, suffix)),
        };
        if !unchanged {
            return Err(Blocked {
                reason: lock.reason.clone(),
            });
        }
    }
    Ok(())
}

/// Whether the text from its line's start to a locked stretch (its indent)
/// reads the same after the edit.
fn indent_kept(
    range: &Range<usize>,
    old: &[u16],
    new: &[u16],
    prefix: usize,
    suffix: usize,
) -> bool {
    let start = range.start;
    let moved = if range.end <= prefix {
        start
    } else if start >= old.len().saturating_sub(suffix) {
        start + new.len() - old.len()
    } else {
        return false;
    };
    let line_start = |units: &[u16], at: usize| {
        units[..at]
            .iter()
            .rposition(|unit| *unit == u16::from(b'\n'))
            .map_or(0, |found| found + 1)
    };
    old[line_start(old, start)..start] == new[line_start(new, moved)..moved]
}

/// For each line of `before`, whether a line diff to `after` keeps it as
/// it is; `None` when the texts differ too much to tell.
fn unchanged_lines(before: &str, after: &str) -> Option<Vec<bool>> {
    use super::super::diff::{DiffEdit, myers_edits};
    let old: Vec<&str> = before.split('\n').collect();
    let new: Vec<&str> = after.split('\n').collect();
    let edits = myers_edits(&old, &new)?;
    let mut same = Vec::with_capacity(old.len());
    for edit in edits {
        match edit {
            DiffEdit::Equal => same.push(true),
            DiffEdit::Delete => same.push(false),
            DiffEdit::Insert => {}
        }
    }
    Some(same)
}

/// Whether `range` of the old text survives the edit untouched: it lies in
/// the common prefix or suffix, and nothing new is glued onto its ends.
fn kept(range: &Range<usize>, old: &[u16], new: &[u16], prefix: usize, suffix: usize) -> bool {
    if range.start >= range.end {
        return true;
    }
    let last = old[range.end - 1];
    let in_prefix =
        range.end <= prefix && neighbour_ok(last, old.get(range.end), new.get(range.end));
    if in_prefix {
        return true;
    }
    let tail_start = old.len().saturating_sub(suffix);
    if range.start >= tail_start {
        let shifted = range.start + new.len() - old.len();
        let before = range.start.checked_sub(1).and_then(|at| old.get(at));
        let after = shifted.checked_sub(1).and_then(|at| new.get(at));
        return neighbour_ok(old[range.start], before, after);
    }
    false
}

/// A character next to a locked stretch (whose character on that side is
/// `edge`) may change only to one that cannot join it: whitespace,
/// punctuation, or nothing. Next to a stretch ending in a blank or
/// punctuation, anything may go.
fn neighbour_ok(edge: u16, old: Option<&u16>, new: Option<&u16>) -> bool {
    old == new || !glues(edge) || new.is_none_or(|unit| !glues(*unit))
}

/// Whether a character written next to text would read as part of it.
fn glues(unit: u16) -> bool {
    match char::from_u32(u32::from(unit)) {
        Some(character) => !character.is_whitespace() && !",;:=(){}[]<>#/".contains(character),
        // Half of a surrogate pair: some symbol, part of a word.
        None => true,
    }
}

/// Whether the edit's `region` is on or next to the lines of `lock` (or
/// between it and the keys holding it), where it could move the field.
fn is_near(lock: &Lock, region: &Range<usize>, lines: &LineIndex<'_>) -> bool {
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
