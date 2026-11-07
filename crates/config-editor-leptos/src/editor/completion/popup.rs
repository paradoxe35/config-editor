//! Where the suggestion list opens and how it scrolls.

use super::*;

/// Rows PageUp/PageDown move: one popup page
/// (`COMPLETION_POPUP_MAX_HEIGHT / COMPLETION_ROW_HEIGHT`).
pub(crate) const COMPLETION_PAGE_ROWS: usize = 7;

pub(crate) fn scroll_completion_into_view(list: &web_sys::HtmlDivElement, index: usize) {
    let Some(option) = list
        .children()
        .item(index as u32)
        .and_then(|option| option.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };

    let top = option.offset_top();
    let bottom = top + option.offset_height();
    let scroll_top = list.scroll_top();
    let visible_bottom = scroll_top + list.client_height();
    if top < scroll_top {
        list.set_scroll_top(top);
    } else if bottom > visible_bottom {
        list.set_scroll_top(bottom - list.client_height());
    }
}

/// Tab width, in columns, for estimating the caret's x position without
/// measuring text; close enough for the short keys suggested.
pub(crate) const COMPLETION_TAB_STOP_WIDTH: f64 = 2.0;

/// The rendered column after `text`, with tabs at their tab stops.
pub(crate) fn visual_column(text: &str) -> f64 {
    let mut column = 0.0;
    for character in text.chars() {
        if character == '\t' {
            column += COMPLETION_TAB_STOP_WIDTH - (column % COMPLETION_TAB_STOP_WIDTH);
        } else {
            column += 1.0;
        }
    }
    column
}

/// Completion popup width, in pixels: room for key, type and detail.
pub(crate) const COMPLETION_POPUP_WIDTH: f64 = 420.0;

/// Total horizontal gutter, in pixels, kept between the popup and the
/// viewport edges.
pub(crate) const COMPLETION_POPUP_VIEWPORT_GUTTER: f64 = 16.0;

/// Height of one suggestion row, in pixels; mirrors the
/// `.config-editor__completion` rule in `preset.css`.
pub(crate) const COMPLETION_ROW_HEIGHT: f64 = 44.0;

/// Vertical padding of the completion list, in pixels.
pub(crate) const COMPLETION_LIST_PADDING: f64 = 2.0;

/// Completion popup height cap, in pixels (about seven rows).
pub(crate) const COMPLETION_POPUP_MAX_HEIGHT: f64 = 320.0;

/// Popup style for a caret at viewport coordinates: below the caret when it
/// fits, otherwise above, clamped to the viewport.
pub(crate) fn completion_popup_geometry(
    viewport_width: f64,
    viewport_height: f64,
    caret_x: f64,
    caret_y: f64,
    line_height: f64,
    option_count: usize,
) -> String {
    let width =
        COMPLETION_POPUP_WIDTH.min((viewport_width - COMPLETION_POPUP_VIEWPORT_GUTTER).max(0.0));
    let left = caret_x.clamp(
        POPUP_MARGIN,
        (viewport_width - width - POPUP_MARGIN).max(POPUP_MARGIN),
    );
    let ideal_height = (option_count as f64 * COMPLETION_ROW_HEIGHT + COMPLETION_LIST_PADDING)
        .min(COMPLETION_POPUP_MAX_HEIGHT);
    let space_below = (viewport_height - caret_y - line_height - POPUP_MARGIN).max(0.0);
    let space_above = (caret_y - POPUP_MARGIN).max(0.0);
    let (top, available_height) = if space_below >= ideal_height || space_below >= space_above {
        (caret_y + line_height, space_below)
    } else {
        ((caret_y - ideal_height).max(POPUP_MARGIN), space_above)
    };
    let max_height = ideal_height.min(available_height).max(0.0);
    format!(
        "position:fixed;left:{left:.1}px;top:{top:.1}px;width:{width:.1}px;max-height:{max_height:.1}px"
    )
}

pub(crate) fn completion_popup_style(
    editor: &web_sys::HtmlTextAreaElement,
    projection: &super::super::projection::Projection,
    source: &str,
    cursor: usize,
    option_count: usize,
) -> String {
    let Some(window) = web_sys::window() else {
        return "position:fixed;visibility:hidden".to_string();
    };
    let element: &web_sys::Element = editor.unchecked_ref();
    let rect = element.get_bounding_client_rect();
    let metrics = read_editor_metrics(element);
    let prefix = &source[..utf16_to_byte_index(source, cursor)];
    let (line, column) = prefix
        .rsplit_once('\n')
        .map_or((0, visual_column(prefix)), |(before, after)| {
            (before.matches('\n').count() + 1, visual_column(after))
        });
    let caret_x = rect.left() + metrics.padding_left + column * metrics.char_width
        - editor.scroll_left() as f64;
    // The caret's row, folded lines skipped.
    let line = projection.row_of_line(line);
    let caret_y = rect.top() + metrics.padding_top + line as f64 * metrics.line_height
        - editor.scroll_top() as f64;
    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|width| width.as_f64())
        .unwrap_or(FALLBACK_VIEWPORT_WIDTH);
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|height| height.as_f64())
        .unwrap_or(FALLBACK_VIEWPORT_HEIGHT);
    completion_popup_geometry(
        viewport_width,
        viewport_height,
        caret_x,
        caret_y,
        metrics.line_height,
        option_count,
    )
}
