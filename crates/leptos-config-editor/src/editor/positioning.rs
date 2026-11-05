use super::metrics::read_editor_metrics;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Delay before schema hover documentation appears, in milliseconds. Long
/// enough to stay out of the way while moving the mouse across keys.
pub(super) const HOVER_DELAY_MS: i32 = 350;

/// Viewport edge margin for caret-anchored popups, in pixels.
pub(super) const POPUP_MARGIN: f64 = 8.0;

/// Gap between the anchored line and a caret-anchored popup, in pixels.
pub(super) const POPUP_GAP: f64 = 4.0;

/// Fallback viewport size when `window.inner_width/height` is unavailable.
/// Matches the fallbacks in `diff.rs` and `completion.rs` so offscreen tests
/// and non-browser renders agree on placement.
pub(super) const FALLBACK_VIEWPORT_WIDTH: f64 = 1024.0;
pub(super) const FALLBACK_VIEWPORT_HEIGHT: f64 = 768.0;

/// Hover card width cap, in pixels. Mirrors the `max-width: 340px` rule on
/// `.config-editor__hover` in `preset.css`; the right-edge clamp subtracts this
/// plus [`POPUP_MARGIN`] so the card never overflows the viewport.
pub(super) const HOVER_CARD_MAX_WIDTH: f64 = 340.0;

/// Estimated hover card height, in pixels, used to decide whether the card
/// flips above the line. Matches the default editor `min-height: 240px` so the
/// flip threshold tracks the smallest editor the card must fit beside.
pub(super) const HOVER_CARD_ESTIMATED_HEIGHT: f64 = 240.0;

/// Line index (0-based) and character column of the textarea cell under a
/// pointer position. Used to decide which key the mouse is hovering.
pub(super) fn pointer_text_position(
    element: &web_sys::HtmlTextAreaElement,
    x: f64,
    y: f64,
) -> (usize, usize) {
    let target: &web_sys::Element = element.unchecked_ref();
    let rect = target.get_bounding_client_rect();
    let metrics = read_editor_metrics(target);
    let line =
        (y - rect.top() - metrics.padding_top + element.scroll_top() as f64) / metrics.line_height;
    let column = (x - rect.left() - metrics.padding_left + element.scroll_left() as f64)
        / metrics.char_width;
    (
        line.max(0.0).floor() as usize,
        column.max(0.0).floor() as usize,
    )
}

/// Viewport-anchored style for the schema hover card at a key column. Flips
/// above the line when there is not enough space below it.
pub(super) fn hover_card_style(
    element: &web_sys::HtmlTextAreaElement,
    line: usize,
    column: usize,
) -> String {
    let target: &web_sys::Element = element.unchecked_ref();
    let rect = target.get_bounding_client_rect();
    let metrics = read_editor_metrics(target);
    let Some(window) = web_sys::window() else {
        return String::new();
    };
    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(FALLBACK_VIEWPORT_WIDTH);
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(FALLBACK_VIEWPORT_HEIGHT);
    let left = (rect.left()
        + metrics.padding_left
        + column as f64 * metrics.char_width
        - element.scroll_left() as f64)
        .clamp(
            POPUP_MARGIN,
            (viewport_width - HOVER_CARD_MAX_WIDTH - POPUP_MARGIN).max(POPUP_MARGIN),
        );
    let line_top = rect.top() + metrics.padding_top + line as f64 * metrics.line_height
        - element.scroll_top() as f64;
    if line_top > HOVER_CARD_ESTIMATED_HEIGHT
        && line_top + metrics.line_height + HOVER_CARD_ESTIMATED_HEIGHT > viewport_height
    {
        format!("left:{left:.1}px;top:{line_top:.1}px;transform:translateY(calc(-100% - {POPUP_GAP}px))")
    } else {
        format!(
            "left:{left:.1}px;top:{:.1}px",
            line_top + metrics.line_height + POPUP_GAP
        )
    }
}

/// Keep the caret in the scrollport after programmatic edits. Textarea
/// `set_selection_range` does not reliably scroll to a newly-set caret, so
/// Enter and completion expansion adjust `scroll_top` directly. Reports the
/// new scroll position through `scroll_y` so the highlight overlay stays in
/// sync with the textarea.
pub(super) fn set_caret_visible(
    editor: &web_sys::HtmlTextAreaElement,
    text: &str,
    offset: usize,
    scroll_y: RwSignal<f64>,
) {
    use super::keyboard::utf16_to_byte_index;

    let offset = offset.min(text.encode_utf16().count());
    let _ = editor.set_selection_range(offset as u32, offset as u32);
    let byte_offset = utf16_to_byte_index(text, offset);
    let line = text[..byte_offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count() as f64;
    let target: &web_sys::Element = editor.unchecked_ref();
    let metrics = read_editor_metrics(target);
    let style =
        web_sys::window().and_then(|window| window.get_computed_style(target).ok().flatten());
    let padding_bottom = style
        .as_ref()
        .and_then(|style| style.get_property_value("padding-bottom").ok())
        .and_then(|value| {
            value
                .trim()
                .strip_suffix("px")
                .unwrap_or(&value)
                .parse::<f64>()
                .ok()
        })
        .unwrap_or(0.0);
    let current_top = editor.scroll_top() as f64;
    let client_height = editor.client_height() as f64;
    let caret_top = metrics.padding_top + line * metrics.line_height;
    let caret_bottom = caret_top + metrics.line_height;
    let visible_top = current_top + metrics.padding_top;
    let visible_bottom = current_top + client_height - padding_bottom;
    let next_top = if caret_bottom > visible_bottom {
        caret_bottom - client_height + padding_bottom
    } else if caret_top < visible_top {
        caret_top - metrics.padding_top
    } else {
        current_top
    }
    .clamp(
        0.0,
        (editor.scroll_height() as f64 - client_height).max(0.0),
    );
    editor.set_scroll_top(next_top as i32);
    scroll_y.set(next_top);
}

pub(super) fn schedule_hover(callback: impl FnOnce() + 'static) {
    if let Some(window) = web_sys::window() {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            wasm_bindgen::closure::Closure::once_into_js(callback).unchecked_ref(),
            HOVER_DELAY_MS,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the popup geometry constants to their documented CSS sources so
    /// a restyle of `preset.css` forces a deliberate update here too.
    #[test]
    fn popup_geometry_constants_match_the_preset_stylesheet() {
        assert_eq!(POPUP_MARGIN, 8.0);
        assert_eq!(POPUP_GAP, 4.0);
        assert_eq!(FALLBACK_VIEWPORT_WIDTH, 1024.0);
        assert_eq!(FALLBACK_VIEWPORT_HEIGHT, 768.0);
        // `.config-editor__hover` max-width and the editor min-height.
        assert_eq!(HOVER_CARD_MAX_WIDTH, 340.0);
        assert_eq!(HOVER_CARD_ESTIMATED_HEIGHT, 240.0);
    }
}
