//! The change commands on the live editor: revert one, move to the next or
//! previous, and report the count to the host.

use leptos::prelude::*;

use super::super::diff::DiffPeek;
use super::super::line_rows::row_state;
use super::super::state::{Edit, EditorState, textarea_selection};
use super::{ChangeSummary, adjacent_change, change_line, hunk_for_peek, revert_hunk};

impl EditorState {
    /// Restores the baseline lines of the change `peek` shows, as one edit:
    /// one undo step, reported through `on_change` (and converted back while
    /// editing as another language) like typing. Closes the preview and
    /// returns to the text.
    pub(in crate::editor) fn revert_change(&self, peek: &DiffPeek) {
        if self.read_only.get_untracked() {
            return;
        }
        let Some(hunk) = self
            .diff
            .with_untracked(|line_diff| hunk_for_peek(line_diff, peek).cloned())
        else {
            return;
        };
        let before = self.source.get_untracked();
        let (text, caret) = revert_hunk(&before, &hunk);
        let element = self.textarea();
        let selection = element.as_ref().map_or_else(
            || {
                let at = self.cursor.get_untracked();
                (at, at)
            },
            textarea_selection,
        );
        self.selected_diff_peek.set(None);
        self.close_suggestions();
        self.hide_hover();
        self.forget_typing_aids();
        if let Some(element) = element {
            // Back from the preview to the text, without the focus scrolling
            // to the old caret; the commit reveals the restored lines.
            let (top, left) = (element.scroll_top(), element.scroll_left());
            let _ = element.focus();
            element.set_scroll_top(top);
            element.set_scroll_left(left);
        }
        self.commit(Edit::new(before, selection, text, caret));
    }

    /// Moves the caret to the start of the next (or previous) change,
    /// wrapping around; with `preview`, opens that change's preview too,
    /// focused so `Enter` or `R` reverts it.
    pub(in crate::editor) fn go_to_change(&self, forward: bool, preview: bool) {
        let line_count = self.lines.with_untracked(Vec::len);
        let here = self.current_line.get_untracked().saturating_sub(1);
        let Some((line, peek)) = self.diff.with_untracked(|line_diff| {
            let hunk = adjacent_change(line_diff, line_count, here, forward)?;
            let line = change_line(hunk, line_count);
            let row = self
                .lines
                .with_untracked(|lines| row_state(line, lines, line_count, line_diff, 0));
            let peek = if !hunk.current.is_empty() {
                row.modified_peek.or(row.added_peek)
            } else if hunk.current.start >= line_diff.current.len() {
                row.deleted_after_peek
            } else {
                row.deleted_peek
            };
            Some((line, peek))
        }) else {
            return;
        };
        self.focus_at(line + 1, 1);
        if preview {
            self.focus_preview
                .set_value(!self.read_only.get_untracked());
            self.selected_diff_peek.set(peek);
        }
    }
}

/// Calls the host's `on_change_summary` with the count of changes, once
/// now and again each time it changes. It reads the diff the gutter already
/// keeps, so it costs no rendering.
pub(in crate::editor) fn report_changes(state: EditorState, callback: Callback<ChangeSummary>) {
    Effect::new(move |_| callback.run(state.changes.get()));
}
