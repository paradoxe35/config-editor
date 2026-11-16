//! Bringing a find match into view.

use wasm_bindgen::JsCast;

use super::super::metrics::read_editor_metrics;
use super::super::positioning::reveal_offset;
use super::super::source_area::SourceArea;
use crate::editor::text::TextSize;

/// Scrolls byte `offset` into view as VS Code reveals a find match: left
/// alone when already in view below `top_inset` pixels (a widget over the
/// text), else its line centered.
pub(super) fn reveal_centered(editor: &SourceArea, text: &str, offset: TextSize, top_inset: f64) {
    let byte_offset = usize::from(offset.min(TextSize::of(text)));
    let line = text[..byte_offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let line = editor.row_of_line(line) as f64;
    let target: &web_sys::Element = editor.element().unchecked_ref();
    let metrics = read_editor_metrics(target);
    let current_top = editor.scroll_top() as f64;
    let client_height = editor.client_height() as f64;
    let line_top = metrics.padding_top + line * metrics.line_height;
    let visible = line_top >= current_top + top_inset
        && line_top + metrics.line_height <= current_top + client_height;
    if !visible {
        let centered = (line_top - (client_height - metrics.line_height) / 2.0)
            .min(line_top - top_inset)
            .clamp(
                0.0,
                (editor.scroll_height() as f64 - client_height).max(0.0),
            );
        editor.set_scroll_top(centered as i32);
    }
    // Sideways too.
    reveal_offset(editor, text, offset);
}
