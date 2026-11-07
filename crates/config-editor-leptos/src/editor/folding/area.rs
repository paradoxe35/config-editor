//! [`SourceArea`]: the textarea seen through the folds. Every part of the
//! editor reads and moves the selection through it in document offsets;
//! only it knows the textarea holds the document with folded lines cut out.

use leptos::prelude::*;
use wasm_bindgen::JsValue;

use super::super::state::EditorState;
use super::projection::Side;

/// The textarea, with offsets and lines in the document's terms.
#[derive(Clone)]
pub(crate) struct SourceArea {
    element: web_sys::HtmlTextAreaElement,
    state: EditorState,
}

impl SourceArea {
    pub(crate) fn new(element: web_sys::HtmlTextAreaElement, state: EditorState) -> Self {
        Self { element, state }
    }

    /// The element itself, for its geometry and events. Never read or set
    /// its text or selection directly: those are in the textarea's terms.
    pub(crate) fn element(&self) -> &web_sys::HtmlTextAreaElement {
        &self.element
    }

    /// The document as the textarea now stands for it: an edit the browser
    /// made to the textarea's text, mapped into the document.
    pub(crate) fn value(&self) -> String {
        let current = self.element.value();
        self.state.fold.projection.with_untracked(|projection| {
            self.state.doc.text.with_untracked(|doc| {
                if projection.view(doc) == current {
                    doc.clone()
                } else {
                    projection.unproject(doc, &current)
                }
            })
        })
    }

    /// Writing the text goes through `EditorState::write_text`, which keeps
    /// the folds in step; this private stand-in keeps the element's own
    /// `set_value` from being reached by mistake.
    #[allow(dead_code)]
    fn set_value(&self, _text: &str) {}

    fn to_doc(&self, offset: Result<Option<u32>, JsValue>) -> Result<Option<u32>, JsValue> {
        let offset = offset?;
        Ok(offset.map(|offset| {
            self.state
                .fold
                .projection
                .with_untracked(|projection| projection.to_doc(offset as usize, Side::Before))
                as u32
        }))
    }

    fn to_view(&self, offset: u32) -> u32 {
        self.state
            .fold
            .projection
            .with_untracked(|projection| projection.to_view(offset as usize)) as u32
    }

    pub(crate) fn selection_start(&self) -> Result<Option<u32>, JsValue> {
        self.to_doc(self.element.selection_start())
    }

    pub(crate) fn selection_end(&self) -> Result<Option<u32>, JsValue> {
        self.to_doc(self.element.selection_end())
    }

    pub(crate) fn selection_direction(&self) -> Result<Option<String>, JsValue> {
        self.element.selection_direction()
    }

    /// Selects document offsets `start..end`, first opening the folds that
    /// hide either end: whatever moves the caret shows where it went.
    pub(crate) fn set_selection_range(&self, start: u32, end: u32) -> Result<(), JsValue> {
        self.state.reveal_offsets(start as usize, end as usize);
        self.element
            .set_selection_range(self.to_view(start), self.to_view(end))
    }

    /// Selects document offsets `start..end` without opening any fold (a
    /// selection put back as it was, such as after a refused edit; an end
    /// inside a fold goes to its edge).
    pub(crate) fn restore_selection(&self, start: u32, end: u32) -> Result<(), JsValue> {
        self.element
            .set_selection_range(self.to_view(start), self.to_view(end))
    }

    /// [`set_selection_range`](Self::set_selection_range) with a direction.
    pub(crate) fn set_selection_range_with_direction(
        &self,
        start: u32,
        end: u32,
        direction: &str,
    ) -> Result<(), JsValue> {
        self.state.reveal_offsets(start as usize, end as usize);
        self.element.set_selection_range_with_direction(
            self.to_view(start),
            self.to_view(end),
            direction,
        )
    }

    pub(crate) fn focus(&self) -> Result<(), JsValue> {
        self.element.focus()
    }

    pub(crate) fn scroll_top(&self) -> i32 {
        self.element.scroll_top()
    }

    pub(crate) fn set_scroll_top(&self, top: i32) {
        self.element.set_scroll_top(top);
    }

    pub(crate) fn scroll_left(&self) -> i32 {
        self.element.scroll_left()
    }

    pub(crate) fn set_scroll_left(&self, left: i32) {
        self.element.set_scroll_left(left);
    }

    pub(crate) fn client_height(&self) -> i32 {
        self.element.client_height()
    }

    pub(crate) fn client_width(&self) -> i32 {
        self.element.client_width()
    }

    pub(crate) fn scroll_height(&self) -> i32 {
        self.element.scroll_height()
    }

    /// The textarea row of document line `line` (0-based); a hidden line is
    /// on its fold's row.
    pub(crate) fn row_of_line(&self, line: usize) -> usize {
        self.state
            .fold
            .projection
            .with_untracked(|projection| projection.row_of_line(line))
    }

    /// The document line (0-based) on textarea row `row`.
    pub(crate) fn line_of_row(&self, row: usize) -> usize {
        self.state
            .fold
            .projection
            .with_untracked(|projection| projection.line_of_row(row))
    }
}
