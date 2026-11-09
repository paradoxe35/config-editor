//! Where a short tip about a place in the text goes: under (or above) it,
//! following it as the text scrolls (the protected text's tip, the
//! read-only editor's notice).

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::completion::visual_column;
use super::keyboard::utf16_to_byte_index;
use super::metrics::read_editor_metrics;
use super::positioning::hover_card_style;
use super::source_area::SourceArea;
use super::state::EditorState;
use crate::cssom::Declarations;

/// The style of a tip at document offset `offset` (UTF-16); `None` while
/// that is out of view. Tracks the scroll and layout.
pub(super) fn caret_tip_style(state: EditorState, offset: usize) -> Option<Declarations> {
    let element = state.textarea()?;
    state.view().layout_revision.track();
    state.view().scroll_x.track();
    state.view().scroll_y.track();
    let (line, column) = state.doc().text.with_untracked(|text| {
        let byte = utf16_to_byte_index(text, offset);
        let line_start = text[..byte].rfind('\n').map_or(0, |at| at + 1);
        (
            text[..line_start].matches('\n').count(),
            visual_column(&text[line_start..byte]) as usize,
        )
    });
    line_in_view(&element, line).then(|| hover_card_style(&element, line, column))
}

/// Whether any of zero-based `line` is inside the text area's scrolled view.
fn line_in_view(element: &SourceArea, line: usize) -> bool {
    let target: &web_sys::Element = element.element().unchecked_ref();
    let metrics = read_editor_metrics(target);
    let top = metrics.padding_top + element.row_of_line(line) as f64 * metrics.line_height
        - element.scroll_top() as f64;
    top + metrics.line_height > 0.0 && top < element.client_height() as f64
}
