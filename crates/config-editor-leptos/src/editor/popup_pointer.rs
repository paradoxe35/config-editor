//! The pointer on a popup it can enter (the hover card, the problem tip
//! with a link or that scrolls), as in VS Code. The popup takes a pointer
//! that comes from what it is for (see `hover_path`), so its text can be
//! selected and copied and its links followed; one that comes onto it
//! another way (from beside it, on the lines it covers) goes through to
//! the text under it, where a click places the caret, until where it goes
//! on to says whether it heads for the popup or for that text (see
//! `popup_under`). A wheel over the popup scrolls it while it can
//! scroll that way, else the text (the page at the text's ends), and the
//! popup closes, as when the wheel scrolls the text.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, MouseEvent, WheelEvent};

use super::scroll_through::{scroll_past, wheel_pixels};
use super::state::EditorState;

/// The attribute of a popup letting the pointer through (see the
/// stylesheet).
const PASSING: &str = "data-passing";

impl EditorState {
    /// The pointer is on the problems the tip shows (no card lists them;
    /// not leaving them): on the way to the tip.
    pub(super) fn note_on_problems(&self) {
        if self.hover().card.with_untracked(Option::is_none)
            && self.problems().hovered.with_untracked(Option::is_some)
            && !self.problems().tip_leaving.get_value()
        {
            self.hover().on_way.set_value(true);
        }
    }

    /// The pointer is on what the popup is for, or in its gap: the popup
    /// takes it again.
    pub(super) fn popup_takes_pointer(&self) {
        if let Some(popup) = self.reachable_popup_element() {
            take_pointer_again(&popup);
        }
    }

    /// Whether the popup was shown at the caret (`Ctrl+K Ctrl+I`, F8, a
    /// tap), not by the pointer: it takes the pointer from anywhere.
    fn popup_at_caret(&self) -> bool {
        if self.hover().card.with_untracked(Option::is_some) {
            return self.hover().at_caret.get_value();
        }
        self.problems().hovered.with_untracked(Option::is_none)
            && self.problems().current.with_untracked(Option::is_some)
    }
}

/// Whether `popup` lets the pointer through to the text under it.
pub(super) fn lets_through(popup: &HtmlElement) -> bool {
    popup.has_attribute(PASSING)
}

/// `popup` takes the pointer again (it shows something else, or the
/// pointer is back on what it is for).
pub(super) fn take_pointer_again(popup: &Element) {
    let _ = popup.remove_attribute(PASSING);
}

/// The pointer came onto `popup`: whether it enters it (on its way from
/// what the popup is for, or to a popup shown at the caret). Otherwise
/// the popup lets it through, and stays while where it goes on to decides
/// (see `popup_under`).
pub(super) fn enter_popup(state: EditorState, popup: &HtmlElement, event: &MouseEvent) -> bool {
    let (x, y) = (f64::from(event.client_x()), f64::from(event.client_y()));
    if state.popup_at_caret() || (state.entered_on_the_way() && !state.comes_from_beside(popup)) {
        return true;
    }
    let _ = popup.set_attribute(PASSING, "");
    state.leave_under();
    // Through the popup, the pointer is over the text.
    state.hover().on_text.set_value(true);
    if let Some(element) = state.textarea() {
        super::input::pointer_at(state, &element, x, y, event.buttons() == 0);
    }
    false
}

/// The pointer left a popup it had entered for the text (`event`, its
/// `mouseleave`): whether it went there. Then what is under it answers at
/// once, as if it had moved there (the page sends the text no move when
/// it comes back to the pixel it last had there): back on what the popup
/// is for, the popup stays, without closing to open again.
pub(super) fn back_on_text(state: EditorState, event: &MouseEvent) -> bool {
    let Some(element) = state.textarea() else {
        return false;
    };
    let onto_text = event.related_target().is_some_and(|to| {
        let text: &web_sys::EventTarget = element.element().unchecked_ref();
        &to == text
    });
    if onto_text {
        let (x, y) = (f64::from(event.client_x()), f64::from(event.client_y()));
        super::input::pointer_at(state, &element, x, y, event.buttons() == 0);
    }
    onto_text
}

/// A wheel over `popup`: it scrolls the popup while it can scroll that
/// way; else the text, or the page at the text's ends, and the popups
/// close.
pub(super) fn popup_wheel(state: EditorState, popup: &HtmlElement, event: &WheelEvent) {
    let (dx, dy) = wheel_pixels(state, event);
    if scrolls_itself(popup, dy) {
        return;
    }
    event.prevent_default();
    scroll_past(state, dx, dy);
    state.hide_hover();
    state.hide_problem_tip();
}

/// Whether `popup` scrolls by `dy` pixels (down when positive) itself.
fn scrolls_itself(popup: &HtmlElement, dy: f64) -> bool {
    let top = popup.scroll_top();
    let most = popup.scroll_height() - popup.client_height();
    (dy > 0.0 && top < most) || (dy < 0.0 && top > 0)
}

/// Whether Escape is free to close the popup under the pointer: the focus
/// is nowhere, or on a control of the view the editor is in (a documents
/// view's tab or "+" button, outside the editor itself), which has no
/// Escape of its own. A field typed into keeps it.
pub(super) fn escape_is_free(state: EditorState) -> bool {
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
pub(super) fn nothing_focused() -> bool {
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
