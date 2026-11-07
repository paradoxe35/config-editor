//! Find and replace and go to line, compiled out (the `search` feature is
//! off): the hooks of `search/` with nothing behind them. `Ctrl+F`, `Ctrl+H`,
//! `F3` and `Ctrl+G` stay the browser's, and the `search` and `go_to_line`
//! settings do nothing.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::state::EditorState;

pub(super) fn shortcuts(_state: EditorState, _event: &KeyboardEvent) -> bool {
    false
}

pub(super) fn follow_settings(_state: EditorState) {}

pub(super) fn close_on_escape(_state: EditorState) -> bool {
    false
}

pub(super) fn is_open(_state: EditorState) -> bool {
    false
}

#[component]
pub(super) fn FindWidget(state: EditorState) -> impl IntoView {
    let _ = state;
}

#[component]
pub(super) fn GoToLineWidget(state: EditorState) -> impl IntoView {
    let _ = state;
}

#[component]
pub(super) fn SearchHighlights(state: EditorState) -> impl IntoView {
    let _ = state;
}
