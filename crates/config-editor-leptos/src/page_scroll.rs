//! `PageDown` / `PageUp` never scroll the host page or its scrollers.
//!
//! From a control without a page of its own (the find and Go to Line
//! fields, a button, a documents tab), the browser scrolls the nearest
//! scroller around it, the host page or a box of the host's. The editor's
//! roots (an editor's, the documents view's) take every page key that
//! bubbles up to them still unhandled: a popup of the editor that scrolls
//! (the change preview) pages itself, and nothing outside moves. The text
//! area pages itself first (see `editor::keys::page_keys`), and so does a
//! focused hover card.

use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, KeyboardEvent};

/// Leaves this much of a page in view after a page key, in pixels.
const KEPT: i32 = 18;

/// A page key from inside `root`: pages the scroller of the editor's
/// around the focus, if any, and keeps it from anything else.
pub(crate) fn keep_inside(root: &Element, event: &KeyboardEvent) {
    let down = match event.key().as_str() {
        "PageDown" => true,
        "PageUp" => false,
        _ => return,
    };
    // Ctrl+PageDown is the browser's tab switch, which moves no page.
    if event.default_prevented() || event.ctrl_key() || event.meta_key() {
        return;
    }
    event.prevent_default();
    let Some(from) = event
        .target()
        .and_then(|target| target.dyn_into::<Element>().ok())
    else {
        return;
    };
    if let Some(scroller) = own_scroller(root, &from) {
        let page = (scroller.client_height() - KEPT).max(KEPT);
        let top = scroller.scroll_top();
        scroller.set_scroll_top(if down { top + page } else { top - page });
    }
}

/// The nearest element from `from` up to (not including) `root` that
/// scrolls vertically.
fn own_scroller(root: &Element, from: &Element) -> Option<HtmlElement> {
    let window = web_sys::window()?;
    let mut at = Some(from.clone());
    while let Some(element) = at {
        if &element == root {
            return None;
        }
        let overflows = element.scroll_height() > element.client_height();
        if overflows
            && let Ok(Some(style)) = window.get_computed_style(&element)
            && matches!(
                style.get_property_value("overflow-y").ok().as_deref(),
                Some("auto" | "scroll")
            )
        {
            return element.dyn_into::<HtmlElement>().ok();
        }
        at = element.parent_element();
    }
    None
}
