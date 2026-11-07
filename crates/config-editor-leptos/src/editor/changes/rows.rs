//! What each gutter row shows of the changes: its line's kind and the
//! previews its markers open.

use leptos::prelude::*;

use super::super::diff::DiffKind;
use super::super::state::EditorState;
use super::line_diff::{DiffPeek, LineDiff, PeekKind};

/// The previews a row's markers open, one per kind of marker it shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::editor) struct RowPeeks {
    pub(in crate::editor) modified: Option<DiffPeek>,
    pub(in crate::editor) added: Option<DiffPeek>,
    pub(in crate::editor) deleted: Option<DiffPeek>,
    pub(in crate::editor) deleted_after: Option<DiffPeek>,
}

/// Calls `f` with the line diff (tracked).
pub(in crate::editor) fn with_line_diff<R>(
    state: EditorState,
    f: impl FnOnce(&LineDiff) -> R,
) -> R {
    state.changes.diff.with(f)
}

/// How line `index` (0-based) differs from the baseline.
pub(in crate::editor) fn line_kind(line_diff: &LineDiff, index: usize) -> DiffKind {
    line_diff
        .current
        .get(index)
        .copied()
        .unwrap_or(DiffKind::Unchanged)
}

/// The previews of row `index`, whose text is `text` and kind `change`, in a
/// document of `line_count` lines.
pub(in crate::editor) fn row_peeks(
    line_diff: &LineDiff,
    index: usize,
    line_count: usize,
    text: &str,
    change: DiffKind,
) -> RowPeeks {
    let line = index + 1;
    RowPeeks {
        modified: line_diff
            .modified_before
            .get(index)
            .and_then(Option::as_ref)
            .map(|before| DiffPeek {
                line,
                kind: PeekKind::Modified,
                title: "Modified line".to_string(),
                before: vec![before.clone()],
                after: vec![text.to_string()],
            }),
        added: (change == DiffKind::Added).then(|| DiffPeek {
            line,
            kind: PeekKind::Added,
            title: "Added line".to_string(),
            before: Vec::new(),
            after: vec![text.to_string()],
        }),
        deleted: line_diff
            .deleted_before
            .get(index)
            .copied()
            .unwrap_or(false)
            .then(|| DiffPeek {
                line,
                kind: PeekKind::Removed,
                title: "Removed lines".to_string(),
                before: line_diff
                    .deleted_lines_before
                    .get(index)
                    .cloned()
                    .unwrap_or_default(),
                after: Vec::new(),
            }),
        deleted_after: (line == line_count && line_diff.deleted_after).then(|| DiffPeek {
            line,
            kind: PeekKind::RemovedAtEnd,
            title: "Removed lines at end of document".to_string(),
            before: line_diff.deleted_lines_after.clone(),
            after: Vec::new(),
        }),
    }
}

/// Whether any of the 0-based `lines` (those a closed fold hides) changed,
/// or lost lines before it. Tracked. (Folding marks such folds.)
#[cfg(feature = "folding")]
pub(in crate::editor) fn hides_change(
    state: EditorState,
    lines: std::ops::RangeInclusive<usize>,
) -> bool {
    state.changes.diff.with(|diff| {
        lines.into_iter().any(|index| {
            line_kind(diff, index) != DiffKind::Unchanged
                || diff.deleted_before.get(index).copied().unwrap_or(false)
        })
    })
}
