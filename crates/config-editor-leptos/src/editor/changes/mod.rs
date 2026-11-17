//! Changes against the baseline: the gutter's line diff and its markers,
//! the before/after preview they open, counting the changes for the host
//! and the footer, reverting one, moving between them, and a unified diff
//! for review. The `diff` feature: without it, `off/changes.rs` stands in
//! with the same hooks (the baseline still decides the dirty flag).
//!
//! - `line_diff`: the line diff and the previews' geometry;
//! - `rows`: what each gutter row shows of it;
//! - `commands`, `keys`: revert, move between changes;
//! - `unified`: the unified diff; `components`: the views.
//!
//! The hunks come from the gutter's own line diff, so what is counted,
//! reverted and walked is exactly what the markers show.

mod commands;
mod components;
mod keys;
mod line_diff;
mod rows;
mod unified;

use crate::editor::text::{TextSize, byte_range, text_size};
pub(super) use commands::{ChangesState, report_changes};
pub(super) use components::{
    ChangeMarkers, ChangeStatus, DiffPeekPopup, tab_around_preview, tab_into_preview,
};
pub(super) use keys::go_to_change;
pub(super) use line_diff::LineDiff;
#[cfg(feature = "folding")]
pub(super) use rows::hides_change;
pub(super) use rows::{RowPeeks, line_kind, row_peeks, with_line_diff};
pub use unified::unified_diff;

use leptos::prelude::*;

use super::change_summary::ChangeSummary;
use super::edit::Change;
use super::live_settings::on_switch_off;
use super::state::EditorState;
#[cfg(test)]
#[allow(
    unused_imports,
    reason = "used by the hostile random suite, which needs every feature"
)]
pub(super) use line_diff::middle_edits;
use line_diff::{DiffPeek, PeekKind, document_lines};
pub(super) use line_diff::{Hunk, diff_lines};

/// Switching the `show_diff` setting off closes the preview.
pub(super) fn follow_settings(state: EditorState) {
    on_switch_off(state.config().settings.diff, move || {
        state.changes().peek.set(None)
    });
}

/// Closes the change preview.
pub(super) fn close_preview(state: EditorState) {
    state.changes().peek.set(None);
}

/// Whether the change preview is open.
pub(super) fn preview_open(state: EditorState) -> bool {
    state.changes().peek.with_untracked(Option::is_some)
}

impl ChangeSummary {
    /// The changes from `baseline` to `current` (line endings ignored).
    /// Needs the `diff` feature.
    pub fn between(baseline: &str, current: &str) -> Self {
        Self::of(&diff_lines(baseline, current))
    }

    pub(super) fn of(line_diff: &LineDiff) -> Self {
        line_diff
            .hunks
            .iter()
            .fold(Self::default(), |summary, hunk| Self {
                added: summary.added + hunk.added,
                removed: summary.removed + hunk.removed,
                modified: summary.modified + hunk.modified,
                hunks: summary.hunks + 1,
            })
    }
}

/// The change a preview shows: the hunk holding its line, or, for removed
/// lines, the hunk they were removed by.
pub(super) fn hunk_for_peek<'a>(line_diff: &'a LineDiff, peek: &DiffPeek) -> Option<&'a Hunk> {
    let index = peek.line.saturating_sub(1);
    line_diff.hunks.iter().find(|hunk| match peek.kind {
        PeekKind::Modified | PeekKind::Added => hunk.current.contains(&index),
        PeekKind::Removed => hunk.removed > 0 && hunk.current.end == index,
        PeekKind::RemovedAtEnd => hunk.removed > 0 && hunk.current.end == line_diff.current.len(),
    })
}

/// `text` with `hunk` reverted, and the UTF-16 caret at the start of the
/// restored lines (see [`reverting`]), for the tests.
#[cfg(test)]
pub(super) fn revert_hunk(text: &str, hunk: &Hunk) -> (String, usize) {
    let (change, caret) = reverting(text, hunk);
    let reverted = change.apply(text).expect("made on the text");
    let caret = reverted[..usize::from(caret)].encode_utf16().count();
    (reverted, caret)
}

/// The change reverting `hunk` in `text`: its lines (and the line breaks
/// between them and the rest) replaced by the baseline's, and the caret at
/// the start of the restored lines (a byte offset of the text after; the
/// text's end when none are restored there).
pub(super) fn reverting(text: &str, hunk: &Hunk) -> (Change, TextSize) {
    let lines = document_lines(text);
    let end = hunk.current.end.min(lines.len());
    let start = hunk.current.start.min(end);
    // Where each line starts.
    let line_start =
        |line: usize| -> usize { lines[..line].iter().map(|text| text.len() + 1).sum() };
    let baseline = &hunk.baseline;
    let (range, written, caret) = if end < lines.len() {
        // Lines before a line kept: each with the line break after it.
        let at = line_start(start);
        let written: String = baseline.iter().map(|line| format!("{line}\n")).collect();
        (at..line_start(end), written, at)
    } else if start > 0 {
        // Down to the end: each after the line break before it.
        let at = line_start(start) - 1;
        let written: String = baseline.iter().map(|line| format!("\n{line}")).collect();
        let caret = if baseline.is_empty() { at } else { at + 1 };
        (at..text.len(), written, caret)
    } else {
        (0..text.len(), baseline.join("\n"), 0)
    };
    (
        Change::replace(text, byte_range(range), &written),
        text_size(caret),
    )
}

/// The zero-based line a change starts on, where moving to it puts the
/// caret: its first line, or the line after removed lines (the last line for
/// lines removed at the end).
pub(super) fn change_line(hunk: &Hunk, line_count: usize) -> usize {
    if hunk.current.is_empty() {
        hunk.current.start.min(line_count.saturating_sub(1))
    } else {
        hunk.current.start
    }
}

/// The change after (or before) zero-based `line`, wrapping around the
/// document as F8 does for problems.
pub(super) fn adjacent_change(
    line_diff: &LineDiff,
    line_count: usize,
    line: usize,
    forward: bool,
) -> Option<&Hunk> {
    let hunks = &line_diff.hunks;
    let starts = |hunk: &&Hunk| change_line(hunk, line_count);
    if forward {
        hunks
            .iter()
            .find(|hunk| starts(hunk) > line)
            .or_else(|| hunks.first())
    } else {
        hunks
            .iter()
            .rev()
            .find(|hunk| starts(hunk) < line)
            .or_else(|| hunks.last())
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod unified_tests;
