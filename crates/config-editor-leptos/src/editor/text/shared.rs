//! [`SharedText`]: a whole text in a signal, held as one `Arc<str>` that
//! the other places holding the same text share (the host's text, the
//! host log, what validation read), so an open editor keeps one copy of
//! its document, not one per place.

use std::sync::Arc;

use leptos::prelude::*;

/// A text in a signal, shared: [`SharedText::shared`] hands out the copy
/// itself, the `get` methods a `String` of it for code that changes it.
#[derive(Clone, Copy)]
pub(crate) struct SharedText(RwSignal<Arc<str>>);

impl SharedText {
    pub(crate) fn new(text: Arc<str>) -> Self {
        Self(RwSignal::new(text))
    }

    /// Runs `read` with the text, tracked.
    pub(crate) fn with<T>(&self, read: impl FnOnce(&str) -> T) -> T {
        self.0.with(|text| read(text))
    }

    /// Runs `read` with the text, untracked.
    pub(crate) fn with_untracked<T>(&self, read: impl FnOnce(&str) -> T) -> T {
        self.0.with_untracked(|text| read(text))
    }

    /// Runs `read` with the text, untracked; `None` once the signal is
    /// gone.
    pub(crate) fn try_with_untracked<T>(&self, read: impl FnOnce(&str) -> T) -> Option<T> {
        self.0.try_with_untracked(|text| read(text))
    }

    /// The text itself, shared, tracked.
    pub(crate) fn shared(&self) -> Arc<str> {
        self.0.get()
    }

    /// The text itself, shared, untracked.
    pub(crate) fn shared_untracked(&self) -> Arc<str> {
        self.0.get_untracked()
    }

    /// A copy of the text, tracked.
    pub(crate) fn get(&self) -> String {
        self.with(str::to_string)
    }

    /// A copy of the text, untracked.
    pub(crate) fn get_untracked(&self) -> String {
        self.with_untracked(str::to_string)
    }

    /// A copy of the text, untracked; `None` once the signal is gone.
    pub(crate) fn try_get_untracked(&self) -> Option<String> {
        self.0.try_with_untracked(|text| text.to_string())
    }

    /// Subscribes to the text.
    pub(crate) fn track(&self) {
        self.0.track();
    }

    /// Sets the text.
    pub(crate) fn set(&self, text: impl Into<Arc<str>>) {
        self.0.set(text.into());
    }

    /// Sets the text, returning the one it replaced.
    pub(crate) fn replace(&self, text: Arc<str>) -> Arc<str> {
        let mut before = text;
        self.0.update(|held| std::mem::swap(held, &mut before));
        before
    }
}

/// `text`, sharing `held` when that is the same text (compared: two
/// places share a copy only when they hold the same text).
pub(crate) fn same_or_new(held: &Arc<str>, text: &str) -> Arc<str> {
    if **held == *text {
        held.clone()
    } else {
        Arc::from(text)
    }
}
