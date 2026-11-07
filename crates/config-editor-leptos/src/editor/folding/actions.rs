//! Folding and unfolding, and the folds through edits: moved with the
//! lines when the edit pipeline writes the text (see `edit`), opened for
//! an edit that reaches into them, closed again when it is refused.

use leptos::prelude::*;

use super::super::commands::line_at_cursor;
use super::super::fold_ranges::FoldRange;
use super::super::metrics::read_editor_metrics;
use super::super::positioning::reveal_offset;
use super::super::state::{EditorState, directed_selection};
use super::follow::{follow_edit, keep_valid};

impl EditorState {
    /// The closed folds moved through the change of the document to `new`
    /// (see [`follow_edit`]); `None` with nothing folded. The edit pipeline
    /// asks before it writes the text and settles them after.
    pub(crate) fn folds_through(&self, new: &str) -> Option<Vec<FoldRange>> {
        if self.fold.folded.with_untracked(Vec::is_empty) {
            return None;
        }
        Some(self.fold.folded.with_untracked(|folds| {
            self.doc
                .text
                .with_untracked(|old| follow_edit(folds, old, new))
        }))
    }

    /// Keeps the folds `moved` through an edit that still fold in the new
    /// text, and shows it. Returns whether the textarea's text had to be
    /// rewritten (its selection is then lost).
    pub(crate) fn settle_folds(&self, moved: Option<Vec<FoldRange>>) -> bool {
        if let Some(moved) = moved {
            let kept = self
                .fold
                .fold_ranges
                .with_untracked(|ranges| keep_valid(&moved, ranges));
            if self.fold.folded.with_untracked(|folds| *folds != kept) {
                self.fold.folded.set(kept);
            }
        }
        self.show_text()
    }

    /// Whether an edit of UTF-16 `start..end` touches a closed fold (see
    /// [`unfold_touched`](Self::unfold_touched)).
    pub(crate) fn touches_fold(&self, start: usize, end: usize) -> bool {
        !self.fold.folded.with_untracked(Vec::is_empty)
            && self.fold.folded.with_untracked(|folds| {
                self.doc.text.with_untracked(|doc| {
                    hidden_spans(doc, folds)
                        .iter()
                        .any(|(from, to)| start <= *to && end >= *from)
                })
            })
    }

    /// Closes exactly `folds`, keeping the caret's text and the first line
    /// in view where they are; a caret the folds hide moves to the end of
    /// its fold's first line.
    pub(crate) fn set_folds(&self, mut folds: Vec<FoldRange>) {
        folds.sort_unstable();
        folds.dedup();
        if self.fold.folded.with_untracked(|current| *current == folds) {
            return;
        }
        let area = self.textarea();
        let before = area.as_ref().map(|area| {
            let selection = directed_selection(area);
            let metrics = read_editor_metrics(area.element());
            let line_height = metrics.line_height.max(1.0);
            let top = f64::from(area.scroll_top());
            let row = ((top - metrics.padding_top) / line_height).floor().max(0.0) as usize;
            // Whether the caret's row is in view, to keep it there.
            let caret_row = self.view.projection.with_untracked(|projection| {
                projection.row_of_line(self.caret_line_of(selection.1))
            }) as f64;
            let caret_top = metrics.padding_top + caret_row * line_height;
            let caret_shown = caret_top >= top
                && caret_top + line_height <= top + f64::from(area.client_height());
            (
                selection,
                area.line_of_row(row),
                row,
                line_height,
                top,
                caret_shown,
            )
        });
        self.fold.folded.set(folds);
        self.show_text();
        let (Some(area), Some(((anchor, caret), top_line, top_row, line_height, top, caret_shown))) =
            (area, before)
        else {
            return;
        };
        let (anchor, caret) = self.view.projection.with_untracked(|projection| {
            let shown = |offset: usize| projection.hidden_at(offset).map_or(offset, |(at, _)| at);
            (shown(anchor), shown(caret))
        });
        let (view_anchor, view_caret) = self
            .view
            .projection
            .with_untracked(|projection| (projection.to_view(anchor), projection.to_view(caret)));
        let element = area.element();
        let (start, end, direction) = if view_caret < view_anchor {
            (view_caret, view_anchor, "backward")
        } else {
            (view_anchor, view_caret, "forward")
        };
        let _ = element.set_selection_range_with_direction(start as u32, end as u32, direction);
        // The line that was at the top stays there.
        let new_row = area.row_of_line(top_line);
        let scroll = (top + (new_row as f64 - top_row as f64) * line_height).max(0.0);
        element.set_scroll_top(scroll as i32);
        self.view.scroll_y.set(f64::from(element.scroll_top()));
        // A caret that was in view stays in view.
        if caret_shown {
            self.doc
                .text
                .with_untracked(|text| reveal_offset(&area, text, caret, self.view.scroll_y));
        }
        self.note_caret(caret);
    }

