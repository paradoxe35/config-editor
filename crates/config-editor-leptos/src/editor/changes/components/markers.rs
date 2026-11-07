//! A line number's change markers; a marker opens the change preview.

use leptos::prelude::*;

use super::super::super::line_rows::RowUiState;
use super::super::super::state::EditorState;
use super::super::line_diff::DiffPeek;

/// The markers of line `line`, from its row's state.
#[component]
pub(in crate::editor) fn ChangeMarkers(
    state: EditorState,
    row: RwSignal<RowUiState>,
    line: usize,
) -> impl IntoView {
    view! {
        {move || {
            row.with(|row| row.peeks.added.clone()).map(|peek| view! {
                <PeekTrigger
                    state=state
                    peek=peek
                    class="config-editor__diff-marker config-editor__diff-marker-added config-editor__diff-change-trigger"
                    label=format!("Show added line {line}")
                    title="Show added line"
                />
            })
        }}
        {move || {
            row.with(|row| row.peeks.modified.clone()).map(|peek| view! {
                <PeekTrigger
                    state=state
                    peek=peek
                    class="config-editor__diff-marker config-editor__diff-marker-modified config-editor__diff-change-trigger"
                    label=format!("Show before and after for modified line {line}")
                    title="Show before and after"
                />
            })
        }}
        {move || {
            row.with(|row| row.peeks.deleted.clone()).map(|peek| view! {
                <PeekTrigger
                    state=state
                    peek=peek
                    class="config-editor__diff-deleted"
                    label=format!("Show removed lines before line {line}")
                    title="Show removed lines"
                />
            })
        }}
        {move || {
            row.with(|row| row.peeks.deleted_after.clone()).map(|peek| view! {
                <PeekTrigger
                    state=state
                    peek=peek
                    class="config-editor__diff-deleted config-editor__diff-deleted-after"
                    label="Show removed lines at end of document".to_string()
                    title="Show removed lines"
                />
            })
        }}
    }
}

/// A gutter marker that toggles the change preview for `peek`.
#[component]
fn PeekTrigger(
    state: EditorState,
    peek: DiffPeek,
    class: &'static str,
    label: String,
    title: &'static str,
) -> impl IntoView {
    let toggle = move |_| {
        state.changes.peek.update(|selected| {
            *selected = if selected.as_ref() == Some(&peek) {
                None
            } else {
                Some(peek.clone())
            };
        });
    };
    let keydown = move |event: web_sys::KeyboardEvent| {
        if event.key() == "Escape" {
            state.changes.peek.set(None);
        }
    };
    view! {
        <button
            type="button"
            class=class
            aria-label=label
            title=title
            on:click=toggle
            on:keydown=keydown
        ></button>
    }
}
