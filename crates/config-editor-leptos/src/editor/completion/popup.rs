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

/// The rendered column after `text`, with tabs at their tab stops and
/// marks drawn over the character before them (`e` + U+0301 is one `é`)
/// taking no column.
pub(crate) fn visual_column(text: &str) -> f64 {
    let mut column = 0.0;
    for character in text.chars() {
        if character == '\t' {
            column += COMPLETION_TAB_STOP_WIDTH - (column % COMPLETION_TAB_STOP_WIDTH);
        } else if !zero_width(character) {
            column += 1.0;
        }
    }
    column
}

/// Whether `character` is drawn without a width of its own: a combining
/// mark, a variation selector, or a zero-width joiner or space.
fn zero_width(character: char) -> bool {
    matches!(
        u32::from(character),
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x200B..=0x200F
            | 0x20D0..=0x20FF
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            | 0xE0100..=0xE01EF
    )
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

/// Popup style for a caret at `frame` coordinates: below the caret when it
/// fits, otherwise above, clamped to the window.
pub(crate) fn completion_popup_geometry(
    frame: &PopupFrame,
    caret_x: f64,
    caret_y: f64,
    line_height: f64,
    option_count: usize,
) -> String {
    let (viewport_width, viewport_height) = (frame.viewport_width, frame.viewport_height);
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
    let below = space_below >= ideal_height || space_below >= space_above;
    let available_height = if below { space_below } else { space_above };
    let max_height = ideal_height.min(available_height).max(0.0);
    let left = frame.left(left);
    if below {
        format!(
            "position:fixed;left:{left:.1}px;top:{:.1}px;width:{width:.1}px;max-height:{max_height:.1}px",
            frame.top(caret_y + line_height)
        )
    } else {
        // Its bottom on the caret's line, however tall its rows turn out.
        format!(
            "position:fixed;left:{left:.1}px;top:{:.1}px;width:{width:.1}px;max-height:{max_height:.1}px;transform:translateY(-100%)",
            frame.top(caret_y)
        )
    }
}

pub(crate) fn completion_popup_style(
    state: EditorState,
    editor: &web_sys::HtmlTextAreaElement,
    projection: &super::super::projection::Projection,
    source: &str,
    cursor: usize,
    option_count: usize,
) -> String {
    let element: &web_sys::Element = editor.unchecked_ref();
    let frame = PopupFrame::measure(state);
    let rect = frame.bounds(&element.get_bounding_client_rect());
    let metrics = read_editor_metrics(element);
    let prefix = &source[..utf16_to_byte_index(source, cursor)];
    let (line, column) = prefix
        .rsplit_once('\n')
        .map_or((0, visual_column(prefix)), |(before, after)| {
            (before.matches('\n').count() + 1, visual_column(after))
        });
    let caret_x = rect.left + metrics.padding_left + column * metrics.char_width
        - editor.scroll_left() as f64;
    // The caret's row, folded lines skipped.
    let line = projection.row_of_line(line);
    let caret_y = rect.top + metrics.padding_top + line as f64 * metrics.line_height
        - editor.scroll_top() as f64;
    completion_popup_geometry(&frame, caret_x, caret_y, metrics.line_height, option_count)
}

#[cfg(test)]
mod tests {
    use super::visual_column;

    /// QA 17: an underline after `é` written as `e` + U+0301 sat one column
    /// to the right: the mark takes no column of its own, in every overlay
    /// that places itself by column (problems, matches, locks, fold chips).
    #[test]
    fn marks_over_a_character_take_no_column() {
        let cases = [
            ("e\u{301}x", 2.0),
            ("caf\u{e9}", 4.0),
            ("cafe\u{301}", 4.0),
            ("a\u{20dd}b", 2.0),
            ("\u{2764}\u{fe0f}", 1.0),
            ("a\u{200d}b", 2.0),
            ("\tx", 3.0),
        ];
        let failures: Vec<String> = cases
            .iter()
            .filter(|(text, expected)| visual_column(text) != *expected)
            .map(|(text, expected)| format!("{text:?}: {} not {expected}", visual_column(text)))
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
