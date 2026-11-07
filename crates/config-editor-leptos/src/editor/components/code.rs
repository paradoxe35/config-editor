//! The highlighted copy of the text drawn under the transparent textarea.

use leptos::prelude::*;

use super::super::diff::DiffKind;
use super::super::highlight::{self, TokenKind, Tokens};
use super::super::line_rows::{LineRows, RowHighlight};
use super::super::state::EditorState;
use super::{RowSpacer, row_state_for};
use crate::reactive::KeyedList;

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
                <KeyedList
                    each=move || state.view.visible_lines.get()
                    key=|index| *index
                    children=move |index| view! { <CodeLine state=state rows=rows index=index /> }
                />
            </code>
        </pre>
    }
}

/// One line of the highlighted text. It reads only its row's own state
/// (see `line_rows`).
#[component]
fn CodeLine(state: EditorState, rows: LineRows, index: usize) -> impl IntoView {
    let row = row_state_for(state, rows, index);
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
                let (text, marks, highlight) = row.with(|row| {
                    (
                        row.text.clone(),
                        row.marks.as_ref().map_or_else(Vec::new, |marks| marks.classes()),
                        row.highlight,
                    )
                });
                let tokens = match highlight {
                    RowHighlight::Plain => Vec::new(),
                    RowHighlight::StringBody => {
                        let mut tokens = Tokens::new();
                        tokens.push(0, text.len(), TokenKind::String);
                        tokens.finish()
                    }
                    RowHighlight::Language(language) => language.behavior().highlight(&text),
                };
                highlight::render(&text, &tokens, &marks)
            }}
        </span>
    }
}
