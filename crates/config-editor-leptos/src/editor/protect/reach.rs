//! An edit out of every lock's reach: it changes one scalar written on one
//! line (the language says so, see `ConfigLanguage::edits_one_scalar`) in a
//! text that parses, no lock, nor a key holding one, is on that line, and
//! the path written there is no locked field's, nor inside or holding one.
//! Every locked value then reads as it did, so the edit is kept without
//! reading the document again, and the locks move past it by its length:
//! a lock costs a keystroke away from it nothing.

use std::collections::HashSet;
use std::ops::Range;

use super::super::language::ConfigLanguage;
use super::super::parse_cache::{line_path, tell_line_path};
use super::super::paths::path_at;
use super::super::text_change::{changed_bytes, utf16_len};
use super::locate::within;
use super::locks::{Lock, Protected};

/// How the offsets of the text before an edit out of reach move: those
/// from `from` on (UTF-16) by `by`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shift {
    from: usize,
    by: isize,
}

impl Shift {
    /// `range`, where it is after the edit.
    pub(super) fn moved_range(&self, range: &Range<usize>) -> Range<usize> {
        let at = |offset: usize| {
            if offset >= self.from {
                offset.saturating_add_signed(self.by)
            } else {
                offset
            }
        };
        at(range.start)..at(range.end)
    }

    /// `lock`, where it is after the edit.
    pub(super) fn moved(&self, lock: &Lock) -> Lock {
        let range = |range: &Range<usize>| self.moved_range(range);
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
    let (start, old_end, new_end) = changed_bytes(before, after);
    let inserted = &after[start..new_end];
    if !behavior.edits_one_scalar(before, start..old_end, inserted) {
        return None;
    }
    let line_start = before[..start].rfind('\n').map_or(0, |at| at + 1);
    let line_end = before[old_end..]
        .find('\n')
        .map_or(before.len(), |at| old_end + at);
    let first = utf16_len(&before[..line_start]);
    let last = first + utf16_len(&before[line_start..line_end]);
    let touches = |range: &Range<usize>| range.start <= last && range.end >= first;
    let reached = protected.locks.iter().any(|lock| {
        let hull = lock
            .ranges
            .iter()
            .map(|range| range.start)
            .min()
            .zip(lock.ranges.iter().map(|range| range.end).max());
        hull.is_some_and(|(low, high)| touches(&(low..high)))
            || lock.key.as_ref().is_some_and(touches)
            || lock.guards.iter().any(touches)
    });
    if reached {
        return None;
    }
    let line = &before[line_start..line_end];
    let code = first + utf16_len(&line[..line.len() - line.trim_start().len()]);
    let read = || path_at(behavior, before, code).map(|path| path.to_pointer());
    let path = line_path(behavior, before, first, read)?;
    if reads_a_lock(behavior, protected, &path) {
        return None;
    }
    // The line after the edit writes the same path: the next key typed on
    // it asks for it there.
    tell_line_path(behavior, after, first, Some(path));
    let by = utf16_len(inserted) as isize - utf16_len(&before[start..old_end]) as isize;
    Some(Shift { from: last, by })
}

/// Whether `path` (the edited line's) is, holds or is inside a locked field:
/// where a lock is written and what the text reads on the line are found
/// apart, and a text that fools one (an HCL string over lines holding
/// `id = 1`) does not hand the other's field to the fast path.
fn reads_a_lock(behavior: &dyn ConfigLanguage, protected: &Protected, path: &str) -> bool {
    protected
        .locks
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
