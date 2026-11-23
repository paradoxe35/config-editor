//! Room above the first line while the find widget is open, as tall as the
//! widget (VS Code's `editor.find.addExtraSpaceOnTop`): the text scrolls
//! below the widget, so no match, not even one on the first line, has to
//! stay under it.
//!
//! The room is the `--config-editor-find-space` variable on the surface,
//! added to the top padding of the text and of every layer drawn over it
//! (the gutter, the code, the marks), so the caret, pointer and popup math
//! that reads the text's padding follows it.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::super::state::EditorState;

/// The variable the stylesheet adds to the text's top padding.
const FIND_SPACE: &str = "--config-editor-find-space";

/// Sets the room to the open widget's height (none when it is closed),
/// and scrolls the text by as much as its top padding changed, so the
/// text stays where it was on the screen.
pub(super) fn fit_find_space(state: EditorState) {
    let search = state.search();
    let height = if search.open.get_untracked() {
        search
            .widget_ref
            .get_untracked()
            .map_or(0, |widget| widget.offset_height())
    } else {
        0
    };
    let Some(area) = state.textarea() else {
        return;
    };
    let Some(surface) = area
        .element()
        .closest(".config-editor__surface")
        .ok()
        .flatten()
        .and_then(|surface| surface.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };
    let style = surface.style();
    let value = format!("{height}px");
    if style.get_property_value(FIND_SPACE).ok().as_deref() == Some(value.as_str()) {
        return;
    }
    let before = padding_top(area.element());
    let _ = style.set_property(FIND_SPACE, &value);
    let moved = padding_top(area.element()) - before;
    if moved != 0 {
        area.set_scroll_top(area.scroll_top() + moved);
    }
}

/// The element's top padding in whole pixels, as the browser lays it out.
fn padding_top(element: &web_sys::Element) -> i32 {
    let pixels = web_sys::window()
        .and_then(|window| window.get_computed_style(element).ok().flatten())
        .and_then(|style| style.get_property_value("padding-top").ok())
        .and_then(|value| value.trim().trim_end_matches("px").parse::<f64>().ok())
        .unwrap_or(0.0);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a padding in pixels, far inside an `i32`"
    )]
    let pixels = pixels.round() as i32;
    pixels
}
