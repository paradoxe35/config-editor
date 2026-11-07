//! The editor's view, one component per part of the screen. Each takes the
//! shared [`EditorState`](super::state::EditorState).

mod chrome;
mod code;
mod completions;
mod diagnostics;
mod diff_peek;
mod find_widget;
mod gutter;
mod hover_card;
mod search_highlights;
mod source;
mod status_bar;
mod toolbar;

pub use chrome::{ConfigEditorFooter, ConfigEditorToolbar};
pub(super) use code::CodeLayer;
pub(super) use completions::CompletionList;
pub(super) use diagnostics::DiagnosticOverlay;
pub(super) use diff_peek::DiffPeekPopup;
pub(super) use find_widget::FindWidget;
pub(super) use gutter::Gutter;
pub(super) use hover_card::{HoverCard, HoverCardView};
pub(super) use search_highlights::SearchHighlights;
pub(super) use source::SourceInput;
pub(super) use status_bar::StatusBar;
pub(super) use toolbar::Toolbar;

use leptos::prelude::*;

use super::line_rows::LineRows;
use super::state::EditorState;

/// Per-row signals for the gutter and code layer, kept in step with the
/// document, diagnostics, diff, caret line and visible range (see
/// `line_rows`).
pub(super) fn row_states(state: EditorState) -> LineRows {
    let rows = LineRows::new();
    let synced = rows.clone();
    Effect::new(move |_| {
        let line = state.current_line.get();
        let range = state.visible_rows.get();
        state.lines.with(|lines| {
            state
                .diff
                .with(|diff| synced.sync(lines, diff, line, range))
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
                    state.visible_rows.with(|rows| rows.start),
                )
            }
        ></span>
    }
}
