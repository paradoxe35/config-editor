//! The values locks were pinned with, compared with the edited text.

use serde_json::Value;

use super::super::super::language::ConfigLanguage;
use super::super::carry::Moves;
use super::super::locate::{LineIndex, extent, lookup};
use super::super::locks::{Lock, Protected};

/// The first locked field whose value in `document` differs from the one
/// pinned, if any.
pub(super) fn pins_hold<'a>(document: &Value, protected: &'a Protected) -> Option<&'a str> {
    protected
        .pins
        .iter()
        .find(|(pointer, value)| document.pointer(pointer) != Some(value))
        .map(|(pointer, _)| pointer.as_str())
}

/// Whether the field of `lock` (found in a text with no value to pin) is
/// written in `after` (which reads as `document`) only within the lock's
/// own stretches, moved through the edit with their text kept: then it
/// reads as it did.
pub(super) fn reads_from_lock(
    behavior: &dyn ConfigLanguage,
    after: &str,
    lines: &LineIndex<'_>,
    document: &Value,
    moves: &Moves<'_>,
    lock: &Lock,
) -> bool {
    let Some(pointer) = lock.pointer.as_deref() else {
        return true;
    };
    let Some(value) = lookup(document, pointer) else {
        return false;
    };
    let (Some(moved), Some(found)) = (
        moves.lock(lock),
        extent(behavior, after, lines, Some(value), None, pointer),
    ) else {
        return false;
    };
    found.ranges.iter().all(|range| {
        moved
            .ranges
            .iter()
            .any(|kept| kept.start <= range.start && range.end <= kept.end)
    })
}

/// Whether every locked field has a pinned value to compare with (one
/// found in a text that never parsed has none: whitespace inside it is not
/// known to be free there).
pub(super) fn all_pinned(protected: &Protected) -> bool {
    protected
        .locks
        .iter()
        .all(|lock| lock.pointer.is_none() || is_pinned(protected, lock))
}

/// Whether `lock` has a value pinned to compare with.
pub(super) fn is_pinned(protected: &Protected, lock: &Lock) -> bool {
    protected
        .pins
        .iter()
        .any(|(pinned, _)| Some(pinned) == lock.pointer.as_ref())
}
