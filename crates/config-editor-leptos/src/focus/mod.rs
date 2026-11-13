//! Focus from code without moving the host page.
//!
//! `element.focus()` also scrolls every scrollable ancestor, the page
//! included, to bring the element into view. The editor's text area is as
//! tall as the document, so a focus on mount, on a tab switch or from the
//! host's `focus()` jumped the page down and hid the host's own controls
//! above the editor. Every focus the editor gives itself goes through here
//! (clippy bans the plain `focus()`): the page stays where it is, and the
//! editor reveals the caret inside its own scroll area as usual.

mod keeper;

pub(crate) use keeper::keep_focus;

use wasm_bindgen::JsValue;
#[cfg(any(feature = "search", feature = "diff"))]
use web_sys::{Element, ScrollIntoViewOptions, ScrollLogicalPosition};
use web_sys::{FocusOptions, HtmlElement};

/// Focuses `element` without scrolling anything.
pub(crate) fn in_place(element: &HtmlElement) -> Result<(), JsValue> {
    let options = FocusOptions::new();
    options.set_prevent_scroll(true);
    #[allow(
        clippy::disallowed_methods,
        reason = "the one place that focuses, with preventScroll"
    )]
    element.focus_with_options(&options)
}

/// Focuses `element` for a user who just asked to go there (a key that opens
/// a field, a `Tab` the editor carries to the next control), and brings it
/// into view by the shortest scroll if it is not: a control focused outside
/// the window (a zoomed page, an editor scrolled past) is lost to a keyboard
/// user, and `in_place` alone never scrolls to it.
#[cfg(any(feature = "search", feature = "diff"))]
pub(crate) fn in_view(element: &HtmlElement) -> Result<(), JsValue> {
    in_place(element)?;
    reveal(element);
    Ok(())
}

/// Scrolls `element` into view by the shortest scroll (nothing if it is
/// already shown).
#[cfg(any(feature = "search", feature = "diff"))]
fn reveal(element: &Element) {
    let options = ScrollIntoViewOptions::new();
    options.set_block(ScrollLogicalPosition::Nearest);
    options.set_inline(ScrollLogicalPosition::Nearest);
    element.scroll_into_view_with_scroll_into_view_options(&options);
}

/// Keeps what has the focus inside `container` in view after `container`
/// changed size or shape (a find row that wraps under its focused button).
#[cfg(feature = "search")]
pub(crate) fn keep_in_view(container: &Element) {
    if let Ok(Some(focused)) = container.query_selector(":focus") {
        reveal(&focused);
    }
}
