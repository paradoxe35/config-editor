//! Find and replace keys anywhere in the editor (the text or the widget):
//! `Ctrl+F`, `Ctrl+H`, `F3` and `Shift+F3`.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::search;
use super::super::state::EditorState;

pub(super) fn shortcuts(state: EditorState, event: &KeyboardEvent) -> bool {
    if !state.features.search {
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
