//! The document and its caret: the text on screen, where the caret is, the
//! undo history, and the indent unit. Edits reach the text only through
//! the edit pipeline (see `edit`).

use leptos::prelude::*;

use super::commands::{indent_unit, line_at_cursor, offset_at};
use super::history::EditHistory;
use super::languages::Language;
use super::paths::{FieldPath, KeptSpans, path_at};
use super::positioning::{reveal_offset, set_caret_visible};
use super::state::EditorState;

/// The document part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct DocState {
    /// The text on screen, in the edit language, with every line (the
    /// textarea leaves out folded ones, see `folding`).
    pub(super) text: RwSignal<String>,
    /// The caret's UTF-16 offset in `text`.
    pub(super) caret: RwSignal<usize>,
    /// The caret's line, 1-based.
    pub(super) current_line: RwSignal<usize>,
    pub(super) history: RwSignal<EditHistory>,
    /// The document's indent unit, detected when it is loaded, replaced or
    /// pasted in (not after every edit, so Tab then Shift+Tab round-trips).
    pub(super) indent: StoredValue<String>,
    /// The path of what the caret is on (see `paths::path_at`). Lazy: only
    /// computed while something reads it (`on_path_change`, a handle).
    pub(super) caret_path: Memo<FieldPath>,
    /// The spans of every field of the text, read for the handle's
    /// `range_of` and kept while the text stays the same: each lookup
    /// alone read the whole document again.
    pub(super) spans: StoredValue<KeptSpans, LocalStorage>,
}

impl DocState {
    /// The document `shown` (in `edit_language`), the caret at its start.
    pub(super) fn new(shown: &str, edit_language: RwSignal<Language>) -> Self {
        let text = RwSignal::new(shown.to_string());
        let caret = RwSignal::new(0usize);
        let caret_path = Memo::new(move |_| {
            let behavior = edit_language.get().behavior();
            let at = caret.get();
            text.with(|text| path_at(behavior, text, at).unwrap_or_default())
        });
        Self {
            text,
            caret,
            current_line: RwSignal::new(1),
            history: RwSignal::new(EditHistory::default()),
            indent: StoredValue::new(indent_unit(shown)),
            caret_path,
            spans: StoredValue::new_local(KeptSpans::default()),
        }
    }
}

impl EditorState {
    /// Detects the indent unit again, for a document loaded or replaced.
    pub(super) fn redetect_indent(&self) {
        let unit = self.doc().text.with_untracked(|text| indent_unit(text));
        self.doc().indent.set_value(unit);
    }

    /// Records where the caret is, without touching the textarea.
    pub(super) fn note_caret(&self, offset: usize) {
        self.doc().caret.set(offset);
        self.doc().current_line.set(
            self.doc()
                .text
                .with_untracked(|text| line_at_cursor(text, offset)),
        );
    }

    /// Puts the caret at `offset` in the textarea, in view, and records it.
    pub(super) fn place_caret(&self, offset: usize) {
        if let Some(element) = self.textarea() {
            let _ = element.set_selection_range(offset as u32, offset as u32);
            self.doc().text.with_untracked(|text| {
                set_caret_visible(&element, text, offset, self.view().scroll_y)
            });
        }
        self.note_caret(offset);
    }

    /// Focuses the editor with the caret at `line`:`column`, scrolled into
    /// view, closing popups.
    pub(super) fn focus_at(&self, line: usize, column: usize) -> Option<usize> {
        let offset = self
            .doc()
            .text
            .with_untracked(|text| offset_at(text, line, column));
        self.focus_range(offset, offset)?;
        Some(offset)
    }

    /// Focuses the editor with UTF-16 `start..end` selected, its start
    /// scrolled into view, closing popups.
    pub(super) fn focus_range(&self, start: usize, end: usize) -> Option<()> {
        let element = self.textarea()?;
        // Focusing scrolls back to the old caret; keep the view where the
        // user clicked (a line number, an underline), then reveal if needed.
        let (top, left) = (element.scroll_top(), element.scroll_left());
        let _ = element.focus_in_place();
        let _ = element.set_selection_range(start as u32, end as u32);
        element.set_scroll_top(top);
        element.set_scroll_left(left);
        self.doc()
            .text
            .with_untracked(|text| reveal_offset(&element, text, start, self.view().scroll_y));
        self.note_caret(start);
        self.close_suggestions();
        self.hide_hover();
        Some(())
    }
}
