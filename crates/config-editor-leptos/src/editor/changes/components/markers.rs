//! A line number's change markers; a marker opens the change preview.

use leptos::prelude::*;

use super::super::super::line_rows::RowUiState;
use super::super::super::state::EditorState;
use super::super::line_diff::DiffPeek;
use crate::EditorStrings;
use crate::strings::fill;

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
                    label=|strings| &strings.show_added
                    title=|strings| &strings.show_added_title
                    line=line
                />
            })
        }}
        {move || {
            row.with(|row| row.peeks.modified.clone()).map(|peek| view! {
                <PeekTrigger
                    state=state
                    peek=peek
                    class="config-editor__diff-marker config-editor__diff-marker-modified config-editor__diff-change-trigger"
                    label=|strings| &strings.show_modified
                    title=|strings| &strings.show_modified_title
                    line=line
                />
            })
        }}
        {move || {
            row.with(|row| row.peeks.deleted.clone()).map(|peek| view! {
                <PeekTrigger
                    state=state
                    peek=peek
                    class="config-editor__diff-deleted"
                    label=|strings| &strings.show_removed
                    title=|strings| &strings.show_removed_title
                    line=line
                />
            })
        }}
        {move || {
            row.with(|row| row.peeks.deleted_after.clone()).map(|peek| view! {
                <PeekTrigger
                    state=state
                    peek=peek
                    class="config-editor__diff-deleted config-editor__diff-deleted-after"
                    label=|strings| &strings.show_removed_at_end
                    title=|strings| &strings.show_removed_title
                    line=line
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
    /// Its accessible name, a text naming the `{line}`.
    label: fn(&EditorStrings) -> &String,
    title: fn(&EditorStrings) -> &String,
    line: usize,
) -> impl IntoView {
    let strings = state.strings_signal();
    let label = move || strings.with(|strings| fill(label(strings), &[("line", &line)]));
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
            title=state.text(title)
            on:click=toggle
            on:keydown=keydown
        ></button>
    }
}
