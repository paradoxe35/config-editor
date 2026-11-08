//! Folding keys, as in VS Code (`Cmd` for `Ctrl` on macOS):
//! `Ctrl+Shift+[` folds at the caret, `Ctrl+Shift+]` unfolds,
//! `Ctrl+K Ctrl+0` folds everything and `Ctrl+K Ctrl+J` unfolds everything
//! (the second key of a `Ctrl+K` chord, see `keys::chord`).
//! Also: an editing key whose edit reaches into a fold opens it first.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::keys::{typed_character, unmodified};
use super::super::state::{EditorState, textarea_selection};

/// The folding shortcuts. Returns whether the key was one.
pub(in crate::editor) fn shortcuts(state: EditorState, event: &KeyboardEvent) -> bool {
    if !state.config().settings.folding.get_untracked() {
        return false;
    }
    let command = event.ctrl_key() || event.meta_key();
    if !command || event.alt_key() || !event.shift_key() {
        return false;
    }
    match event.code().as_str() {
        "BracketLeft" => {
            event.prevent_default();
            state.fold_at_caret();
            true
        }
        "BracketRight" => {
            event.prevent_default();
            state.unfold_at_caret();
            true
        }
        _ => false,
    }
}

/// The second key of a `Ctrl+K` chord: `Ctrl+0` folds everything,
/// `Ctrl+J` unfolds everything. Returns whether the key was one.
pub(in crate::editor) fn chord_key(state: EditorState, event: &KeyboardEvent) -> bool {
    if !state.config().settings.folding.get_untracked() || !(event.ctrl_key() || event.meta_key()) {
        return false;
    }
    let key = event.key().to_ascii_lowercase();
    if key == "0" || event.code() == "Digit0" {
        event.prevent_default();
        state.fold_all();
        return true;
    }
    if key == "j" {
        event.prevent_default();
        state.unfold_all();
        return true;
    }
    false
}

/// Opens the folds the edit an editing key is about to make would touch:
/// a character typed at a folded line's end, Backspace at the start of the
/// line after a fold, Delete at a folded line's end, any edit of a
/// selection reaching into one. The edit then applies to the whole text.
pub(in crate::editor) fn unfold_before_edit(state: EditorState, event: &KeyboardEvent) {
    if state.fold().folded.with_untracked(Vec::is_empty) {
        return;
    }
    let Some(element) = state.textarea() else {
        return;
    };
    let (start, end) = textarea_selection(&element);
    let collapsed = usize::from(start == end);
    let reach = match event.key().as_str() {
        "Backspace" => (start.saturating_sub(collapsed), end),
        "Delete" => (start, end + collapsed),
        "Enter" | "Tab" => (start, end),
        _ if typed_character(event).is_some() && unmodified(event) => (start, end),
        _ => return,
    };
    state.open_folds_for_edit(reach.0, reach.1);
}
