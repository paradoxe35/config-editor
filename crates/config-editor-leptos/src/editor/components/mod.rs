//! The editor's view, one component per part of the screen. Each takes the
//! shared [`EditorState`](super::state::EditorState).

mod announcer;
mod baseline_notice;
mod chrome;
mod code;
mod completions;
mod diagnostics;
mod edit_as_notice;
mod gutter;
mod hover_card;
mod problem_summary;
mod read_only_notice;
mod source;
mod status_bar;
mod toolbar;

pub(super) use announcer::Announcer;
pub(super) use baseline_notice::BaselineNotice;
pub use chrome::{ConfigEditorFooter, ConfigEditorToolbar};
pub(super) use code::CodeLayer;
pub(super) use completions::CompletionList;
pub(super) use diagnostics::{DiagnosticOverlay, ProblemTip};
pub(super) use edit_as_notice::EditAsNotice;
pub(super) use gutter::Gutter;
pub(super) use hover_card::{HoverCard, HoverCardView};
pub(super) use read_only_notice::ReadOnlyNotice;
pub(super) use source::SourceInput;
pub(super) use status_bar::StatusBar;
pub(super) use toolbar::Toolbar;

use leptos::prelude::*;

use super::changes::with_line_diff;
use super::line_rows::{Highlighting, LineRows, RowFolds, RowInputs};
use super::state::EditorState;

/// Per-row state for the gutter and code layer, kept in step with the
/// document, diagnostics, diff, caret line, folds, highlighting and visible
/// range (see `line_rows`).
pub(super) fn row_states(state: EditorState) -> LineRows {
    let rows = LineRows::new();
    Effect::new(move |_| {
        state
            .view()
            .visible_lines
            .with(|visible| with_row_inputs(state, |inputs| rows.sync(inputs, visible)));
    });
    rows
}

/// Calls `f` with what the rows' states are built from (tracked).
pub(super) fn with_row_inputs<R>(state: EditorState, f: impl FnOnce(&RowInputs<'_>) -> R) -> R {
    let active_line = state.doc().current_line.get();
    let language = state
        .config()
        .settings
        .highlighting
        .get()
        .then(|| state.sync().edit_language.get());
    state.with_closed_folds(|closed| {
        state.view().string_bodies.with(|string_bodies| {
            state.view().spanning.with(|spanning| {
                state.view().lines.with(|rows| {
                    with_line_diff(state, |line_diff| {
                        f(&RowInputs {
                            rows,
                            line_diff,
                            active_line,
                            folds: RowFolds { closed },
                            highlighting: Highlighting {
                                language,
                                string_bodies,
                                spanning,
                            },
                        })
                    })
                })
            })
        })
    })
}

/// The state signal of row `index`, owned by the calling row.
pub(super) fn row_state_for(
    state: EditorState,
    rows: LineRows,
    index: usize,
) -> RwSignal<super::line_rows::RowUiState> {
    // Untracked: a row subscribes only to its own signal (see `line_rows`).
    untrack(|| with_row_inputs(state, |inputs| rows.state_for(index, inputs)))
}

/// The spacer above the rendered rows, keeping their positions.
#[component]
fn RowSpacer(state: EditorState) -> impl IntoView {
    view! {
        <span
            class="config-editor__row-spacer"
            style:height=move || {
                format!(
                    "calc({} * var(--config-editor-line-height))",
                    state.view().visible_rows.with(|rows| rows.start),
                )
            }
        ></span>
    }
}
