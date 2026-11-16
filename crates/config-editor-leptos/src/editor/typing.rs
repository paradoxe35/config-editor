//! Typing aids tied to the caret's place: closers the editor wrote, the
//! whole line last copied, and the input the browser sent but the editor
//! has not applied yet (see `input`).

use leptos::prelude::*;

use super::drag_move::DragDelete;
use super::edit::{Assoc, Change};
use super::history::{EditHistory, Selection};
use super::input::{InputBurst, UNKNOWN_SELECTION};
use super::state::EditorState;

/// Closing text the editor wrote after the caret: a bracket, a quote, or
/// markup such as an XML end tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Closer {
    /// UTF-16 offset where it starts, mapped through every edit (see
    /// [`EditorState::follow_typing_aids`]): typing before it moves it on.
    pub(super) start: usize,
    /// UTF-16 length.
    pub(super) len: usize,
    /// For a closer that ends a value (a string, an element's text): the
    /// character just before the value (`"`, the `>` of `<k>`). Once the text
    /// before the caret no longer ends with it, a value was typed, and Enter
    /// steps past the closer. `None` for brackets, which Enter opens instead.
    pub(super) opener: Option<char>,
    /// Markup a typed character finished (`</` writing `tcp>`): each of
    /// its characters is typed over, not only closing brackets and quotes.
    pub(super) completion: bool,
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
    /// `Ctrl+K` was pressed: the next key may finish a chord (see
    /// `keys::chord`).
    pub(super) chord: StoredValue<bool>,
    /// The delete of a drag going on, before it is applied (see
    /// `drag_move`).
    pub(super) drag_before: StoredValue<Option<DragDelete>>,
    /// The document's version when a drag of its text began
    /// (`dragstart`), until the drag ends (see `drag_move`).
    pub(super) drag_source: StoredValue<Option<u64>>,
    /// An input method is composing text (`compositionstart` came, its
    /// `compositionend` not yet): its input is applied once it ends.
    pub(super) composing: StoredValue<bool>,
}

impl TypingState {
    pub(super) fn new() -> Self {
        Self {
            closers: StoredValue::new(Vec::new()),
            copied_line: StoredValue::new(None),
            pre_edit_selection: StoredValue::new(UNKNOWN_SELECTION),
            input_burst: StoredValue::new(None),
            chord: StoredValue::new(false),
            drag_before: StoredValue::new(None),
            drag_source: StoredValue::new(None),
            composing: StoredValue::new(false),
        }
    }
}

impl EditorState {
    /// The inserted closer starting at UTF-16 `offset`, if any.
    pub(super) fn closer_at(&self, offset: usize) -> Option<Closer> {
        self.typing().closers.with_value(|closers| {
            closers
                .iter()
                .find(|closer| closer.start == offset)
                .copied()
        })
    }

    /// Forgets the inserted closer starting at UTF-16 `offset`.
    pub(super) fn forget_closer_at(&self, offset: usize) {
        self.typing()
            .closers
            .update_value(|closers| closers.retain(|closer| closer.start != offset));
    }

    /// Remembers a closer the edit just made wrote at UTF-16 `start` of the
    /// document.
    pub(super) fn remember_closer(&self, start: usize, len: usize, opener: Option<char>) {
        self.remember(Closer {
            start,
            len,
            opener,
            completion: false,
        });
    }

    /// Remembers `closer`, which the edit just made wrote.
    pub(super) fn remember(&self, closer: Closer) {
        self.typing()
            .closers
            .update_value(|closers| closers.push(closer));
    }

    /// Forgets the typing aids tied to the caret's place (inserted closers,
    /// placeholder slots) and ends the undo group.
    pub(super) fn forget_typing_aids(&self) {
        self.typing().closers.update_value(Vec::clear);
        self.completion().slot_region.set_value(None);
        self.completion().value_stops.update_value(Vec::clear);
        self.doc().history.update(EditHistory::seal);
    }

    /// Maps the typing aids kept by position through `change` (an edit's
    /// `Edit::mapping`, made of `before`), as the edit pipeline writes
    /// it: the inserted closers, and the default, the block
    /// and the values an accepted suggestion left for Tab. One whose own
    /// text the edit changed goes. A text in another language (`switching`,
    /// an Edit-as switch) keeps none of them.
    pub(super) fn follow_typing_aids(&self, change: &Change, before: &str, switching: bool) {
        let (typing, completion) = (self.typing(), self.completion());
        let kept = typing.closers.with_value(|closers| !closers.is_empty())
            || completion.placeholder.with_value(Option::is_some)
            || completion.slot_region.with_value(Option::is_some)
            || completion.value_stops.with_value(|stops| !stops.is_empty());
        if !kept {
            return;
        }
        let change = (!switching).then(|| change.in_utf16(before)).flatten();
        let Some(change) = change else {
            typing.closers.update_value(Vec::clear);
            completion.placeholder.set_value(None);
            completion.slot_region.set_value(None);
            completion.value_stops.update_value(Vec::clear);
            return;
        };
        let span = |(start, end): (usize, usize)| {
            change
                .map_range(&(start..end))
                .map(|range| (range.start, range.end))
        };
        typing.closers.update_value(|closers| {
            closers.retain_mut(
                |closer| match span((closer.start, closer.start + closer.len)) {
                    Some((start, _)) => {
                        closer.start = start;
                        true
                    }
                    None => false,
                },
            );
        });
        completion.placeholder.update_value(|placeholder| {
            // (anchor, caret), the caret at either end.
            *placeholder = placeholder.and_then(|(anchor, caret)| {
                let (start, end) = span((anchor.min(caret), anchor.max(caret)))?;
                Some(if caret < anchor {
                    (end, start)
                } else {
                    (start, end)
                })
            });
        });
        completion.slot_region.update_value(|region| {
            // Text typed at either end of the block is in it.
            *region = region.map(|(start, end)| {
                (
                    change.map_pos(start, Assoc::Before).pos,
                    change.map_pos(end, Assoc::After).pos,
                )
            });
        });
        completion
            .value_stops
            .update_value(|stops| *stops = stops.iter().copied().filter_map(span).collect());
    }
}
