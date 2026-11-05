use super::diff::{DiffKind, DiffPeek, LineDiff};
use leptos::prelude::*;

/// One rendered line: its text plus the flags the gutter, highlight, and
/// diagnostic marker rows display. Computed once per document change by the
/// `lines` memo in `editor.rs`; [`LineRows::sync`] fans it out to per-row
/// state below.
#[derive(Clone, PartialEq)]
pub(super) struct RenderedLine {
    pub(super) text: String,
    pub(super) has_error: bool,
    pub(super) change: DiffKind,
}

/// Everything a single gutter/highlight row displays, as one comparable
/// value so [`LineRows::sync`] can skip notifying rows whose visible state
/// did not change.
#[derive(Clone, PartialEq)]
pub(super) struct RowUiState {
    pub(super) text: String,
    pub(super) has_error: bool,
    pub(super) change: DiffKind,
    pub(super) is_active: bool,
    pub(super) modified_peek: Option<DiffPeek>,
    pub(super) deleted_peek: Option<DiffPeek>,
    pub(super) deleted_after_peek: Option<DiffPeek>,
}

/// Builds the display state for one row from the shared snapshot data.
/// Used both to seed newly created rows and by [`LineRows::sync`].
pub(super) fn row_state(
    index: usize,
    rows: &[RenderedLine],
    line_count: usize,
    line_diff: &LineDiff,
    active_line: usize,
) -> RowUiState {
    let line = index + 1;
    let row = rows.get(index);
    RowUiState {
        text: row.map(|row| row.text.clone()).unwrap_or_default(),
        has_error: row.is_some_and(|row| row.has_error),
        change: row.map(|row| row.change).unwrap_or(DiffKind::Unchanged),
        is_active: line == active_line,
        modified_peek: line_diff
            .modified_before
            .get(index)
            .and_then(Option::as_ref)
            .map(|before| DiffPeek {
                line,
                title: "Modified line".to_string(),
                before: vec![before.clone()],
                after: vec![row.map(|row| row.text.clone()).unwrap_or_default()],
            }),
        deleted_peek: line_diff
            .deleted_before
            .get(index)
            .copied()
            .unwrap_or(false)
            .then(|| DiffPeek {
                line,
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
            title: "Removed lines at end of document".to_string(),
            before: line_diff.deleted_lines_after.clone(),
            after: Vec::new(),
        }),
    }
}

/// Per-row display state for the `For`-keyed gutter and highlight rows.
///
/// Each row owns one signal holding its [`RowUiState`]. [`LineRows::sync`]
/// rewrites the signals whose visible state actually changed, so a keystroke
/// re-renders exactly those rows. Rows never subscribe to the document-wide
/// `lines`/`diff` memos: with thousands of rows, every keystroke would
/// otherwise re-run every row's closures, and each re-run unsubscribes and
/// re-subscribes through linear subscriber sets — quadratic work per
/// keystroke that makes large documents feel stuck.
///
/// Signals are positional: row `i` always shows line `i + 1`. Insertions and
/// deletions shift positions, so a middle-of-document split rewrites the
/// whole tail — one guarded write per shifted row, still a single pass with
/// no subscription churn. The container only grows; entries past the current
/// line count linger until reused, bounded by the longest document seen.
#[derive(Clone)]
pub(super) struct LineRows {
    states: RwSignal<Vec<RwSignal<RowUiState>>>,
    /// Owner pinned at creation (the component owner). Row signals are
    /// created under this owner so `For` row churn — which disposes the
    /// row-local owner on every rebuild — can never dispose them.
    owner: Owner,
}

impl LineRows {
    pub(super) fn new() -> Self {
        Self {
            states: RwSignal::new(Vec::new()),
            owner: Owner::current().expect("LineRows::new needs a reactive owner"),
        }
    }

    /// Returns the state signal for `index`, creating it seeded from the
    /// current snapshot when the document grew past all previous rows.
    /// Called from `For` row constructors, which may build rows in any
    /// order; extension is idempotent and single-threaded.
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
        // Seed each missing slot from the snapshot individually: row
        // constructors normally arrive in order, but sharing one signal across
        // skipped indices would alias several rows to the same state.
        // Signals are created under the pinned component owner, never the
        // row-local one: `For` disposes row owners on rebuild, which would
        // otherwise dispose the signals stored here.
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

    /// Rewrites the row signals whose visible state changed. Only notifying
    /// changed rows keeps every other row's closures — and subscriptions —
    /// quiet. The comparison must happen *before* touching the signal:
    /// `set` notifies subscribers unconditionally, which would re-render
    /// every row on every keystroke and defeat this type's whole point.
    pub(super) fn sync(&self, rows: &[RenderedLine], line_diff: &LineDiff, active_line: usize) {
        // Extend first, on its own: `set` below notifies row subscribers
        // synchronously, and notifying any signal while another signal's
        // `update` guard is held traps in the reactive runtime. Keep every
        // `set` outside of any guard.
        self.owner.with(|| {
            self.states.update(|states| {
                while states.len() < rows.len() {
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
        // Copy the handles out under a short read; compare and notify with
        // no guard held.
        let signals: Vec<RwSignal<RowUiState>> = self
            .states
            .with_untracked(|states| states[..rows.len()].to_vec());
        for (index, signal) in signals.iter().enumerate() {
            let next = row_state(index, rows, rows.len(), line_diff, active_line);
            if signal.with_untracked(|current| current != &next) {
                signal.set(next);
            }
        }
    }
}
