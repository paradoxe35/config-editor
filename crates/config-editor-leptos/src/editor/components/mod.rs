//! The editor's view, one component per part of the screen. Each takes the
//! shared [`EditorState`](super::state::EditorState).

mod baseline_notice;
mod chrome;
mod code;
mod completions;
mod diagnostics;
mod diff_peek;
mod find_widget;
mod folds;
mod go_to_line;
mod gutter;
mod hover_card;
mod protect_tip;
mod search_highlights;
mod source;
mod status_bar;
mod toolbar;

pub(super) use baseline_notice::BaselineNotice;
pub use chrome::{ConfigEditorFooter, ConfigEditorToolbar};
pub(super) use code::CodeLayer;
pub(super) use completions::CompletionList;
pub(super) use diagnostics::DiagnosticOverlay;
pub(super) use diff_peek::DiffPeekPopup;
pub(super) use find_widget::FindWidget;
pub(super) use folds::mount_fold_layers;
pub(super) use go_to_line::GoToLineWidget;
pub(super) use gutter::Gutter;
pub(super) use hover_card::{HoverCard, HoverCardView};
pub(super) use protect_tip::ProtectTipView;
pub(super) use search_highlights::SearchHighlights;
pub(super) use source::SourceInput;
pub(super) use status_bar::StatusBar;
pub(super) use toolbar::Toolbar;

use leptos::prelude::*;

use super::line_rows::{LineRows, RowFolds};
use super::state::EditorState;

/// Per-row signals for the gutter and code layer, kept in step with the
/// document, diagnostics, diff, caret line and visible range (see
/// `line_rows`).
pub(super) fn row_states(state: EditorState) -> LineRows {
    let rows = LineRows::new();
    let synced = rows.clone();
    Effect::new(move |_| {
        let line = state.doc.current_line.get();
        state.fold.visible_lines.with(|visible| {
            state.fold.folded.with(|closed| {
                let folds = RowFolds { closed };
                state.view.lines.with(|lines| {
                    state
                        .changes
                        .diff
                        .with(|diff| synced.sync(lines, diff, line, visible, folds))
                })
            })
        });
    });
    rows
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
                    state.view.visible_rows.with(|rows| rows.start),
                )
            }
        ></span>
    }
}
