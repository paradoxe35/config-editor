//! `PageDown` / `PageUp` in the text scroll the editor only. The browser
//! moves the caret by a page and scrolls the text; Chrome also scrolls
//! every scroller around it (the host page too) to put the caret at their
//! top. When the text is wholly in view the caret stays in view without
//! that, so those scroll positions come back before the next frame is
//! drawn.

use wasm_bindgen::JsCast;
use web_sys::{DomRect, Element, KeyboardEvent};

use super::super::state::EditorState;
use crate::reactive::later;

/// Keeps the page still on a `PageDown` / `PageUp` the browser handles.
pub(super) fn keep_page_still(state: EditorState, event: &KeyboardEvent) {
    if !matches!(event.key().as_str(), "PageDown" | "PageUp")
        || event.alt_key()
        || event.ctrl_key()
        || event.meta_key()
    {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(text) = state.textarea() else {
        return;
    };
    let text: &Element = text.element().unchecked_ref();
    let Some(saved) = scroll_positions(&window, text) else {
        return;
    };
    let x = window.scroll_x().unwrap_or(0.0);
    let y = window.scroll_y().unwrap_or(0.0);
    later::next_frame(state, move |_| {
        for (element, top, left) in saved {
            if element.scroll_top() != top {
                element.set_scroll_top(top);
            }
            if element.scroll_left() != left {
                element.set_scroll_left(left);
            }
        }
        if window.scroll_x().ok() != Some(x) || window.scroll_y().ok() != Some(y) {
            window.scroll_to_with_x_and_y(x, y);
        }
    });
}

/// The scroll positions of `text`'s ancestors, or `None` when `text` is
/// not wholly in view in the window and in each scroller around it.
fn scroll_positions(window: &web_sys::Window, text: &Element) -> Option<Vec<(Element, i32, i32)>> {
    let rect = text.get_bounding_client_rect();
    let width = window.inner_width().ok()?.as_f64()?;
    let height = window.inner_height().ok()?.as_f64()?;
    if !inside(&rect, [0.0, 0.0, width, height]) {
        return None;
    }
    let root = window
        .document()
        .and_then(|document| document.document_element());
    let mut saved = Vec::new();
    let mut ancestor = text.parent_element();
    while let Some(element) = ancestor {
        let scrolls = element.scroll_height() > element.client_height()
            || element.scroll_width() > element.client_width();
        if scrolls && root.as_ref() != Some(&element) {
            let outer = element.get_bounding_client_rect();
            if !inside(
                &rect,
                [outer.left(), outer.top(), outer.right(), outer.bottom()],
            ) {
                return None;
            }
        }
        saved.push((element.clone(), element.scroll_top(), element.scroll_left()));
        ancestor = element.parent_element();
    }
    Some(saved)
}

/// Whether `rect` lies within the box `[left, top, right, bottom]`, to the
/// pixel.
fn inside(rect: &DomRect, [left, top, right, bottom]: [f64; 4]) -> bool {
    rect.left() >= left - 1.0
        && rect.top() >= top - 1.0
        && rect.right() <= right + 1.0
        && rect.bottom() <= bottom + 1.0
}
