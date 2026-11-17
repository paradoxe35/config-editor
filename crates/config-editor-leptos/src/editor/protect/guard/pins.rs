//! Whether every lock has the value it was pinned with.

use std::collections::HashSet;

use super::super::locks::Protected;

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
