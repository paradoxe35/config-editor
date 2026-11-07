//! Typing aids tied to the caret's place: closers the editor wrote, the
//! whole line last copied, and the input the browser sent but the editor
//! has not applied yet (see `input`).

use leptos::prelude::*;

use super::history::{EditHistory, Selection};
use super::input::{InputBurst, UNKNOWN_SELECTION};
use super::state::EditorState;

/// Closing text the editor wrote after the caret: a bracket, a quote, or
/// markup such as an XML end tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Closer {
    /// UTF-16 distance from the document end to where it starts; typing
    /// before it does not change it.
    pub(super) distance: usize,
    /// UTF-16 length.
    pub(super) len: usize,
    /// For a closer that ends a value (a string, an element's text): the
    /// character just before the value (`"`, the `>` of `<k>`). Once the text
    /// before the caret no longer ends with it, a value was typed, and Enter
    /// steps past the closer. `None` for brackets, which Enter opens instead.
    pub(super) opener: Option<char>,
}

/// The typing part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct TypingState {
    /// Closers the editor inserted. Only these are typed over, deleted with
    /// their opener, or stepped past by Enter.
    pub(super) closers: StoredValue<Vec<Closer>>,
    /// The line last copied or cut with nothing selected, to paste it whole.
    pub(super) copied_line: StoredValue<Option<String>>,
    /// The selection just before the browser applies an edit, for undo.
    pub(super) pre_edit_selection: StoredValue<Selection>,
    /// Input events not applied yet (see `input::flush_input`).
    pub(super) input_burst: StoredValue<Option<InputBurst>>,
}

impl TypingState {
    pub(super) fn new() -> Self {
        Self {
            closers: StoredValue::new(Vec::new()),
            copied_line: StoredValue::new(None),
            pre_edit_selection: StoredValue::new(UNKNOWN_SELECTION),
            input_burst: StoredValue::new(None),
        }
    }
}

impl EditorState {
    /// The inserted closer starting at UTF-16 `offset`, if any.
    pub(super) fn closer_at(&self, offset: usize) -> Option<Closer> {
        let distance = self
            .doc()
            .text
            .with_untracked(|text| text.encode_utf16().count())
            .checked_sub(offset)?;
        self.typing().closers.with_value(|closers| {
            closers
                .iter()
                .find(|closer| closer.distance == distance)
                .copied()
        })
    }

    /// Forgets the inserted closer starting at UTF-16 `offset`.
    pub(super) fn forget_closer_at(&self, offset: usize) {
        let distance = self
            .doc()
            .text
            .with_untracked(|text| text.encode_utf16().count())
            .saturating_sub(offset);
        self.typing()
            .closers
            .update_value(|closers| closers.retain(|closer| closer.distance != distance));
    }

    /// Remembers a closer written at UTF-16 `offset` of `text` (the document
    /// after the edit that wrote it).
    pub(super) fn remember_closer(
        &self,
        text: &str,
        offset: usize,
        len: usize,
        opener: Option<char>,
    ) {
        let distance = text.encode_utf16().count() - offset;
        self.typing().closers.update_value(|closers| {
            closers.push(Closer {
                distance,
                len,
                opener,
            });
        });
    }

    /// Forgets the typing aids tied to the caret's place (inserted closers,
    /// placeholder slots) and ends the undo group.
    pub(super) fn forget_typing_aids(&self) {
        self.typing().closers.update_value(Vec::clear);
        self.completion().slot_region.set_value(None);
        self.completion().value_stops.update_value(Vec::clear);
        self.doc().history.update(EditHistory::seal);
    }
}