    /// The 0-based line of UTF-16 `offset`.
    fn caret_line_of(&self, offset: usize) -> usize {
        self.doc
            .text
            .with_untracked(|text| line_at_cursor(text, offset).saturating_sub(1))
    }

    /// Opens the folds hiding UTF-16 `start` or `end`, so a selection moved
    /// there shows.
    pub(crate) fn reveal_offsets(&self, start: usize, end: usize) {
        let hidden = self.view.projection.with_untracked(|projection| {
            projection.hidden_at(start).is_some() || projection.hidden_at(end).is_some()
        });
        if !hidden {
            return;
        }
        let kept = self.fold.folded.with_untracked(|folds| {
            self.doc.text.with_untracked(|doc| {
                let spans = hidden_spans(doc, folds);
                folds
                    .iter()
                    .zip(spans)
                    .filter(|(_, (from, to))| {
                        !((*from < start && start <= *to) || (*from < end && end <= *to))
                    })
                    .map(|(fold, _)| *fold)
                    .collect::<Vec<_>>()
            })
        });
        self.fold.folded.set(kept);
        self.show_text();
    }

    /// Opens the folds an edit about to be made of UTF-16 `start..end`
    /// would touch (see [`unfold_touched`](Self::unfold_touched)), keeping
    /// the folds as they were: a refused edit closes them again (see
    /// `before_edit`).
    pub(crate) fn open_folds_for_edit(&self, start: usize, end: usize) {
        if self.fold.folded.with_untracked(Vec::is_empty) {
            return;
        }
        let folded = self.fold.folded.get_untracked();
        self.unfold_touched(start, end);
        // Kept until the edit is made: a refused one puts the folds back.
        let opened = self.fold.folded.with_untracked(|now| *now != folded);
        self.fold.before_edit.set_value(opened.then_some(folded));
    }

    /// The folds as they were before an editing key opened some for the
    /// edit now being made (see [`open_folds_for_edit`](Self::open_folds_for_edit)).
    pub(crate) fn take_opened_folds(&self) -> Option<Vec<FoldRange>> {
        self.fold
            .before_edit
            .try_update_value(Option::take)
            .flatten()
    }

    /// Closes `folds` again (those before a refused edit opened some); the
    /// caller shows the text.
    pub(crate) fn restore_folds(&self, folds: Vec<FoldRange>) {
        self.fold.folded.set(folds);
    }

    /// Opens every fold without moving the view (the text changes to
    /// another language's lines, which they do not fit).
    pub(crate) fn forget_folds(&self) {
        if !self.fold.folded.with_untracked(Vec::is_empty) {
            self.fold.folded.set(Vec::new());
        }
    }

    /// Opens the folds an edit of UTF-16 `start..end` touches: from a fold's
    /// first line end through its last line end, both included.
    pub(crate) fn unfold_touched(&self, start: usize, end: usize) {
        if self.fold.folded.with_untracked(Vec::is_empty) {
            return;
        }
        let kept = self.fold.folded.with_untracked(|folds| {
            self.doc.text.with_untracked(|doc| {
                folds
                    .iter()
                    .zip(hidden_spans(doc, folds))
                    .filter(|(_, (from, to))| !(start <= *to && end >= *from))
                    .map(|(fold, _)| *fold)
                    .collect::<Vec<_>>()
            })
        });
        if self
            .fold
            .folded
            .with_untracked(|folds| folds.len() != kept.len())
        {
            let selection = self.textarea().map(|area| directed_selection(&area));
            self.fold.folded.set(kept);
            self.show_text();
            if let (Some(area), Some((anchor, caret))) = (self.textarea(), selection) {
                let direction = if caret < anchor {
                    "backward"
                } else {
                    "forward"
                };
                let _ = area.set_selection_range_with_direction(
                    anchor.min(caret) as u32,
                    anchor.max(caret) as u32,
                    direction,
                );
            }
        }
    }

