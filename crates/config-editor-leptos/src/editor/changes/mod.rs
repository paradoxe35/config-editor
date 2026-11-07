//! Changes against the baseline as units: counting them for the host,
//! reverting one, moving between them, and a unified diff for review.
//!
//! The hunks come from the gutter's own line diff (`diff`), so what is
//! counted, reverted and walked is exactly what the markers show.

mod commands;
mod unified;

pub(super) use commands::report_changes;
pub use unified::unified_diff;

use super::commands::offset_at;
use super::diff::{DiffPeek, Hunk, LineDiff, PeekKind, diff_lines, document_lines};

/// How the current text differs from the baseline, by lines, as the diff
/// gutter shows it.
///
/// ```
/// use config_editor_leptos::ChangeSummary;
///
/// let summary = ChangeSummary::between("a: 1\nb: 2\nc: 3\n", "a: 1\nb: 20\nc: 3\nd: 4\n");
/// assert_eq!((summary.added, summary.modified, summary.removed), (1, 1, 0));
/// assert_eq!(summary.hunks, 2);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ChangeSummary {
    /// Lines added.
    pub added: usize,
    /// Lines removed.
    pub removed: usize,
    /// Lines changed in place.
    pub modified: usize,
    /// Separate changes (runs of changed lines), each one a gutter marker
    /// group that "Revert this change" restores at once.
    pub hunks: usize,
}

impl ChangeSummary {
    /// The changes from `baseline` to `current` (line endings ignored).
    pub fn between(baseline: &str, current: &str) -> Self {
        Self::of(&diff_lines(baseline, current))
    }

    /// Whether the texts have the same lines.
    pub fn is_empty(&self) -> bool {
        self.hunks == 0
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
/// restored lines.
pub(super) fn revert_hunk(text: &str, hunk: &Hunk) -> (String, usize) {
    let mut lines = document_lines(text);
    let end = hunk.current.end.min(lines.len());
    let start = hunk.current.start.min(end);
    lines.splice(start..end, hunk.baseline.iter().map(String::as_str));
    let reverted = lines.join("\n");
    let caret = offset_at(&reverted, start + 1, 1);
    (reverted, caret)
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
