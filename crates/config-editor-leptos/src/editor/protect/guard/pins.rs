//! The values locks were pinned with, compared with the edited text.

use std::collections::HashSet;

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

/// The fields with a value pinned to compare with, looked up by pointer
/// (a lookup per lock, where searching the pins for each was a pass over
/// every lock for each lock).
pub(super) struct Pinned<'a> {
    pointers: HashSet<&'a str>,
    /// Whether every locked field has one (one found in a text that never
    /// parsed has none: whitespace inside it is not known to be free
    /// there).
    all: bool,
}

impl<'a> Pinned<'a> {
    pub(super) fn new(protected: &'a Protected) -> Self {
        let pointers: HashSet<&str> = protected
            .pins
            .iter()
            .map(|(pointer, _)| pointer.as_str())
            .collect();
        let all = protected.locks.iter().all(|lock| {
            lock.pointer
                .as_deref()
                .is_none_or(|pointer| pointers.contains(pointer))
        });
        Self { pointers, all }
    }

    /// Whether `lock` has a value pinned to compare with.
    pub(super) fn holds(&self, lock: &Lock) -> bool {
        lock.pointer
            .as_deref()
            .is_some_and(|pointer| self.pointers.contains(pointer))
    }

    /// Whether every locked field has a pinned value.
    pub(super) fn all(&self) -> bool {
        self.all
    }
}