    /// The caret's line (0-based).
    fn caret_line(&self) -> usize {
        self.doc.current_line.get_untracked().saturating_sub(1)
    }

    /// Folds the innermost open range around the caret's line; again, the
    /// one around that (`Ctrl+Shift+[`).
    pub(crate) fn fold_at_caret(&self) {
        self.fold_line(self.caret_line());
    }

    /// Folds the innermost open range starting at or around `line`
    /// (0-based). Returns whether one was found.
    pub(crate) fn fold_line(&self, line: usize) -> bool {
        let range = self.fold.fold_ranges.with_untracked(|ranges| {
            self.fold.folded.with_untracked(|folded| {
                ranges
                    .iter()
                    .filter(|range| range.contains(line) && !folded.contains(range))
                    .max_by_key(|range| range.start)
                    .copied()
            })
        });
        let Some(range) = range else {
            return false;
        };
        let mut folds = self.fold.folded.get_untracked();
        folds.push(range);
        self.set_folds(folds);
        true
    }

    /// Folds the range starting at `line` (0-based). Returns whether there
    /// is one, open.
    pub(crate) fn fold_starting_at(&self, line: usize) -> bool {
        let range = self.fold.fold_ranges.with_untracked(|ranges| {
            ranges
                .binary_search_by_key(&line, |range| range.start)
                .ok()
                .map(|index| ranges[index])
        });
        match range {
            Some(range)
                if !self
                    .fold
                    .folded
                    .with_untracked(|folded| folded.contains(&range)) =>
            {
                let mut folds = self.fold.folded.get_untracked();
                folds.push(range);
                self.set_folds(folds);
                true
            }
            _ => false,
        }
    }

    /// Opens the folds around the caret's line (`Ctrl+Shift+]`).
    pub(crate) fn unfold_at_caret(&self) {
        self.unfold_line(self.caret_line());
    }

    /// Opens the folds starting at or around `line` (0-based). Returns
    /// whether one was closed.
    pub(crate) fn unfold_line(&self, line: usize) -> bool {
        let mut folds = self.fold.folded.get_untracked();
        let count = folds.len();
        folds.retain(|fold| !fold.contains(line));
        if folds.len() == count {
            return false;
        }
        self.set_folds(folds);
        true
    }

    /// Folds or opens the range starting at `line` (a gutter chevron).
    pub(crate) fn toggle_fold(&self, line: usize) {
        let mut folds = self.fold.folded.get_untracked();
        if let Some(index) = folds.iter().position(|fold| fold.start == line) {
            folds.remove(index);
        } else if let Some(range) = self.fold.fold_ranges.with_untracked(|ranges| {
            ranges
                .binary_search_by_key(&line, |range| range.start)
                .ok()
                .map(|index| ranges[index])
        }) {
            folds.push(range);
        } else {
            return;
        }
        self.set_folds(folds);
    }

    /// Folds every range (`Ctrl+K Ctrl+0`).
    pub(crate) fn fold_all(&self) {
        let all = self.fold.fold_ranges.get_untracked();
        self.set_folds(all);
    }

    /// Opens every fold (`Ctrl+K Ctrl+J`).
    pub(crate) fn unfold_all(&self) {
        self.set_folds(Vec::new());
    }
}

/// Each fold's hidden UTF-16 range in `doc`: from its first line's end
/// through its last line's end.
pub(crate) fn hidden_spans(doc: &str, folds: &[FoldRange]) -> Vec<(usize, usize)> {
    let mut ends = Vec::new();
    let mut units = 0;
    for character in doc.chars() {
        if character == '\n' {
            ends.push(units);
        }
        units += character.len_utf16();
    }
    ends.push(units);
    let end_of = |line: usize| ends[line.min(ends.len() - 1)];
    folds
        .iter()
        .map(|fold| (end_of(fold.start), end_of(fold.end)))
        .collect()
}
