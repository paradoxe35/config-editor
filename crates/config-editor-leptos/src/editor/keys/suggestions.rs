//! Keys for the suggestion list: Ctrl+Space, moving through it, accepting.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::completion::{ListMode, apply_completion};
use super::super::state::{EditorState, select};
use crate::editor::text::slice;

/// Ctrl+Space opens the list, schema examples included.
pub(super) fn request(state: EditorState, event: &KeyboardEvent) -> bool {
    if !(event.ctrl_key() && event.code() == "Space") {
        return false;
    }
    event.prevent_default();
    // With a selection, the list opens at its end, the selection let go
    // (otherwise a selection closes the list, and Enter would replace it).
    let selected = state.textarea().and_then(|element| {
        let (start, end) = element.selection();
        (start != end).then_some((element, end))
    });
    let replace_word = selected.is_some() || state.list_mode().replaces_word();
    if let Some((element, end)) = selected {
        let _ = element.set_selection_range(end, end);
    }
    state.completion().active.set(0);
    // The selected word is to be replaced: every key is offered.
    state.set_list_mode(ListMode::Asked {
        replace_word,
        heeded: true,
    });
    // The user asked: nothing to offer is said, with the reason shown where
    // the text is protected (a read-only value, a secret placeholder). That
    // refusal ends the request: the caret moving on to other lines opens
    // no list there.
    if state.completion().suggestions.with_untracked(Vec::is_empty) {
        if super::super::a11y::show_protected_at_caret(state) {
            state.close_suggestions();
        } else {
            super::super::a11y::say_no_suggestions(state);
        }
    }
    true
}

/// Accepting what is already typed would change nothing, so Enter and Tab
/// close the list and edit as usual, after stepping past the value
/// (`"balance|"` → `"balance"|`) so Enter starts a new line instead of
/// splitting it. When the line goes on after the value (`nodelay| = true`),
/// Enter only closes the list.
///
/// Returns whether the list is still open for the key, or `None` when the
/// key was handled here.
pub(super) fn step_past_unchanged(state: EditorState, event: &KeyboardEvent) -> Option<bool> {
    if state.completion().suggestions.with_untracked(Vec::is_empty) {
        return Some(false);
    }
    if super::modified(event) {
        return Some(true);
    }
    let key = event.key();
    if !matches!(key.as_str(), "Enter" | "Tab") {
        return Some(true);
    }
    // The one option, not the whole list (a long one is cloned per key).
    let active = state.active_suggestion();
    let active = active.as_ref();
    let unchanged_caret = active.and_then(|option| {
        state.doc().text.with_untracked(|text| {
            let (updated, _, caret) = apply_completion(text, option);
            (updated == *text).then_some(caret)
        })
    });
    let Some(caret) = unchanged_caret else {
        return Some(true);
    };
    if let Some(option) = active {
        state.say_why_read_only(option);
    }
    if let Some(element) = state.textarea() {
        select(&element, caret, caret, false);
    }
    // After an explicit Ctrl+Space, Enter on what is already written only
    // closes the list.
    let asked = state.list_mode().is_explicit();
    state.close_suggestions();
    // Text after the caret on its line (`nodelay| = true`) would be split
    // off by a new line: Enter only closes the list there. A closer the
    // editor inserted (`</mode>`) is stepped out of by Enter instead.
    let line_continues = state.closer_at(caret).is_none()
        && state.doc().text.with_untracked(|text| {
            slice::after(text, caret)
                .and_then(|rest| rest.lines().next())
                .is_some_and(|rest| !rest.trim().is_empty())
        });
    if key == "Tab" || line_continues || asked {
        event.prevent_default();
        return None;
    }
    Some(false)
}

/// Page and arrow keys move through an open list; Enter and Tab accept.
pub(super) fn list_keys(state: EditorState, event: &KeyboardEvent) -> bool {
    let count = state.completion().suggestions.with_untracked(Vec::len);
    // With a modifier the keys keep their editing meaning (Shift+arrows
    // select, Ctrl+Enter opens a line below).
    if count == 0 || super::modified(event) {
        return false;
    }
    let current = state.completion().active.get_untracked();
    let page = state.completion().row_size.get_untracked().page_rows();
    let next = match event.key().as_str() {
        "PageDown" => (current + page).min(count - 1),
        "PageUp" => current.saturating_sub(page),
        "ArrowDown" => (current + 1) % count,
        "ArrowUp" => current.checked_sub(1).unwrap_or(count - 1),
        "Enter" => {
            event.prevent_default();
            state.accept_active();
            return true;
        }
        "Tab" => {
            event.prevent_default();
            state.accept_active();
            return true;
        }
        _ => return false,
    };
    event.prevent_default();
    state.completion().active.set(next);
    state.set_list_mode(state.list_mode().heeding(true));
    true
}
