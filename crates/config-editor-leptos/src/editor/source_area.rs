//! [`SourceArea`]: the textarea seen through the folds (see `projection`),
//! and writing the document into it. Every part of the editor reads and
//! moves the selection through it in byte offsets of the document; only it
//! knows the textarea counts UTF-16 code units and holds the document with
//! folded lines cut out. The offsets are converted here, once, both ways.

#![deny(
    clippy::string_slice,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use leptos::prelude::*;
use wasm_bindgen::JsValue;

use std::sync::Arc;

use super::keyboard::utf16_to_byte_index;
use super::projection::{Projection, Side};
use super::state::{EditorState, directed_selection};
use super::text::{TextRange, TextSize, Utf16, slice, text_size};

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

    /// The editor this text area belongs to.
    pub(crate) fn state(&self) -> EditorState {
        self.state
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
        // Where the browser left the caret: the end of the text it wrote,
        // as a byte of the textarea's text, as `unproject` reads it.
        let caret = Utf16::new(self.element.selection_end().ok().flatten().unwrap_or(0));
        self.state.view().projection.with_untracked(|projection| {
            self.state.doc().text.with_untracked(|doc| {
                if projection.view(doc) == current {
                    doc.clone()
                } else {
                    projection.unproject(doc, &current, Self::byte_in(&current, caret))
                }
            })
        })
    }

    /// Writing the text goes through the edit pipeline (see `edit`), which keeps
    /// the folds in step; this private stand-in keeps the element's own
    /// `set_value` from being reached by mistake.
    #[allow(dead_code)]
    fn set_value(&self, _text: &str) {}

    /// The document's UTF-16 offset of the textarea's `offset`.
    fn to_doc(&self, offset: u32) -> Utf16 {
        self.state
            .view()
            .projection
            .with_untracked(|projection| projection.doc_units(Utf16::new(offset), Side::Before))
    }

    /// The textarea's offset of the document's UTF-16 `offset`.
    fn to_view(&self, offset: Utf16) -> u32 {
        let view = self
            .state
            .view()
            .projection
            .with_untracked(|projection| projection.view_units(offset));
        u32::from(view)
    }

    /// The byte of `doc` (the document the textarea stands for, or the
    /// textarea's own text) at its UTF-16 `offset`; one inside a character
    /// is the character's end.
    fn byte_in(doc: &str, offset: Utf16) -> TextSize {
        text_size(utf16_to_byte_index(doc, usize::from(offset)))
    }

    /// The textarea's offset of byte `at` of the document.
    fn view_offset(&self, at: TextSize) -> u32 {
        let units = self.state.doc().text.with_untracked(|doc| {
            let at = at.min(TextSize::of(doc.as_str()));
            super::paths::units_at(doc, at)
        });
        self.to_view(units)
    }

    /// The selection's start and end as UTF-16 offsets of the document.
    fn selection_units(&self) -> (Utf16, Utf16) {
        let start = self.element.selection_start().ok().flatten().unwrap_or(0);
        let end = self.element.selection_end().ok().flatten().unwrap_or(start);
        (self.to_doc(start), self.to_doc(end))
    }

    /// The selection's start and end, as bytes of the document on screen.
    pub(crate) fn selection(&self) -> (TextSize, TextSize) {
        self.state
            .doc()
            .text
            .with_untracked(|doc| self.selection_in(doc))
    }

    /// The selection's start and end as bytes of `doc`: the document the
    /// textarea now stands for (after the browser's own edit, the text it
    /// left, not yet the document's).
    pub(crate) fn selection_in(&self, doc: &str) -> (TextSize, TextSize) {
        let (start, end) = self.selection_units();
        (Self::byte_in(doc, start), Self::byte_in(doc, end))
    }

    /// The selection's start and end as UTF-16 offsets of the document,
    /// for the browser's own measures of it (a copy of half a character).
    pub(crate) fn selection_utf16(&self) -> (Utf16, Utf16) {
        self.selection_units()
    }

    pub(crate) fn selection_direction(&self) -> Result<Option<String>, JsValue> {
        self.element.selection_direction()
    }

    /// Selects bytes `start..end` of the document (the caret at `end`),
    /// first opening the folds that hide either end: whatever moves the
    /// caret shows where it went.
    pub(crate) fn set_selection_range(
        &self,
        start: TextSize,
        end: TextSize,
    ) -> Result<(), JsValue> {
        self.set_selection_range_with_direction(start, end, "forward")
    }

    /// Selects bytes `start..end` of the document without opening any fold
    /// (a selection put back as it was, such as after a refused edit; an
    /// end inside a fold goes to its edge).
    pub(crate) fn restore_selection(&self, start: TextSize, end: TextSize) -> Result<(), JsValue> {
        let moved = self
            .element
            .set_selection_range(self.view_offset(start), self.view_offset(end));
        self.follow_caret();
        moved
    }

    /// [`set_selection_range`](Self::set_selection_range) with a direction:
    /// the caret at `start` when it is `"backward"`.
    pub(crate) fn set_selection_range_with_direction(
        &self,
        start: TextSize,
        end: TextSize,
        direction: &str,
    ) -> Result<(), JsValue> {
        self.state.reveal_offsets(start, end);
        let moved = self.element.set_selection_range_with_direction(
            self.view_offset(start),
            self.view_offset(end),
            direction,
        );
        self.follow_caret();
        moved
    }

    /// The caret: the selection's end that moves (its start when the
    /// selection is backward), in bytes of the document.
    pub(crate) fn caret(&self) -> TextSize {
        directed_selection(self).1
    }

    /// Records the textarea's caret as the editor's (the active line, the
    /// caret's path). The one place the caret is noted from the textarea:
    /// every selection the editor sets passes here, and so does every
    /// selection the browser makes (a key, a click, its own edit).
    pub(crate) fn follow_caret(&self) {
        self.state.note_caret(self.caret());
    }

    /// Focuses the text area without scrolling the page (see [`crate::focus`]).
    pub(crate) fn focus_in_place(&self) -> Result<(), JsValue> {
        crate::focus::in_place(&self.element)
    }

    pub(crate) fn scroll_top(&self) -> i32 {
        self.element.scroll_top()
    }

    /// Scrolls the text to `top`, and the overlays drawn over it with it
    /// (the highlight, the underlines, the popups): the one place the
    /// text's scroll is set, so they never wait for its scroll event.
    pub(crate) fn set_scroll_top(&self, top: i32) {
        self.element.set_scroll_top(top);
        self.mirror_scroll();
    }

    /// Copies the text's scroll for the overlays, as the browser left it
    /// (it keeps the scroll inside the text).
    pub(crate) fn mirror_scroll(&self) {
        let view = self.state.view();
        let (left, top) = (
            f64::from(self.element.scroll_left()),
            f64::from(self.element.scroll_top()),
        );
        if view.scroll_x.get_untracked() != left {
            view.scroll_x.set(left);
        }
        if view.scroll_y.get_untracked() != top {
            view.scroll_y.set(top);
        }
    }

    pub(crate) fn scroll_left(&self) -> i32 {
        self.element.scroll_left()
    }

    /// Scrolls the text sideways to `left`, the overlays with it (see
    /// [`Self::set_scroll_top`]).
    pub(crate) fn set_scroll_left(&self, left: i32) {
        self.element.set_scroll_left(left);
        self.mirror_scroll();
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
            .view()
            .projection
            .with_untracked(|projection| projection.row_of_line(line))
    }

    /// The document line (0-based) on textarea row `row`.
    pub(crate) fn line_of_row(&self, row: usize) -> usize {
        self.state
            .view()
            .projection
            .with_untracked(|projection| projection.line_of_row(row))
    }
}

