//! The values locks were pinned with, compared with the edited text.

use serde_json::Value;

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
