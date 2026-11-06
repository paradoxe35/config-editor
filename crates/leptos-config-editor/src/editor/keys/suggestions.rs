//! Keys for the suggestion list: Ctrl+Space, moving through it, accepting.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::completion::{
    COMPLETION_PAGE_ROWS, apply_completion, scroll_completion_into_view,
};
use super::super::state::EditorState;

/// Ctrl+Space opens the list, schema examples included.
pub(super) fn request(state: EditorState, event: &KeyboardEvent) -> bool {
    if !(event.ctrl_key() && event.code() == "Space") {
        return false;
    }
    event.prevent_default();
    state.active_completion.set(0);
    state.explicit_request.set(true);
    state.suggestions_requested.set(true);
    true
}

/// Accepting what is already typed would change nothing, so Enter and Tab
/// close the list and edit as usual, after stepping past the value
/// (`"balance|"` → `"balance"|`) so Enter starts a new line instead of
/// splitting it.
///
/// Returns whether the list is still open for the key, or `None` when the
/// key was handled here (Tab).
pub(super) fn step_past_unchanged(state: EditorState, event: &KeyboardEvent) -> Option<bool> {
    let options = state.suggestions.get_untracked();
    if options.is_empty() {
        return Some(false);
    }
    let key = event.key();
    if !matches!(key.as_str(), "Enter" | "Tab") {
        return Some(true);
    }
    let unchanged_caret = options
        .get(state.active_completion.get_untracked())
        .and_then(|option| {
            state.source.with_untracked(|text| {
                let (updated, _, caret) = apply_completion(text, option);
                (updated == *text).then_some(caret)
            })
        });
    let Some(caret) = unchanged_caret else {
        return Some(true);
    };
    if let Some(element) = state.textarea() {
        let _ = element.set_selection_range(caret as u32, caret as u32);
        state.cursor.set(caret);
    }
    state.close_suggestions();
    if key == "Tab" {
        event.prevent_default();
        return None;
    }
    Some(false)
}

/// Page and arrow keys move through an open list; Enter and Tab accept.
pub(super) fn list_keys(state: EditorState, event: &KeyboardEvent) -> bool {
    let count = state.suggestions.with_untracked(Vec::len);
    if count == 0 {
        return false;
    }
    let current = state.active_completion.get_untracked();
    let next = match event.key().as_str() {
        "PageDown" => (current + COMPLETION_PAGE_ROWS).min(count - 1),
        "PageUp" => current.saturating_sub(COMPLETION_PAGE_ROWS),
        "ArrowDown" => (current + 1) % count,
        "ArrowUp" => current.checked_sub(1).unwrap_or(count - 1),
        "Enter" => {
            event.prevent_default();
            state.accept_active();
            return true;
        }
        "Tab" if !event.shift_key() => {
            event.prevent_default();
            state.accept_active();
            return true;
        }
        _ => return false,
    };
    event.prevent_default();
    state.active_completion.set(next);
    if let Some(list) = state.completion_ref.get_untracked() {
        scroll_completion_into_view(&list, next);
    }
    true
}