impl EditorState {
    /// Puts the document, folds cut out, in the textarea. Returns whether
    /// its text changed.
    pub(super) fn show_text(&self) -> bool {
        let projection = self.with_closed_folds_untracked(|folds| {
            self.doc()
                .text
                .with_untracked(|doc| Projection::new(doc, folds))
        });
        let rewritten = self
            .view()
            .textarea_ref
            .get_untracked()
            .is_some_and(|element| {
                self.doc().text.with_untracked(|doc| {
                    let view = projection.view(doc);
                    let shown = element.value();
                    let rewrite = shown != view;
                    if rewrite {
                        replace_changed(&element, &shown, view);
                    }
                    rewrite
                })
            });
        if self
            .view()
            .projection
            .with_untracked(|shown| **shown != projection)
        {
            self.view().projection.set(Arc::new(projection));
        }
        rewritten
    }
}

/// Writes `view` into `element`, which shows `shown`: only the stretch that
/// differs is replaced (two lines' text for a line moved), so the browser
/// lays out what changed rather than the whole document.
fn replace_changed(element: &web_sys::HtmlTextAreaElement, shown: &str, view: &str) {
    use super::text_change::{changed_bytes, utf16_len};
    let (start, old_end, new_end) = changed_bytes(shown, view);
    // The browser counts the textarea's text in UTF-16 units.
    fn cut(text: &str, from: usize, to: usize) -> &str {
        slice::slice(text, TextRange::new(text_size(from), text_size(to))).unwrap_or_default()
    }
    let from = Utf16::count(utf16_len(cut(shown, 0, start)));
    let to = from + Utf16::count(utf16_len(cut(shown, start, old_end)));
    if element
        .set_range_text_with_start_and_end(cut(view, start, new_end), from.into(), to.into())
        .is_err()
    {
        element.set_value(view);
    }
}
