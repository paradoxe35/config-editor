//! Whether an edit keeps every lock: one check for every way text changes
//! (typing, paste, cut, delete, replace, completions, Revert, comment
//! toggling, undo), given the text before and after.
//!
//! An edit is refused when
//! - it changes a locked stretch's text, or a key holding a locked field;
//! - it glues text onto a locked stretch's ends (`port: 80` → `port: 808`,
//!   `xport: 80`); whitespace and punctuation next to it are free;
//! - the document still parses afterwards and a locked field's value there
//!   differs from the value pinned when the lock was last read (it moved
//!   under another parent, a comment took it, a line added below joined a
//!   locked block, a TOML table or dotted key added elsewhere extended it);
//! - the document does not parse afterwards and the edit adds a line that
//!   would join a locked field once it does: a line whose path is inside
//!   it, or a second key holding it (duplicates are errors here, but a
//!   host's parser may let the last one win).
//!
//! Formatting around a locked field stays free: blank lines, trailing
//! spaces, comments after the value, and re-indenting where indentation
//! carries no structure (JSON, TOML, HCL, XML). In YAML a changed indent
//! moves the field, which the parsed check refuses.

use std::ops::Range;

use serde_json::Value;

use super::super::codec;
use super::super::language::ConfigLanguage;
use super::super::paths::path_at;
use super::locate::{LineIndex, within};
use super::locks::{Lock, Protected};
use super::texts::Reason;
use crate::EditorStrings;

/// An edit that would change a locked field, and why it is locked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Blocked {
    pub(crate) reason: Reason,
    /// It renames or moves a key holding the locked field.
    pub(crate) holds: bool,
}

impl Blocked {
    /// Why the edit is refused, as the tooltip says it.
    pub(crate) fn message(&self, strings: &EditorStrings) -> String {
        if self.holds {
            self.reason.holding(strings)
        } else {
            self.reason.text(strings)
        }
    }
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
                holds: false,
            });
        }
        if !lock.guards.iter().all(kept) {
            return Err(Blocked {
                reason: lock.reason.clone(),
                holds: true,
            });
        }
    }
    // The region the edit replaced, read as narrowly as the texts allow.
    let tail = suffix
        .min(old.len() - prefix.min(old.len()))
        .min(new.len() - prefix.min(new.len()));
    let region = prefix.min(old.len())..old.len() - tail;
    let new_region = prefix.min(new.len())..new.len() - tail;
    let pointed: Vec<&Lock> = protected
        .locks
        .iter()
        .filter(|lock| lock.pointer.is_some())
        .collect();
    if pointed.is_empty() {
        return Ok(());
    }
    if let Ok(edited) = codec::read(behavior, after, reading_schema) {
        // The edited text parses: every locked value must read as it was
        // pinned, wherever the edit was (a TOML `[web.sub]` table added at
        // the end changes `web`).
        for (pointer, value) in &protected.pins {
            if edited.pointer(pointer) != Some(value) {
                let lock = pointed
                    .iter()
                    .find(|lock| lock.pointer.as_deref() == Some(pointer.as_str()));
                return Err(Blocked {
                    reason: lock.map_or(Reason::Field, |lock| lock.reason.clone()),
                    holds: false,
                });
            }
        }
        return Ok(());
    }
    // The edited text does not parse, so its values cannot be compared.
    // Nothing may be written where it would join a locked field once it
    // parses (a key half typed inside a locked block, a duplicate of a
    // locked key or of a key holding one): the edit may not add a line
    // whose path is inside a locked field, or which names a key holding it.
    let new_lines = LineIndex::new(after);
    for lock in &pointed {
        let pointer = lock.pointer.as_deref().unwrap_or_default();
        let count = |text: &str, lines: &LineIndex<'_>, region: &Range<usize>| {
            joining_lines(behavior, text, lines, region, pointer)
        };
        let (Some(was), Some(now)) = (
            count(before, &lines, &region),
            count(after, &new_lines, &new_region),
        ) else {
            return Err(Blocked {
                reason: lock.reason.clone(),
                holds: false,
            });
        };
        if now > was {
            return Err(Blocked {
                reason: lock.reason.clone(),
                holds: false,
            });
        }
    }
    // Without a parse to compare, a locked line keeps its indent: in YAML
    // it is structure.
    for lock in pointed {
        if is_near(lock, &region, &lines)
            && !lock
                .ranges
                .iter()
                .all(|range| indent_kept(range, &old, &new, prefix, suffix))
        {
            return Err(Blocked {
                reason: lock.reason.clone(),
                holds: false,
            });
        }
    }
    Ok(())
}

/// Lines of an edited region checked one by one at most; a larger edit
/// leaving the text unreadable next to a lock is refused.
const MAX_REGION_LINES: usize = 2000;

/// How many lines touching `region` of `text` would join the field at
/// `pointer`: lines whose path is inside it, and lines naming a key that
/// holds it (a second `web:` above a locked `web/port`). Blank and comment
/// lines join nothing. `None` when the region is too large to check.
fn joining_lines(
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
