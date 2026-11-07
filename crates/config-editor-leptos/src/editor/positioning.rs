use super::metrics::read_editor_metrics;
use super::source_area::SourceArea;
use super::state::EditorState;
use crate::reactive::later;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Hover delay in milliseconds, so moving across keys does not flash cards.
pub(super) const HOVER_DELAY_MS: i32 = 350;

/// Viewport edge margin for caret-anchored popups, in pixels.
pub(super) const POPUP_MARGIN: f64 = 8.0;

/// Gap between the anchored line and a caret-anchored popup, in pixels.
pub(super) const POPUP_GAP: f64 = 4.0;

/// Viewport size when `window.inner_width/height` is unavailable; keep in sync
/// with `diff` and `completion`.
pub(super) const FALLBACK_VIEWPORT_WIDTH: f64 = 1024.0;
pub(super) const FALLBACK_VIEWPORT_HEIGHT: f64 = 768.0;

/// Matches `max-width` on `.config-editor__hover` in `preset.css`.
pub(super) const HOVER_CARD_MAX_WIDTH: f64 = 340.0;

/// Card height assumed when deciding to flip above the line; matches the
/// editor `min-height` in `preset.css`.
pub(super) const HOVER_CARD_ESTIMATED_HEIGHT: f64 = 240.0;

/// Zero-based document line and column of the textarea cell under a
/// pointer position.
pub(super) fn pointer_text_position(element: &SourceArea, x: f64, y: f64) -> (usize, usize) {
    let target: &web_sys::Element = element.element().unchecked_ref();
    let rect = target.get_bounding_client_rect();
    let metrics = read_editor_metrics(target);
    let line =
        (y - rect.top() - metrics.padding_top + element.scroll_top() as f64) / metrics.line_height;
    let column = (x - rect.left() - metrics.padding_left + element.scroll_left() as f64)
        / metrics.char_width;
    (
        element.line_of_row(line.max(0.0).floor() as usize),
        column.max(0.0).floor() as usize,
    )
}

/// Fixed-position style for the hover card; flips above the line when space
/// below is short.
pub(super) fn hover_card_style(element: &SourceArea, line: usize, column: usize) -> String {
    let target: &web_sys::Element = element.element().unchecked_ref();
    let line = element.row_of_line(line);
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
    // At most the window's width, so narrow screens do not clip the text.
    let width = HOVER_CARD_MAX_WIDTH.min(viewport_width - 2.0 * POPUP_MARGIN);
    let left = (rect.left() + metrics.padding_left + column as f64 * metrics.char_width
        - element.scroll_left() as f64)
        .clamp(
            POPUP_MARGIN,
            (viewport_width - width - POPUP_MARGIN).max(POPUP_MARGIN),
        );
    let line_top = rect.top() + metrics.padding_top + line as f64 * metrics.line_height
        - element.scroll_top() as f64;
    // Room inside the visible part of the editor, not the whole window.
    let room_below = rect.bottom().min(viewport_height) - (line_top + metrics.line_height);
    let room_above = line_top - rect.top().max(0.0);
    if room_below < HOVER_CARD_ESTIMATED_HEIGHT && room_above > room_below {
        format!(
            "left:{left:.1}px;top:{line_top:.1}px;max-width:{width:.1}px;transform:translateY(calc(-100% - {POPUP_GAP}px))"
        )
    } else {
        format!(
            "left:{left:.1}px;top:{:.1}px;max-width:{width:.1}px",
            line_top + metrics.line_height + POPUP_GAP
        )
    }
}

/// Places the caret and scrolls it into view, mirroring the scroll into
/// `scroll_y` for the highlight overlay.
///
/// `set_selection_range` alone does not reliably scroll the textarea.
pub(super) fn set_caret_visible(
    editor: &SourceArea,
    text: &str,
    offset: usize,
    scroll_y: RwSignal<f64>,
) {
    let offset = offset.min(text.encode_utf16().count());
    let _ = editor.set_selection_range(offset as u32, offset as u32);
    reveal_offset(editor, text, offset, scroll_y);
}

/// Scrolls UTF-16 `offset` into view without touching the selection.
pub(super) fn reveal_offset(
    editor: &SourceArea,
    text: &str,
    offset: usize,
    scroll_y: RwSignal<f64>,
) {
    use super::keyboard::utf16_to_byte_index;

    let offset = offset.min(text.encode_utf16().count());
    let byte_offset = utf16_to_byte_index(text, offset);
    let line = text[..byte_offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let line = editor.row_of_line(line) as f64;
    let target: &web_sys::Element = editor.element().unchecked_ref();
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
    // Horizontally too: a caret back at the start of a line after a long one
    // must not stay scrolled off to the left.
    let line_start = text[..byte_offset].rfind('\n').map_or(0, |index| index + 1);
    let caret_x =
        super::completion::visual_column(&text[line_start..byte_offset]) * metrics.char_width;
    let current_left = editor.scroll_left() as f64;
    let client_width = editor.client_width() as f64 - metrics.padding_left;
    if caret_x < current_left {
        editor.set_scroll_left((caret_x - 4.0 * metrics.char_width).max(0.0) as i32);
    } else if caret_x > current_left + client_width - 2.0 * metrics.char_width {
        editor.set_scroll_left((caret_x - client_width + 4.0 * metrics.char_width) as i32);
    }
}

/// Runs `callback` once the pointer has rested, if the editor is still
/// mounted.
pub(super) fn schedule_hover(state: EditorState, callback: impl FnOnce(EditorState) + 'static) {
    later::after(state, HOVER_DELAY_MS, callback);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_geometry_constants_match_the_preset_stylesheet() {
        assert_eq!(POPUP_MARGIN, 8.0);
        assert_eq!(POPUP_GAP, 4.0);
        assert_eq!(FALLBACK_VIEWPORT_WIDTH, 1024.0);
        assert_eq!(FALLBACK_VIEWPORT_HEIGHT, 768.0);
        assert_eq!(HOVER_CARD_MAX_WIDTH, 340.0);
        assert_eq!(HOVER_CARD_ESTIMATED_HEIGHT, 240.0);
    }
}
