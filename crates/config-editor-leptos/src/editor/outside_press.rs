//! A press outside the editor (a tap on the page, a click beside it)
//! closes the hover card and the problem tip a tap or F8 opened, as a
//! click in the text does: on a phone the next tap could land on its link
//! otherwise.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::state::EditorState;
use crate::reactive::window_listener;

/// Closes the editor's popups on each press outside it.
pub(super) fn close_popups_on_outside_press(state: EditorState) {
    window_listener(&["pointerdown"], move |event| {
        let open = state.hover().card.with_untracked(Option::is_some)
            || state.problems().current.with_untracked(Option::is_some)
            || state.problems().hovered.with_untracked(Option::is_some);
        if open && !inside_editor(state, &event) {
            state.hide_hover();
            state.hide_shown_problem();
        }
    });
}

/// Whether `event` happened inside the editor (its popups are inside it).
fn inside_editor(state: EditorState, event: &web_sys::Event) -> bool {
    let Some(root) = state.view().root_ref.get_untracked() else {
        return true;
    };
    let target = event
        .composed_path()
        .get(0)
        .dyn_into::<web_sys::Node>()
        .ok();
    target.is_some_and(|target| root.contains(Some(&target)))
}
