//! A press outside the editor (a tap on the page, a click beside it)
//! closes the hover card and the problem tip a tap or F8 opened, as a
//! click in the text does: on a phone the next tap could land on its link
//! otherwise. Escape with the pointer on one of them or on the text it
//! shows for, and the focus nowhere (the page's body), closes it too, as
//! Escape in the text does.

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

/// Closes the hover card or the problem tip on Escape with the pointer on
/// it or on the text it shows for, while nothing has the focus or the
/// focus is on a control of the view around the editor that has no
/// Escape of its own (a documents view's tabs and "+", QA 28) (the
/// editor's own keys close them when it has it; another field on the page
/// keeps its Escape).
pub(super) fn close_popups_on_escape_over_them(state: EditorState) {
    window_listener(&["keydown"], move |event| {
        let escape = event
            .dyn_ref::<web_sys::KeyboardEvent>()
            .is_some_and(|event| event.key() == "Escape");
        // On the popup, or on the text (the key or underline it shows for,
        // or on the way to it).
        let over = state.hover().card_entered.get_value()
            || state.problems().tip_entered.get_value()
            || state.hover().on_text.get_value();
        if escape && over && super::popup_pointer::escape_is_free(state) {
            state.hide_hover();
            state.hide_problem_tip();
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
