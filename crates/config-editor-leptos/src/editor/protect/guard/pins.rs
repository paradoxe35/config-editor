//! The values locks were pinned with, compared with the edited text.

use std::collections::HashSet;

use serde_json::Value;

use super::super::locks::Protected;
use super::super::texts::Reason;

/// The first locked field whose value in `document` differs from the one
/// pinned, if any.
pub(super) fn pins_hold<'a>(document: &Value, protected: &'a Protected) -> Option<&'a str> {
    protected
        .pins
        .iter()
        .find(|(pointer, value)| document.pointer(pointer) != Some(value))
        .map(|(pointer, _)| pointer.as_str())
}

/// The reason of the first field the language could not locate (it has no
/// span but its value) whose value in `document` differs from the one
/// pinned, if any.
pub(super) fn unplaced_hold<'a>(document: &Value, protected: &'a Protected) -> Option<&'a Reason> {
    protected
        .unplaced
        .iter()
        .find(|(pointer, _)| {
            protected
                .pins
                .iter()
                .find(|(pinned, _)| pinned == pointer)
                .is_some_and(|(_, value)| document.pointer(pointer) != Some(value))
        })
        .map(|(_, reason)| reason)
}

/// Whether every locked field has a value pinned to compare with (one
/// found in a text that never parsed has none: whitespace inside it is not
/// known to be free there).
pub(super) fn all_pinned(protected: &Protected) -> bool {
    let pointers: HashSet<&str> = protected
        .pins
        .iter()
        .map(|(pointer, _)| pointer.as_str())
        .collect();
    protected.locks.iter().all(|lock| {
        lock.pointer
            .as_deref()
            .is_none_or(|pointer| pointers.contains(pointer))
    })
}
