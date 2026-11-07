//! Find, replace and go-to-line keys anywhere in the editor (the text or a
//! widget): `Ctrl+F`, `Ctrl+H`, `F3`, `Shift+F3` and `Ctrl+G`.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::state::EditorState;
use super::{actions as search, go_to_line};

/// The shortcuts of the find widget and the go-to-line field: `true` when
/// the key was taken.
pub(in crate::editor) fn shortcuts(state: EditorState, event: &KeyboardEvent) -> bool {
    find_keys(state, event) || go_to_line_key(state, event)
}

fn find_keys(state: EditorState, event: &KeyboardEvent) -> bool {
    if !state.config.settings.search.get_untracked() {
        return false;
    }
    let command = (event.ctrl_key() || event.meta_key()) && !event.alt_key() && !event.shift_key();
    let key = event.key().to_ascii_lowercase();
    if command && key == "f" {
        event.prevent_default();
        search::open(state, false);
        return true;
    }
    if command && key == "h" {
        event.prevent_default();
        search::open(state, true);
        return true;
    }
    if event.key() == "F3" && !event.ctrl_key() && !event.meta_key() && !event.alt_key() {
        event.prevent_default();
        if !state.search.open.get_untracked() {
            search::open(state, false);
        }
        search::step(state, event.shift_key());
        return true;
    }
    false
}

/// `Ctrl+G` opens the go-to-line field.
fn go_to_line_key(state: EditorState, event: &KeyboardEvent) -> bool {
    let command = (event.ctrl_key() || event.meta_key()) && !event.alt_key() && !event.shift_key();
    if !state.config.settings.go_to_line.get_untracked()
        || !command
        || !event.key().eq_ignore_ascii_case("g")
    {
        return false;
    }
    event.prevent_default();
    go_to_line::open(state);
    true
}
