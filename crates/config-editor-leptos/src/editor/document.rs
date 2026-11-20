//! The document and its caret: the text on screen, where the caret is, the
//! undo history, and the indent unit. Edits reach the text only through
//! the edit pipeline (see `edit`).

use leptos::prelude::*;

use super::commands::{indent_unit, offset_at};
use super::history::EditHistory;
use super::keyboard::utf16_to_byte_index;
use super::languages::Language;
use super::paths::{FieldPath, KeptSpans, path_at};
use super::positioning::{reveal_offset, set_caret_visible};
use super::readings::Readings;
use super::state::EditorState;
#[cfg(test)]
use super::text::byte_of;
use super::text::{TextSize, slice, text_size};

/// The document part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct DocState {
    /// The text on screen, in the edit language, with every line (the
    /// textarea leaves out folded ones, see `folding`).
    pub(super) text: RwSignal<String>,
    /// The caret's byte offset in `text`.
    pub(super) caret: RwSignal<TextSize>,
    /// The caret's line, 1-based.
    pub(super) current_line: RwSignal<usize>,
    pub(super) history: RwSignal<EditHistory>,
    /// Which text `text` is: moved on by every write (the edit pipeline's
    /// `write_text`), so "the same document" is a number compared, not a
    /// text (an edit and its undo give the same text, a new version).
    pub(super) version: StoredValue<u64>,
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
    /// The readings of the whole text the next frame can show late (see
    /// `readings`).
    pub(super) readings: Readings,
}

impl DocState {
    /// The document `shown` (in `edit_language`), the caret at its start.
    pub(super) fn new(shown: &str, edit_language: RwSignal<Language>) -> Self {
        let text = RwSignal::new(shown.to_string());
        let caret = RwSignal::new(TextSize::default());
        let caret_path = Memo::new(move |_| {
            let behavior = edit_language.get().behavior();
            let at = caret.get();
            text.with(|text| {
                // The paths are read at UTF-16 offsets (the public language
                // API's).
                let at = usize::from(super::paths::units_at(text, at));
                path_at(behavior, text, at).unwrap_or_default()
            })
        });
        Self {
            text,
            caret,
            current_line: RwSignal::new(1),
            history: RwSignal::new(EditHistory::default()),
            version: StoredValue::new(0),
            indent: StoredValue::new(indent_unit(shown)),
            caret_path,
            spans: StoredValue::new_local(KeptSpans::default()),
            readings: Readings::new(),
        }
    }
}

impl EditorState {
    /// Detects the indent unit again, for a document loaded or replaced.
    pub(super) fn redetect_indent(&self) {
        let unit = self.doc().text.with_untracked(|text| indent_unit(text));
        self.doc().indent.set_value(unit);
    }

    /// Records where the caret is, without touching the textarea: the only
    /// writer of `caret` and `current_line`. With a textarea, the caret is
    /// its selection's moving end, noted through `SourceArea` whenever the
    /// selection is set or read back (see `SourceArea::follow_caret`).
    pub(super) fn note_caret(&self, offset: TextSize) {
        self.doc().caret.set(offset);
        self.doc()
            .current_line
            .set(self.doc().text.with_untracked(|text| {
                let before = slice::before(text, offset.min(TextSize::of(text.as_str())));
                before
                    .unwrap_or(text)
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1
            }));
    }

    /// Puts the caret at byte `offset` in the textarea, in view, and
    /// records it.
    pub(super) fn place_caret(&self, offset: TextSize) {
        match self.textarea() {
            Some(element) => self
                .doc()
                .text
                .with_untracked(|text| set_caret_visible(&element, text, offset)),
            None => self.note_caret(offset),
        }
    }

    /// The caret as a UTF-16 offset of the text, as the tests read it.
    #[cfg(test)]
    #[cfg_attr(
        not(all(feature = "yaml", feature = "json")),
        allow(dead_code, reason = "only the tests over several languages use it")
    )]
    pub(super) fn caret_units(&self) -> usize {
        let caret = self.doc().caret.get_untracked();
        self.doc()
            .text
            .with_untracked(|text| super::text::utf16_of(text, caret))
    }

    /// Records the caret at UTF-16 `offset` of the text, as the tests set
    /// it.
    #[cfg(test)]
    #[cfg_attr(
        not(all(feature = "yaml", feature = "json")),
        allow(dead_code, reason = "only the tests over several languages use it")
    )]
    pub(super) fn set_caret_units(&self, offset: usize) {
        let offset = self.doc().text.with_untracked(|text| byte_of(text, offset));
        self.doc().caret.set(offset);
    }

    /// Focuses the editor with the caret at `line`:`column`, scrolled into
    /// view, closing popups.
    pub(super) fn focus_at(&self, line: usize, column: usize) -> Option<TextSize> {
        let offset = self.doc().text.with_untracked(|text| {
            text_size(utf16_to_byte_index(text, offset_at(text, line, column)))
        });
        self.focus_range(offset, offset)?;
        Some(offset)
    }

    /// Focuses the editor with bytes `start..end` selected, its start
    /// scrolled into view, closing popups.
    pub(super) fn focus_range(&self, start: TextSize, end: TextSize) -> Option<()> {
        self.focus_range_directed(start, end, false)
    }

    /// [`focus_range`](Self::focus_range), the caret at `start` when
    /// `backward`.
    pub(super) fn focus_range_directed(
        &self,
        start: TextSize,
        end: TextSize,
        backward: bool,
    ) -> Option<()> {
        let element = self.textarea()?;
        // Focusing scrolls back to the old caret; keep the view where the
        // user clicked (a line number, an underline), then reveal if needed.
        let (top, left) = (element.scroll_top(), element.scroll_left());
        let _ = element.focus_in_place();
        let direction = if backward { "backward" } else { "forward" };
        let _ = element.set_selection_range_with_direction(start, end, direction);
        element.set_scroll_top(top);
        element.set_scroll_left(left);
        self.doc()
            .text
            .with_untracked(|text| reveal_offset(&element, text, start));
        self.close_suggestions();
        self.hide_hover();
        Some(())
    }
}
