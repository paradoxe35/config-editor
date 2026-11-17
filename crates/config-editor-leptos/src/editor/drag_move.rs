//! A drag inside the text is one move: the browser sends its delete
//! (`deleteByDrag`) and its drop (`insertFromDrop`) as two edits, and either
//! may be refused (it reaches into a closed fold, or changes a locked
//! field). A refused half refuses the other: a refused delete leaves the
//! drop out (it would copy the text, a fold's header twice), and a refused
//! drop puts the deleted text back (the move would lose it).
//!
//! The text a drag carries is the text it began in: while it goes on, a
//! switch waiting for the document to convert waits for it too (it would
//! rewrite the text under the pointer), and a drag whose document was
//! rewritten anyway (a host value) is refused whole: its drop would copy
//! text of the document before into the new one. Which document it is, is
//! its version (`DocState::version`), never a text compared: an edit and
//! its undo give the same text back, as another document.

use leptos::prelude::*;

use super::diagnostics::DIAGNOSTICS_DEBOUNCE_MS;
use super::edit::{Change, Edit};
use super::state::EditorState;
use super::text::{TextRange, TextSize, slice};
use crate::reactive::later;

/// A drag's delete about to be applied: the version of the document it is
/// applied to, and the text it deletes, at byte `at` of that document.
pub(super) struct DragDelete {
    version: u64,
    at: TextSize,
    deleted: String,
}

impl EditorState {
    /// A drag of the editor's own text begins.
    pub(super) fn drag_began(&self) {
        let version = self.doc().version.get_value();
        self.typing().drag_source.set_value(Some(version));
    }

    /// The drag ended (dropped, or cancelled): a switch that waited for it
    /// is tried after the pause a drop's edit also waits for.
    pub(super) fn drag_ended(&self) {
        if self
            .typing()
            .drag_source
            .try_update_value(Option::take)
            .flatten()
            .is_none()
        {
            return;
        }
        later::after(*self, DIAGNOSTICS_DEBOUNCE_MS, |state| {
            if !state.dragging() {
                super::sync::retry_pending_switch(state);
            }
        });
    }

    /// Whether a drag of the editor's own text is going on.
    pub(super) fn dragging(&self) -> bool {
        self.typing().drag_source.with_value(Option::is_some)
    }

    /// Whether the document was rewritten since the drag going on began:
    /// the text it carries is not this document's.
    pub(super) fn rewritten_under_drag(&self) -> bool {
        let version = self.doc().version.get_value();
        self.typing()
            .drag_source
            .with_value(|source| source.is_some_and(|source| source != version))
    }

    /// A drag's delete of bytes `start..end` is about to be applied to the
    /// document as it is now.
    pub(super) fn drag_starts(&self, start: TextSize, end: TextSize) {
        let deleted = self.doc().text.with_untracked(|text| {
            slice::slice(text, TextRange::new(start, end.max(start)))
                .unwrap_or_default()
                .to_string()
        });
        let delete = DragDelete {
            version: self.doc().version.get_value(),
            at: start,
            deleted,
        };
        self.typing().drag_before.set_value(Some(delete));
        // The drop comes in the same task, or this drag left the editor.
        later::next_frame(*self, |state| state.typing().drag_before.set_value(None));
    }

    /// Whether the drop arriving now has to be refused: its drag's delete
    /// was (the document is still the one it was before the delete), or
    /// with no delete (a copy), the document was rewritten under the drag.
    /// Input still pending is applied first.
    pub(super) fn drop_refused_early(&self) -> bool {
        super::input::flush_input(*self);
        let version = self.doc().version.get_value();
        self.typing()
            .drag_before
            .with_value(|delete| delete.as_ref().map(|delete| delete.version == version))
            .unwrap_or_else(|| self.rewritten_under_drag())
    }

    /// The drop of a drag whose delete was applied is refused: the deleted
    /// text comes back, the delete's undo step with it.
    pub(super) fn drop_refused(&self) {
        let Some(delete) = self
            .typing()
            .drag_before
            .try_update_value(Option::take)
            .flatten()
        else {
            return;
        };
        let version = self.doc().version.get_value();
        if version == delete.version {
            return;
        }
        // The delete's own step, when it is the newest and nothing was
        // written since: undone, as if it was never made.
        let deleted = self
            .doc()
            .history
            .with_untracked(|history| history.newest_made_between())
            == Some((delete.version, version));
        if deleted && let Some(edit) = self.history_edit(false) {
            if self.commit(edit) {
                self.doc().history.update(|history| history.forget_redo());
            }
            return;
        }
        // Else the deleted text is written back where it was.
        let current = self.doc().text.get_untracked();
        let change = Change::replace(&current, TextRange::empty(delete.at), &delete.deleted);
        self.commit(Edit::command(current, change, (delete.at, delete.at)));
    }

    /// The drop went through: the move is done.
    pub(super) fn drop_done(&self) {
        self.typing().drag_before.set_value(None);
    }
}
