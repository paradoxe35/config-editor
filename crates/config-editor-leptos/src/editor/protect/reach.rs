//! An edit out of every lock's reach: it changes one scalar written on one
//! line (the language says so, see `SaveLayout::edits_one_scalar`) in a
//! text that parses, no lock, nor a key holding one, is on that line, and
//! the path written there is no locked field's, nor inside or holding one.
//! Every locked value then reads as it did, so the edit is kept without
//! reading the document again, and the locks move past it by its length:
//! a lock costs a keystroke away from it nothing.

use std::collections::HashSet;

use super::super::language::ConfigLanguage;
use super::super::parse_cache::{line_path, tell_line_path};
use super::super::paths::{api_units, path_at};
use super::super::text::{TextRange, TextSize, slice, text_size};
use super::super::text_change::changed_bytes;
use super::locate::within;
use super::locks::{Lock, Protected};

/// How the offsets of the text before an edit out of reach move: those
/// from `from` on by `inserted - removed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shift {
    from: TextSize,
    inserted: TextSize,
    removed: TextSize,
}

impl Shift {
    /// `range`, where it is after the edit.
    pub(super) fn moved_range(&self, range: &TextRange) -> TextRange {
        // At or past `from`, an offset is past the text the edit removed.
        let at = |offset: TextSize| {
            if offset >= self.from {
                offset + self.inserted - self.removed
            } else {
                offset
            }
        };
        TextRange::new(at(range.start()), at(range.end()))
    }

    /// `lock`, where it is after the edit.
    pub(super) fn moved(&self, lock: &Lock) -> Lock {
        let range = |range: &TextRange| self.moved_range(range);
        Lock {
            pointer: lock.pointer.clone(),
            key: lock.key.as_ref().map(range),
            ranges: lock.ranges.iter().map(range).collect(),
            guards: lock.guards.iter().map(range).collect(),
            reason: lock.reason.clone(),
        }
    }
}

/// How the locks of `before` (`protected`) move through the edit making
/// `after`, when it is out of every lock's reach; `None` when it may be in
/// one's (the document is then read).
pub(super) fn out_of_reach(
    behavior: &dyn ConfigLanguage,
    protected: &Protected,
    before: &str,
    after: &str,
) -> Option<Shift> {
    if protected.document.is_none() || !protected.unplaced.is_empty() || !all_pinned(protected) {
        return None;
    }
    reaches_none(behavior, &protected.locks, before, after)
}

/// How offsets move through the edit making `after` of `before` (a text
/// that parses), when it changes one scalar on a line none of `locks` is
/// on, whose path is none of theirs, nor inside or holding one; `None`
/// when it may reach one.
pub(super) fn reaches_none(
    behavior: &dyn ConfigLanguage,
    locks: &[Lock],
    before: &str,
    after: &str,
) -> Option<Shift> {
    let on_lines = |lines: &TextRange| {
        let touches =
            |range: &TextRange| range.start() <= lines.end() && range.end() >= lines.start();
        locks.iter().any(|lock| {
            let hull = lock
                .ranges
                .iter()
                .map(|range| range.start())
                .min()
                .zip(lock.ranges.iter().map(|range| range.end()).max());
            hull.is_some_and(|(low, high)| touches(&TextRange::new(low, high)))
                || lock.key.as_ref().is_some_and(touches)
                || lock.guards.iter().any(touches)
        })
    };
    let reads = |path: &str| reads_a_lock(behavior, locks, path);
    reaches_none_by(behavior, before, after, on_lines, reads)
}

/// [`reaches_none`] for what is held asked of `on_lines` (whether a held
/// place is on the lines `first..last`, ends included) and `reads`
/// (whether a path is, holds or is inside a held field).
pub(super) fn reaches_none_by(
    behavior: &dyn ConfigLanguage,
    before: &str,
    after: &str,
    on_lines: impl Fn(&TextRange) -> bool,
    reads: impl Fn(&str) -> bool,
) -> Option<Shift> {
    let (start, old_end, new_end) = changed_bytes(before, after);
    let (start, old_end, new_end) = (text_size(start), text_size(old_end), text_size(new_end));
    let inserted = slice::slice(after, TextRange::new(start, new_end))?;
    let removed = TextRange::new(start, old_end);
    if !behavior.edits_one_scalar(before, removed.into(), inserted) {
        return None;
    }
    let line_start = slice::before(before, start)?
        .rfind('\n')
        .map_or(TextSize::default(), |at| text_size(at + 1));
    let line_end = slice::after(before, old_end)?
        .find('\n')
        .map_or(TextSize::of(before), |at| old_end + text_size(at));
    let lines = TextRange::new(line_start, line_end);
    if on_lines(&lines) {
        return None;
    }
    let line = slice::slice(before, lines)?;
    let code = line_start + TextSize::of(line) - TextSize::of(line.trim_start());
    // The public language API takes UTF-16 carets.
    let read = || {
        path_at(behavior, before, api_units(before, usize::from(code)))
            .map(|path| path.to_pointer())
    };
    let path = line_path(behavior, before, line_start, read)?;
    if reads(&path) {
        return None;
    }
    // The line after the edit writes the same path: the next key typed on
    // it asks for it there.
    tell_line_path(behavior, after, line_start, Some(path));
    Some(Shift {
        from: line_end,
        inserted: TextSize::of(inserted),
        removed: removed.len(),
    })
}
/// Whether `path` (the edited line's) is, holds or is inside a locked field:
/// where a lock is written and what the text reads on the line are found
/// apart, and a text that fools one (an HCL string over lines holding
/// `id = 1`) does not hand the other's field to the fast path.
fn reads_a_lock(behavior: &dyn ConfigLanguage, locks: &[Lock], path: &str) -> bool {
    locks
        .iter()
        .filter_map(|lock| lock.pointer.as_deref())
        .any(|pointer| within(behavior, path, pointer) || within(behavior, pointer, path))
}

/// Whether every locked field has its value pinned, to compare with.
fn all_pinned(protected: &Protected) -> bool {
    let pinned: HashSet<&str> = protected
        .pins
        .iter()
        .map(|(pointer, _)| pointer.as_str())
        .collect();
    protected
        .locks
        .iter()
        .filter_map(|lock| lock.pointer.as_deref())
        .all(|pointer| pinned.contains(pointer))
}
