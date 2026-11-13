//! A drag inside the text is one move: the browser sends its delete
//! (`deleteByDrag`) and its drop (`insertFromDrop`) as two edits, and either
//! may be refused (it reaches into a closed fold, or changes a locked
//! field). A refused half refuses the other: a refused delete leaves the
//! drop out (it would copy the text, a fold's header twice), and a refused
//! drop puts the deleted text back (the move would lose it).

use leptos::prelude::*;

use super::edit::Edit;
use super::state::EditorState;
use crate::reactive::later;

impl EditorState {
    /// A drag's delete is about to be applied to the document as it is now.
    pub(super) fn drag_starts(&self) {
        let before = self.doc().text.get_untracked();
        self.typing().drag_before.set_value(Some(before));
        // The drop comes in the same task, or this drag left the editor.
        later::next_frame(*self, |state| state.typing().drag_before.set_value(None));
    }

    /// Whether the drop arriving now has to be refused because its drag's
    /// delete was (the document is still what it was before the delete).
    /// Input still pending is applied first.
    pub(super) fn drop_refused_by_its_delete(&self) -> bool {
        super::input::flush_input(*self);
        self.typing().drag_before.with_value(|before| {
            before
                .as_ref()
                .is_some_and(|before| self.doc().text.with_untracked(|text| text == before))
        })
    }

    /// The drop of a drag whose delete was applied is refused: the deleted
    /// text comes back, the delete's undo step with it.
    pub(super) fn drop_refused(&self) {
        let Some(before) = self
            .typing()
            .drag_before
            .try_update_value(Option::take)
            .flatten()
        else {
            return;
        };
        if self.doc().text.with_untracked(|text| *text == before) {
            return;
        }
        let deleted = self
            .doc()
            .history
            .with_untracked(|history| history.next_undo().is_some_and(|step| step.value == before));
        if deleted && let Some(edit) = self.history_edit(false) {
            if self.commit(edit) {
                self.doc().history.update(|history| history.forget_redo());
            }
            return;
        }
        let current = self.doc().text.get_untracked();
        let caret = before.encode_utf16().count();
        self.commit(Edit::new(
            current.clone(),
            (0, current.encode_utf16().count()),
            before,
            caret,
        ));
    }

    /// The drop went through: the move is done.
    pub(super) fn drop_done(&self) {
        self.typing().drag_before.set_value(None);
    }
}
