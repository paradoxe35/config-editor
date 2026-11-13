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
