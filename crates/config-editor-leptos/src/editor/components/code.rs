//! The highlighted copy of the text drawn under the transparent textarea.

use leptos::prelude::*;

use super::super::diff::DiffKind;
use super::super::highlight;
use super::super::line_rows::LineRows;
use super::super::state::EditorState;
use super::RowSpacer;

#[component]
pub(crate) fn CodeLayer(state: EditorState, rows: LineRows) -> impl IntoView {
    view! {
        <pre
            class="config-editor__highlight"
            aria-hidden="true"
            style:transform=move || format!("translate({}px, {}px)", -state.scroll_x.get(), -state.scroll_y.get())
        >
            <code>
                <RowSpacer state=state />
                <For
                    each=move || state.visible_rows.get()
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
    let active = state.current_line.get_untracked();
    // Borrow, never clone: cloning the whole document per new row made large
    // pastes quadratic.
    let row = state.lines.with_untracked(|lines| {
        state
            .diff
            .with_untracked(|diff| rows.state_for(index, lines, diff, active))
    });
    let highlighting = state.features.highlighting;
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
                let text = row.with(|row| row.text.clone());
                if highlighting {
                    highlight::render(&text, &state.edit_language.get().behavior().highlight(&text))
                } else {
                    view! { <span>{text}</span> }.into_any()
                }
            }}
        </span>
    }
}
