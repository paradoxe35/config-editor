//! `PageDown` / `PageUp` never scroll the host page or its scrollers.
//!
//! From a control without a page of its own (the find and Go to Line
//! fields, a button, a documents tab), the browser scrolls the nearest
//! scroller around it, the host page or a box of the host's. The editor's
//! roots (an editor's, the documents view's) take every page key that
//! bubbles up to them still unhandled: a popup of the editor that scrolls
//! (the change preview) pages itself, and nothing outside moves. The text
//! area pages itself first (see `editor::keys::page_keys`), and so does a
//! focused hover card; it measures its page by [`band_in_view`], and
//! [`reveal_band`] moves the boxes around it only as far as shows the
//! caret.

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

/// The band of `element`'s client box in view, as `(top, bottom)` in the
/// window's pixels: inside the window and every box around it that clips
/// its overflow; `None` when none of it is.
pub(crate) fn band_in_view(element: &Element) -> Option<(f64, f64)> {
    let window = web_sys::window()?;
    let rect = element.get_bounding_client_rect();
    let scale = scale_of(element);
    let mut top = rect.top() + f64::from(element.client_top()) * scale;
    let mut bottom = top + f64::from(element.client_height()) * scale;
    let mut at = element.parent_element();
    while let Some(around) = at {
        if clips(&window, &around)
            && let Some((clip_top, clip_bottom)) = client_band(&around)
        {
            top = top.max(clip_top);
            bottom = bottom.min(clip_bottom);
        }
        at = around.parent_element();
    }
    let (_, viewport_bottom) = viewport_band()?;
    top = top.max(0.0);
    bottom = bottom.min(viewport_bottom);
    (bottom > top).then_some((top, bottom))
}

/// How much bigger `element` shows than its own pixels (a host may scale
/// it with a transform or `zoom`).
pub(crate) fn scale_of(element: &Element) -> f64 {
    let height = element
        .dyn_ref::<HtmlElement>()
        .map_or(0, HtmlElement::offset_height);
    if height > 0 {
        element.get_bounding_client_rect().height() / f64::from(height)
    } else {
        1.0
    }
}

/// Scrolls the boxes around `element` that scroll (a host's box, the
/// page) as little as shows the band `top..bottom` (the window's pixels),
/// as the browser does for a caret it moves: nothing moves while it is in
/// view.
pub(crate) fn reveal_band(element: &Element, mut top: f64, mut bottom: f64) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let page = window
        .document()
        .and_then(|document| document.scrolling_element());
    let mut at = element.parent_element();
    while let Some(around) = at {
        let is_page = page.as_ref() == Some(&around);
        let band = if is_page {
            viewport_band()
        } else if scrolls(&window, &around) {
            client_band(&around)
        } else {
            None
        };
        if let Some((view_top, view_bottom)) = band {
            let by = if bottom > view_bottom {
                (bottom - view_bottom).min(top - view_top)
            } else if top < view_top {
                top - view_top
            } else {
                0.0
            };
            if by != 0.0 {
                let before = around.scroll_top();
                around.set_scroll_top(before + by.round() as i32);
                let moved = f64::from(around.scroll_top() - before);
                top -= moved;
                bottom -= moved;
            }
        }
        if is_page {
            return;
        }
        at = around.parent_element();
    }
}

/// The window's band, `(0, height)`, without a horizontal scroll bar.
fn viewport_band() -> Option<(f64, f64)> {
    let root = web_sys::window()?.document()?.document_element()?;
    Some((0.0, f64::from(root.client_height())))
}

/// The band of `element`'s client box (inside its borders and scroll
/// bars), in the window's pixels.
fn client_band(element: &Element) -> Option<(f64, f64)> {
    let rect = element.get_bounding_client_rect();
    let scale = scale_of(element);
    let top = rect.top() + f64::from(element.client_top()) * scale;
    Some((top, top + f64::from(element.client_height()) * scale))
}

/// Whether `element` clips what overflows it vertically.
fn clips(window: &web_sys::Window, element: &Element) -> bool {
    overflow_y(window, element).is_some_and(|overflow| overflow != "visible")
}

/// Whether `element` is a scroll box the user can scroll vertically,
/// with room to.
fn scrolls(window: &web_sys::Window, element: &Element) -> bool {
    element.scroll_height() > element.client_height()
        && overflow_y(window, element)
            .is_some_and(|overflow| matches!(overflow.as_str(), "auto" | "scroll" | "overlay"))
}

fn overflow_y(window: &web_sys::Window, element: &Element) -> Option<String> {
    window
        .get_computed_style(element)
        .ok()
        .flatten()?
        .get_property_value("overflow-y")
        .ok()
}
