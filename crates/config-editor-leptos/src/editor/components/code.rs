//! The highlighted copy of the text drawn under the transparent textarea.

use leptos::prelude::*;

use super::super::diff::DiffKind;
use super::super::highlight::{self, TokenKind, Tokens};
use super::super::line_rows::{LineRows, RowFolds};
use super::super::state::EditorState;
use super::RowSpacer;

#[component]
pub(crate) fn CodeLayer(state: EditorState, rows: LineRows) -> impl IntoView {
    view! {
        <pre
            class="config-editor__highlight"
            aria-hidden="true"
            style:transform=move || format!("translate({}px, {}px)", -state.view.scroll_x.get(), -state.view.scroll_y.get())
        >
            <code>
                <RowSpacer state=state />
                <For
                    each=move || state.fold.visible_lines.get()
                    key=|index| *index
                    children=move |index| view! { <CodeLine state=state rows=rows.clone() index=index /> }
                />
            </code>
        </pre>
    }
}

#[component]
fn CodeLine(state: EditorState, rows: LineRows, index: usize) -> impl IntoView {
    // Subscribe only to the row's own signal (see `line_rows`).
    let active = state.doc.current_line.get_untracked();
    // Borrow, never clone: cloning the whole document per new row made large
    // pastes quadratic.
    let row = state.view.lines.with_untracked(|lines| {
        state
            .changes
            .diff
            .with_untracked(|diff| rows.state_for(index, lines, diff, active, RowFolds::default()))
    });
    let highlighting = state.config.features.highlighting;
    // A line inside a multi-line string's body is all string.
    let in_string_body = move || {
        state
            .view
            .string_bodies
            .with(|bodies| bodies.iter().any(|lines| lines.contains(&index)))
    };
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
                let (text, marks) = row.with(|row| {
                    (row.text.clone(), row.marks.as_ref().map_or_else(Vec::new, |marks| marks.classes()))
                });
                let tokens = if highlighting && in_string_body() {
                    let mut tokens = Tokens::new();
                    tokens.push(0, text.len(), TokenKind::String);
                    tokens.finish()
                } else if highlighting {
                    state.sync.edit_language.get().behavior().highlight(&text)
                } else {
                    Vec::new()
                };
                highlight::render(&text, &tokens, &marks)
            }}
        </span>
    }
}
