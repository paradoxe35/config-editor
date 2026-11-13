//! Changes against the baseline, compiled out (the `diff` feature is off):
//! the hooks of `changes/` with nothing behind them. No gutter markers,
//! previews or change count; `Alt+F5` and `Alt+F3` stay the browser's and
//! the `show_diff` setting does nothing. The baseline still decides the
//! dirty flag and the baseline-conflict notice.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::change_summary::ChangeSummary;
use super::diff::DiffKind;
use super::line_rows::RowUiState;
use super::state::EditorState;

/// No line diff.
pub(super) struct LineDiff;

/// No previews.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct RowPeeks;

pub(super) fn with_line_diff<R>(_state: EditorState, f: impl FnOnce(&LineDiff) -> R) -> R {
    f(&LineDiff)
}

pub(super) fn line_kind(_line_diff: &LineDiff, _index: usize) -> DiffKind {
    DiffKind::Unchanged
}

pub(super) fn row_peeks(
    _line_diff: &LineDiff,
    _index: usize,
    _line_count: usize,
    _text: &str,
    _change: DiffKind,
) -> RowPeeks {
    RowPeeks
}

#[cfg(feature = "folding")]
pub(super) fn hides_change(_state: EditorState, _lines: std::ops::RangeInclusive<usize>) -> bool {
    false
}

/// `on_change_summary` without the feature: said once, never called.
pub(super) fn report_changes(_state: EditorState, _callback: Callback<ChangeSummary>) {
    web_sys::console::warn_1(
        &"config-editor: `on_change_summary` needs the `diff` feature; it is never called".into(),
    );
}

pub(super) fn go_to_change(_state: EditorState, _event: &KeyboardEvent) -> bool {
    false
}

pub(super) fn follow_settings(_state: EditorState) {}

pub(super) fn close_preview(_state: EditorState) {}

pub(super) fn preview_open(_state: EditorState) -> bool {
    false
}

pub(super) fn tab_around_preview(_state: EditorState, _event: &KeyboardEvent) -> bool {
    false
}

pub(super) fn tab_into_preview(_state: EditorState) -> bool {
    false
}

#[component]
pub(super) fn ChangeMarkers(
    state: EditorState,
    row: RwSignal<RowUiState>,
    line: usize,
) -> impl IntoView {
    let _ = (state, row, line);
}

#[component]
pub(super) fn ChangeStatus(state: EditorState) -> impl IntoView {
    let _ = state;
}

#[component]
pub(super) fn DiffPeekPopup(state: EditorState) -> impl IntoView {
    let _ = state;
}
