//! The change preview in the Tab order. Its element is at the editor's
//! end (for room to show), but it comes right after the text: `Tab` from
//! the text (`Ctrl+M` mode) goes to its first button, `Shift+Tab` from
//! there back to the text, and `Tab` from its last button on to what
//! follows the text; the other way round, `Shift+Tab` from what follows
//! the text goes back to its last button, and `Tab` from the last control
//! before its element goes past it.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, KeyboardEvent, Node};

use super::super::super::state::EditorState;

/// `Tab` in the preview: from its last button on to what follows the
/// text, `Shift+Tab` from its first back to the text; between its buttons
/// the browser's own.
pub(super) fn tab_past_edge(state: EditorState, event: &KeyboardEvent) {
    let Some(preview) = event
        .current_target()
        .and_then(|target| target.dyn_into::<Element>().ok())
    else {
        return;
    };
    let Ok(buttons) = preview.query_selector_all("button") else {
        return;
    };
    let edge = if event.shift_key() {
        0
    } else {
        buttons.length().saturating_sub(1)
    };
    let target = event
        .target()
        .and_then(|target| target.dyn_into::<Node>().ok());
    let on_edge = buttons
        .item(edge)
        .is_some_and(|button| button.is_same_node(target.as_ref()));
    let Some(text) = state
        .view()
        .textarea_ref
        .get_untracked()
        .filter(|_| on_edge)
    else {
        return;
    };
    let to = if event.shift_key() {
        Some(text.unchecked_into::<HtmlElement>())
    } else {
        next_tabbable(&text, &preview)
    };
    if let Some(to) = to {
        event.prevent_default();
        let _ = crate::focus::in_view(&to);
    }
}

/// `Tab` or `Shift+Tab` on a control outside the open preview that has it
/// as its neighbour in the page's order but not in the order above: back
/// from what follows the text, to its last button; on from the control
/// before its element, past it. Returns whether it moved the focus.
pub(in crate::editor) fn tab_around_preview(state: EditorState, event: &KeyboardEvent) -> bool {
    if event.key() != "Tab" || event.ctrl_key() || event.alt_key() || event.meta_key() {
        return false;
    }
    let Some(preview) = state.view().root_ref.get_untracked().and_then(|root| {
        root.query_selector(".config-editor__diff-peek")
            .ok()
            .flatten()
    }) else {
        return false;
    };
    let target = event
        .target()
        .and_then(|target| target.dyn_into::<Element>().ok());
    let text = state.view().textarea_ref.get_untracked();
    let (Some(target), Some(text)) = (target, text) else {
        return false;
    };
    if preview.contains(Some(&target)) || text.is_same_node(Some(&target)) {
        return false;
    }
    let to = if event.shift_key() {
        let after_text = next_tabbable(&text, &preview);
        after_text
            .filter(|after| after.is_same_node(Some(&target)))
            .and_then(|_| last_button(&preview))
    } else {
        // The page's next stop is the preview: past it instead.
        next_tabbable(&target, &target)
            .filter(|next| preview.contains(Some(next)))
            .and_then(|_| next_tabbable(&preview, &preview))
    };
    match to {
        Some(to) if crate::focus::in_place(&to).is_ok() => {
            event.prevent_default();
            true
        }
        _ => false,
    }
}

/// The preview's last button.
fn last_button(preview: &Element) -> Option<HtmlElement> {
    let buttons = preview.query_selector_all("button").ok()?;
    buttons
        .item(buttons.length().checked_sub(1)?)
        .and_then(|button| button.dyn_into().ok())
}

/// `Tab` out of the text (`Ctrl+M` mode) with the preview open: to its
/// first button. Returns whether it went there.
pub(in crate::editor) fn tab_into_preview(state: EditorState) -> bool {
    let button = state.view().root_ref.get_untracked().and_then(|root| {
        root.query_selector(".config-editor__diff-peek button")
            .ok()
            .flatten()
    });
    button
        .and_then(|button| button.dyn_into::<HtmlElement>().ok())
        .is_some_and(|button| crate::focus::in_view(&button).is_ok())
}

/// What `Tab` can stop at (positive `tabindex` aside, which the editor
/// never sets).
const TABBABLE: &str = "a[href], button, input, select, textarea, [tabindex]";

/// The elements `Tab` stops at on the page, in order, leaving out those in
/// `skip`.
fn tabbables(skip: &Element) -> Vec<HtmlElement> {
    let Some(found) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.query_selector_all(TABBABLE).ok())
    else {
        return Vec::new();
    };
    (0..found.length())
        .filter_map(|index| found.item(index))
        .filter_map(|node| node.dyn_into::<HtmlElement>().ok())
        .filter(|element| {
            element.tab_index() >= 0
                && !element.has_attribute("disabled")
                && element.get_client_rects().length() > 0
                && element.closest("[inert]").ok().flatten().is_none()
                && !skip.contains(Some(element))
        })
        .collect()
}

/// The element `Tab` goes to after `from`, as though `skip` (a popup
/// placed elsewhere in the page) were not there; `None` at the page's end.
fn next_tabbable(from: &Element, skip: &Element) -> Option<HtmlElement> {
    tabbables(skip).into_iter().find(|element| {
        from.compare_document_position(element) & Node::DOCUMENT_POSITION_FOLLOWING != 0
    })
}
