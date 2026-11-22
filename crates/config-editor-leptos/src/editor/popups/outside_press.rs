//! A press outside the editor (a tap on the page, a click beside it)
//! closes the hover card and the problem tip a tap or F8 opened, as a
//! click in the text does: on a phone the next tap could land on its link
//! otherwise. Escape with the pointer on one of them or on the text it
//! shows for, and the focus nowhere (the page's body), closes it too, as
//! Escape in the text does.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement};

use super::super::state::EditorState;
use crate::reactive::window_listener;

/// Closes the editor's popups on each press outside it.
pub(crate) fn close_popups_on_outside_press(state: EditorState) {
    window_listener(&["pointerdown"], move |event| {
        let open =
            state.popups().is_open() || state.problems().current.with_untracked(Option::is_some);
        let open = open || state.uses().is_open();
        if open && !inside_editor(state, &event) {
            state.hide_hover();
            state.hide_shown_problem();
            state.close_uses();
        }
    });
}

/// Closes the hover card or the problem tip on Escape with the pointer on
/// it or on the text it shows for, while nothing has the focus or the
/// focus is on a control of the view around the editor that has no
/// Escape of its own (a documents view's tabs and "+") (the editor's own
/// keys close them when it has it; another field on the page keeps its
/// Escape).
pub(crate) fn close_popups_on_escape_over_them(state: EditorState) {
    window_listener(&["keydown"], move |event| {
        let escape = event
            .dyn_ref::<web_sys::KeyboardEvent>()
            .is_some_and(|event| event.key() == "Escape");
        if escape && state.pointer_over_popups() && escape_is_free(state) {
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

/// Whether Escape is free to close the popup under the pointer: the focus
/// is nowhere, or on a control of the view the editor is in (a documents
/// view's tab or "+" button, outside the editor itself), which has no
/// Escape of its own. A field typed into keeps it.
fn escape_is_free(state: EditorState) -> bool {
    nothing_focused() || on_own_control(state)
}

/// Whether the focus is on a control of the view around the editor (the
/// closest element of the view's class holding it), not in the editor.
fn on_own_control(state: EditorState) -> bool {
    let Some(active) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element())
    else {
        return false;
    };
    let Some(root) = state.view().root_ref.get_untracked() else {
        return false;
    };
    let root: &Element = root.unchecked_ref();
    let view = root
        .parent_element()
        .and_then(|parent| parent.closest(".config-editor-documents").ok().flatten());
    let typed_into = matches!(active.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")
        || active
            .dyn_ref::<HtmlElement>()
            .is_some_and(HtmlElement::is_content_editable);
    view.is_some_and(|view| view.contains(Some(&active)))
        && !root.contains(Some(&active))
        && !typed_into
}

/// Whether the focus is nowhere (the page's body): Escape then closes the
/// popup under the pointer.
fn nothing_focused() -> bool {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return false;
    };
    match document.active_element() {
        None => true,
        Some(active) => {
            document.body().is_some_and(|body| {
                let body: &Element = body.unchecked_ref();
                body == &active
            }) || document.document_element().as_ref() == Some(&active)
        }
    }
}
