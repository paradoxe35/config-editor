use super::metrics::read_editor_metrics;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Delay before schema hover documentation appears, in milliseconds. Long
/// enough to stay out of the way while moving the mouse across keys.
pub(super) const HOVER_DELAY_MS: i32 = 350;

/// Viewport edge margin for caret-anchored popups, in pixels.
pub(super) const POPUP_MARGIN: f64 = 8.0;

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
        .unwrap_or(1024.0);
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(768.0);
    let left = (rect.left()
        + metrics.padding_left
        + column as f64 * metrics.char_width
        - element.scroll_left() as f64)
        .clamp(POPUP_MARGIN, (viewport_width - 348.0).max(POPUP_MARGIN));
    let line_top = rect.top() + metrics.padding_top + line as f64 * metrics.line_height
        - element.scroll_top() as f64;
    if line_top > 240.0 && line_top + metrics.line_height + 240.0 > viewport_height {
        format!("left:{left:.1}px;top:{line_top:.1}px;transform:translateY(calc(-100% - 4px))")
    } else {
        format!(
            "left:{left:.1}px;top:{:.1}px",
            line_top + metrics.line_height + 4.0
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
