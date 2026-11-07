use super::diff::{DiffKind, DiffPeek, LineDiff, PeekKind};
use leptos::prelude::*;

/// One line of the document plus its error and diff flags.
#[derive(Clone, PartialEq)]
pub(super) struct RenderedLine {
    pub(super) text: String,
    pub(super) has_error: bool,
    pub(super) change: DiffKind,
}

/// Everything one gutter/highlight row displays, comparable so unchanged rows
/// are not notified.
#[derive(Clone, PartialEq)]
pub(super) struct RowUiState {
    pub(super) text: String,
    pub(super) has_error: bool,
    pub(super) change: DiffKind,
    pub(super) is_active: bool,
    pub(super) modified_peek: Option<DiffPeek>,
    pub(super) added_peek: Option<DiffPeek>,
    pub(super) deleted_peek: Option<DiffPeek>,
    pub(super) deleted_after_peek: Option<DiffPeek>,
}

pub(super) fn row_state(
    index: usize,
    rows: &[RenderedLine],
    line_count: usize,
    line_diff: &LineDiff,
    active_line: usize,
) -> RowUiState {
    let line = index + 1;
    let row = rows.get(index);
    let text = row.map(|row| row.text.clone()).unwrap_or_default();
    let change = row.map(|row| row.change).unwrap_or(DiffKind::Unchanged);
    RowUiState {
        has_error: row.is_some_and(|row| row.has_error),
        change,
        is_active: line == active_line,
        modified_peek: line_diff
            .modified_before
            .get(index)
            .and_then(Option::as_ref)
            .map(|before| DiffPeek {
                line,
                kind: PeekKind::Modified,
                title: "Modified line".to_string(),
                before: vec![before.clone()],
                after: vec![text.clone()],
            }),
        added_peek: (change == DiffKind::Added).then(|| DiffPeek {
            line,
            kind: PeekKind::Added,
            title: "Added line".to_string(),
            before: Vec::new(),
            after: vec![text.clone()],
        }),
        deleted_peek: line_diff
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
        deleted_after_peek: (line == line_count && line_diff.deleted_after).then(|| DiffPeek {
            line,
            kind: PeekKind::RemovedAtEnd,
            title: "Removed lines at end of document".to_string(),
            before: line_diff.deleted_lines_after.clone(),
            after: Vec::new(),
        }),
        text,
    }
}

/// Per-row display state for the `For`-keyed gutter and highlight rows.
///
/// Each row owns one signal and [`LineRows::sync`] writes only the signals
/// whose state changed. Rows must not subscribe to the document-wide memos:
/// with thousands of rows, every keystroke would re-run every row's closures
/// and churn subscriptions, which is quadratic.
///
/// Signals are positional (row `i` shows line `i + 1`) and the list only
/// grows; unused tail entries are kept for reuse.
#[derive(Clone)]
pub(super) struct LineRows {
    states: RwSignal<Vec<RwSignal<RowUiState>>>,
    /// Component owner; row signals live here because `For` disposes
    /// row-local owners on rebuild.
    owner: Owner,
}

impl LineRows {
    pub(super) fn new() -> Self {
        Self {
            states: RwSignal::new(Vec::new()),
            owner: Owner::current().expect("LineRows::new needs a reactive owner"),
        }
    }

    /// Returns the signal for row `index`, creating any missing rows.
    pub(super) fn state_for(
        &self,
        index: usize,
        rows: &[RenderedLine],
        line_diff: &LineDiff,
        active_line: usize,
    ) -> RwSignal<RowUiState> {
        if let Some(signal) = self
            .states
            .with_untracked(|states| states.get(index).copied())
        {
            return signal;
        }
        // One signal per skipped slot; sharing one would alias rows.
        let mut seeded = None;
        self.owner.with(|| {
            self.states.update(|states| {
                while states.len() <= index {
                    let missing = states.len();
                    let signal =
                        RwSignal::new(row_state(missing, rows, rows.len(), line_diff, active_line));
                    if missing == index {
                        seeded = Some(signal);
                    }
                    states.push(signal);
                }
            });
        });
        seeded.expect("sync just pushed a signal for `index`")
    }

    /// Writes the row signals in `visible` whose state changed. Rows outside
    /// it are not rendered; they are brought up to date when they scroll in.
    ///
    /// Compare before `set`: `set` always notifies, which would re-render
    /// every row.
    pub(super) fn sync(
        &self,
        rows: &[RenderedLine],
        line_diff: &LineDiff,
        active_line: usize,
        visible: std::ops::Range<usize>,
    ) {
        let visible = visible.start.min(rows.len())..visible.end.min(rows.len());
        // Extend in a separate step: calling `set` while an `update` guard is
        // held traps in the reactive runtime.
        self.owner.with(|| {
            self.states.update(|states| {
                while states.len() < visible.end {
                    let index = states.len();
                    states.push(RwSignal::new(row_state(
                        index,
                        rows,
                        rows.len(),
                        line_diff,
                        active_line,
                    )));
                }
            });
        });
        let signals: Vec<RwSignal<RowUiState>> = self
            .states
            .with_untracked(|states| states[visible.clone()].to_vec());
        for (signal, index) in signals.iter().zip(visible) {
            let next = row_state(index, rows, rows.len(), line_diff, active_line);
            if signal.with_untracked(|current| current != &next) {
                signal.set(next);
            }
        }
    }
}
