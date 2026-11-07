//! Keys for the suggestion list: Ctrl+Space, moving through it, accepting.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::completion::{
    COMPLETION_PAGE_ROWS, apply_completion, scroll_completion_into_view,
};
use super::super::keyboard::utf16_to_byte_index;
use super::super::state::EditorState;

/// Ctrl+Space opens the list, schema examples included.
pub(super) fn request(state: EditorState, event: &KeyboardEvent) -> bool {
    if !(event.ctrl_key() && event.code() == "Space") {
        return false;
    }
    event.prevent_default();
    // With a selection, the list opens at its end, the selection let go
    // (otherwise a selection closes the list, and Enter would replace it).
    let selected = state.textarea().and_then(|element| {
        match (element.selection_start(), element.selection_end()) {
            (Ok(Some(start)), Ok(Some(end))) if start != end => Some((element, end)),
            _ => None,
        }
    });
    if let Some((element, end)) = selected {
        let _ = element.set_selection_range(end, end);
        state.note_caret(end as usize);
        // The selected word is to be replaced: every key is offered.
        state.completion.replace_word.set(true);
    }
    state.completion.active.set(0);
    state.completion.explicit.set(true);
    state.completion.requested.set(true);
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
    let options = state.completion.suggestions.get_untracked();
    if options.is_empty() {
        return Some(false);
    }
    if super::modified(event) {
        return Some(true);
    }
    let key = event.key();
    if !matches!(key.as_str(), "Enter" | "Tab") {
        return Some(true);
    }
    let unchanged_caret = options
        .get(state.completion.active.get_untracked())
        .and_then(|option| {
            state.doc.text.with_untracked(|text| {
                let (updated, _, caret) = apply_completion(text, option);
                (updated == *text).then_some(caret)
            })
        });
    let Some(caret) = unchanged_caret else {
        return Some(true);
    };
    if let Some(element) = state.textarea() {
        let _ = element.set_selection_range(caret as u32, caret as u32);
        state.doc.caret.set(caret);
    }
    state.close_suggestions();
    // Text after the caret on its line (`nodelay| = true`) would be split
    // off by a new line: Enter only closes the list there. A closer the
    // editor inserted (`</mode>`) is stepped out of by Enter instead.
    let line_continues = state.closer_at(caret).is_none()
        && state.doc.text.with_untracked(|text| {
            let byte = utf16_to_byte_index(text, caret);
            text[byte..]
                .lines()
                .next()
                .is_some_and(|rest| !rest.trim().is_empty())
        });
    // After an explicit Ctrl+Space, Enter on what is already written only
    // closes the list.
    let asked = state.completion.explicit.get_untracked();
    if key == "Tab" || line_continues || asked {
        event.prevent_default();
        return None;
    }
    Some(false)
}

/// Page and arrow keys move through an open list; Enter and Tab accept.
pub(super) fn list_keys(state: EditorState, event: &KeyboardEvent) -> bool {
    let count = state.completion.suggestions.with_untracked(Vec::len);
    // With a modifier the keys keep their editing meaning (Shift+arrows
    // select, Ctrl+Enter opens a line below).
    if count == 0 || super::modified(event) {
        return false;
    }
    let current = state.completion.active.get_untracked();
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
        "Tab" => {
            event.prevent_default();
            state.accept_active();
            return true;
        }
        _ => return false,
    };
    event.prevent_default();
    state.completion.active.set(next);
    if let Some(list) = state.view.completion_ref.get_untracked() {
        scroll_completion_into_view(&list, next);
    }
    true
}
