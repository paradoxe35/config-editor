//! The lock tip: what the protected text under the pointer is (once the
//! pointer rests there, as the card and the problem tip; at once for the
//! caret), or why an edit there was refused (at the caret at once, for a
//! moment). Its place and style are `caret_tip`'s; it is said to screen
//! readers by the protect view.

use leptos::prelude::*;

use super::PopupState;
use crate::reactive::later::{self, Timeout};

/// How long a refused edit's tip stays, in milliseconds.
const REFUSAL_MS: i32 = 2500;

/// A short message at a place in the text: why an edit was refused, or
/// what the hovered protected text is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LockTip {
    pub(crate) message: String,
    /// UTF-16 offset it points at.
    pub(crate) offset: usize,
    pub(crate) cause: Cause,
}

/// What showed a lock tip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    /// An edit there was refused: it shows for a moment.
    Refused,
    /// The pointer rested there: it goes when the pointer leaves.
    Pointer,
    /// The caret (a tap, `Ctrl+K Ctrl+I`): the pointer leaving the text
    /// keeps it (a tap's pointer leaves at once).
    Caret,
}

impl LockTip {
    /// Whether it says what the protected text is (for the pointer or the
    /// caret), not why an edit was refused.
    pub(crate) fn hover(&self) -> bool {
        self.cause != Cause::Refused
    }
}

impl PopupState {
    /// The lock tip, while one shows (tracked).
    pub(crate) fn lock_tip(&self) -> Option<LockTip> {
        self.lock.get()
    }

    /// The lock tip, while one shows.
    #[cfg(test)]
    pub(crate) fn lock_tip_untracked(&self) -> Option<LockTip> {
        self.lock.get_untracked()
    }

    /// Shows `tip` in place of the lock tip there, going after `timer`
    /// (the one before cancelled).
    fn show_lock(&self, tip: Option<LockTip>, timer: Timeout) {
        later::replace_in(self.lock_timer, timer);
        if self.lock.with_untracked(|shown| *shown != tip) {
            self.lock.set(tip);
        }
    }

    /// Shows why an edit at `offset` was refused, for a moment.
    pub(crate) fn flash_lock(&self, message: String, offset: usize) {
        let tip = LockTip {
            message,
            offset,
            cause: Cause::Refused,
        };
        // Native unit tests have no clock: there it stays (unless a test
        // holds the timers).
        let timer = if later::defers() {
            later::after(self.lock, REFUSAL_MS, |lock| lock.set(None))
        } else {
            Timeout::default()
        };
        self.show_lock(Some(tip), timer);
    }

    /// Whether the lock tip shown says what protected text with `message`
    /// is.
    pub(crate) fn hover_lock_shows(&self, message: &str) -> bool {
        self.lock.with_untracked(|shown| {
            shown
                .as_ref()
                .is_some_and(|shown| shown.hover() && shown.message == message)
        })
    }

    /// Shows what the protected text at `offset` is, for `cause` (the
    /// pointer or the caret): the same message stays where it opened.
    pub(crate) fn hover_lock(&self, message: String, offset: usize, cause: Cause) {
        let same = self.lock.with_untracked(|shown| {
            shown
                .as_ref()
                .is_some_and(|shown| shown.cause == cause && shown.message == message)
        });
        if !same {
            let tip = LockTip {
                message,
                offset,
                cause,
            };
            self.show_lock(Some(tip), Timeout::default());
        }
    }

    /// The pointer left the text: a lock tip it rested for goes.
    pub(crate) fn leave_lock(&self) {
        let pointer = self.lock.with_untracked(|shown| {
            shown
                .as_ref()
                .is_some_and(|shown| shown.cause == Cause::Pointer)
        });
        if pointer {
            self.hide_lock(true);
        }
    }

    /// Hides the lock tip (only one shown for the pointer with
    /// `hover_only`).
    pub(crate) fn hide_lock(&self, hover_only: bool) {
        let shown = self
            .lock
            .with_untracked(|shown| shown.as_ref().map(LockTip::hover));
        if shown.is_some_and(|hover| hover || !hover_only) {
            self.show_lock(None, Timeout::default());
        }
        // The pointer going on to the next popup soon rests briefly.
        if shown == Some(true) {
            self.machine
                .update_untracked(|machine| machine.note_closed(super::now()));
        }
    }
}
