//! Keyboard handling for the textarea, one concern per module. Each handler
//! returns `true` when it took the key, and [`on_keydown`] stops there; the
//! order below is the order keys are matched in.

mod enter;
mod navigation;
mod suggestions;
mod tab;
mod typing;

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::state::EditorState;

pub(super) fn on_keydown(state: EditorState, event: &KeyboardEvent) {
    // Keys act on the document as the browser last left it.
    super::input::flush_input(state);
    state.hide_hover();
    if !matches!(
        event.key().as_str(),
        "F8" | "Shift" | "Control" | "Alt" | "Meta"
    ) {
        state.hide_shown_problem();
    }
    if navigation::smart_home(state, event) || navigation::keep_placeholder(state, event) {
        return;
    }
    if state.read_only.get_untracked() {
        // Navigation works as usual: Escape collapses a selection (the root
        // listener closes popups); suggestions never open.
        if event.key() == "Escape" {
            event.prevent_default();
            navigation::escape(state, event);
        } else if event.ctrl_key() && event.code() == "Space" {
            event.prevent_default();
        }
        return;
    }
    let handled = navigation::undo_redo(state, event)
        || suggestions::request(state, event)
        || navigation::next_problem(state, event)
        || typing::toggle_comments(state, event)
        || typing::join_with_next_line(state, event)
        || (state.features.auto_closing && typing::pairs_and_tags(state, event))
        || typing::indentation(state, event);
    if handled {
        return;
    }
    let Some(options_open) = suggestions::step_past_unchanged(state, event) else {
        return;
    };
    if options_open && suggestions::list_keys(state, event) {
        return;
    }
    match event.key().as_str() {
        "Escape" => navigation::escape(state, event),
        "Tab" => tab::tab(state, event),
        "Enter" => enter::enter(state, event),
        _ => {}
    }
}

/// Keys on the editor root: Escape closes every popup.
pub(super) fn on_root_keydown(state: EditorState, event: &KeyboardEvent) {
    if event.key() == "Escape" {
        event.prevent_default();
        state.close_suggestions();
        state.explicit_request.set(false);
        state.selected_diff_peek.set(None);
        state.hide_hover();
    }
}

/// Ctrl, Cmd, Alt or Shift held.
fn modified(event: &KeyboardEvent) -> bool {
    event.ctrl_key() || event.meta_key() || event.alt_key() || event.shift_key()
}

/// No Ctrl, Cmd or Alt held.
fn unmodified(event: &KeyboardEvent) -> bool {
    !event.ctrl_key() && !event.meta_key() && !event.alt_key()
}

/// The single character a key types, if it types one.
fn typed_character(event: &KeyboardEvent) -> Option<char> {
    let key = event.key();
    let mut characters = key.chars();
    let character = characters.next()?;
    characters.next().is_none().then_some(character)
}
