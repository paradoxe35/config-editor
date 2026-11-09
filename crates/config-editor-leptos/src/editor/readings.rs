//! The readings of the whole document that the next frame can show a moment
//! late: its multi-line constructs and string bodies (the code layer's
//! colors past the edited line), the fold markers and the change bars.
//!
//! A keystroke that keeps the lines (typing inside one, the common case)
//! leaves them as they were until that frame is drawn, then makes them
//! again: the typed text shows without waiting for a read of the whole
//! document, and keys coming faster than frames are read once per frame.
//! Lines stay where they were, so what each line shows of them is still
//! right but where the keystroke itself changed one (a quote opening a
//! string over lines, a line now changed against the baseline), which the
//! next frame shows. An edit adding or removing lines makes them at once.
//!
//! What acts on them (a key, a fold or change command, the handle) makes
//! them first if they are due ([`EditorState::settle_readings`]), so it
//! never acts on a stale one.

use leptos::prelude::*;

use super::state::EditorState;
use crate::reactive::later;

/// Whether readings are due, and a count of the times they were made,
/// which the readings follow.
#[derive(Clone, Copy)]
pub(super) struct Readings {
    due: StoredValue<bool>,
    made: RwSignal<u64>,
}

impl Readings {
    pub(super) fn new() -> Self {
        Self {
            due: StoredValue::new(false),
            made: RwSignal::new(0),
        }
    }

    /// A reading of the whole of `text` that `make` makes (and tracks what
    /// else it reads): while readings are due it stays as it was.
    pub(super) fn memo<T>(
        self,
        text: RwSignal<String>,
        make: impl Fn() -> T + Send + Sync + 'static,
    ) -> Memo<T>
    where
        T: PartialEq + Send + Sync + 'static,
    {
        Memo::new_owning(move |previous: Option<T>| {
            self.made.track();
            match previous {
                Some(previous) if self.due.get_value() => {
                    // Made again when the text changes otherwise too.
                    text.track();
                    (previous, false)
                }
                previous => {
                    let made = make();
                    let changed = previous.as_ref() != Some(&made);
                    (made, changed)
                }
            }
        })
    }

    /// Makes the readings now if they are due.
    fn settle(self) {
        if self.due.try_get_value() == Some(true) {
            self.due.set_value(false);
            self.made.update(|made| *made += 1);
        }
    }
}

/// The line breaks in `text`.
fn line_breaks(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count()
}

impl EditorState {
    /// The document is about to change from `before` to `after`: an edit
    /// keeping the lines leaves the readings for after the next frame,
    /// any other makes them at once.
    pub(super) fn note_text_change(&self, before: &str, after: &str) {
        let readings = self.doc().readings;
        if line_breaks(before) != line_breaks(after) {
            readings.settle();
            return;
        }
        if readings.due.get_value() {
            return;
        }
        readings.due.set_value(true);
        // After the frame is drawn: a frame's callback runs before it is.
        later::next_frame(*self, |state| {
            later::after(state, 0, |state| state.settle_readings());
        });
    }

    /// Makes the readings now if they are due: before anything acts on
    /// them.
    pub(super) fn settle_readings(&self) {
        self.doc().readings.settle();
    }
}
