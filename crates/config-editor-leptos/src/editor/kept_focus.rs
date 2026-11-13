//! The focus stays in the editor when the control that has it goes: the
//! footer's change count and problem status when the text no longer has
//! any, the Edit-as notice's Dismiss, the change preview's Revert and Close
//! when a host push edits the hunk away, a fold's chip or chevron that a
//! host update moves. The text area takes it (see `focus::keep_focus`).

use leptos::prelude::*;

use super::state::EditorState;
use crate::focus::keep_focus;

/// Keeps the focus in the editor, on its text, while its root lives.
pub(super) fn keep_focus_in_editor(state: EditorState) {
    let root = state.view().root_ref;
    // A new root (or none) ends the watch on the old one.
    Effect::new(move |_| {
        if let Some(root) = root.get() {
            untrack(|| {
                keep_focus(&root, move |_| {
                    state.view().textarea_ref.get_untracked().map(Into::into)
                });
            });
        }
    });
}
