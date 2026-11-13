//! The highlighted copy of the text drawn under the transparent textarea.

use leptos::either::Either;
use leptos::prelude::*;

use super::super::diff::DiffKind;
use super::super::highlight;
use super::super::highlight::pieces::{self, Piece, RowPieces};
use super::super::line_rows::{LineRows, RowUiState};
use super::super::state::EditorState;
use super::row_state_for;
use crate::reactive::{KeyedList, MANY_ROWS, RowSpacer};

#[component]
pub(crate) fn CodeLayer(state: EditorState, rows: LineRows) -> impl IntoView {
    view! {
        <pre
            class="config-editor__highlight"
            aria-hidden="true"
            style:transform=move || format!("translate({}px, {}px)", -state.view().scroll_x.get(), -state.view().scroll_y.get())
        >
            <code>
                <RowSpacer
                    rows=move || state.view().visible_rows.with(|rows| rows.start)
                    row_height=|| "1lh"
                />
                <KeyedList
                    each=move || state.view().visible_lines.get()
                    key=|index| *index
                    reserve=MANY_ROWS
                    children=move |index| view! { <CodeLine state=state rows=rows index=index /> }
                />
            </code>
        </pre>
    }
}

/// One line of the highlighted text. It reads only its row's own state
/// (see `line_rows`). A long row is drawn in pieces (see
/// `highlight::pieces`), and a short one whole.
#[component]
fn CodeLine(state: EditorState, rows: LineRows, index: usize) -> impl IntoView {
    let row = row_state_for(state, rows, index);
    let pieces = row_pieces(row);
    let split = Memo::new(move |_| pieces.with(Option::is_some));
    view! {
        <span
            class="config-editor__code-line"
            data-line=index + 1
            class:config-editor__code-line-active=move || row.with(|row| row.is_active)
            class:config-editor__code-line-error=move || row.with(|row| row.has_error)
            class:config-editor__code-line-added=move || row.with(|row| row.change == DiffKind::Added)
            class:config-editor__code-line-modified=move || row.with(|row| row.change == DiffKind::Modified)
        >
            {move || {
                if split.get() {
                    Either::Left(view! { <RowPieceViews pieces=pieces /> })
                } else {
                    Either::Right(move || {
                        let (text, marks, highlight) = row.with(|row| {
                            (row.text.clone(), row_marks(row), row.highlight)
                        });
                        let tokens = highlight.tokens(&text);
                        highlight::render(&text, &tokens, &marks)
                    })
                }
            }}
        </span>
    }
}

/// The pieces of a long row, kept from one state of the row to the next;
/// `None` for a row drawn whole.
fn row_pieces(row: RwSignal<RowUiState>) -> Memo<Option<RowPieces>> {
    Memo::new_owning(move |previous: Option<Option<RowPieces>>| {
        let previous = previous.flatten();
        row.with(|row| {
            if !pieces::splits(&row.text) {
                return (None, previous.is_some());
            }
            let runs = highlight::runs_of(&row.text, &row.highlight.tokens(&row.text));
            let (next, changed) = RowPieces::next(previous, &row.text, &runs, &row_marks(row));
            (Some(next), changed)
        })
    })
}

fn row_marks(row: &RowUiState) -> Vec<highlight::Mark> {
    row.marks
        .as_ref()
        .map_or_else(Vec::new, |marks| marks.classes())
}

/// The pieces of a long row, each an element of its own kept while it
/// draws the same.
#[component]
fn RowPieceViews(pieces: Memo<Option<RowPieces>>) -> impl IntoView {
    view! {
        <KeyedList
            each=move || pieces.with(|pieces| pieces.as_ref().map(|pieces| pieces.pieces().to_vec()).unwrap_or_default())
            key=|piece: &Piece| piece.id
            children=|piece: Piece| {
                let content = piece.content;
                view! {
                    <span class="config-editor__code-piece">
                        {highlight::render_runs(&content.text, &content.runs, &content.marks)}
                    </span>
                }
            }
        />
    }
}
