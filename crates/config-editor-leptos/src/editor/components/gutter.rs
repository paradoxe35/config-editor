//! Line numbers with error and diff markers; a modified or deleted marker
//! opens the change preview.

use leptos::prelude::*;

use super::super::commands::line_range;
use super::super::diff::{DiffKind, DiffPeek};
use super::super::line_rows::{LineRows, RowUiState};
use super::super::state::{EditorState, line_count};
use super::{RowSpacer, row_state_for};
use crate::EditorStrings;
use crate::reactive::KeyedList;
use crate::strings::fill;

#[component]
pub(crate) fn Gutter(state: EditorState, rows: LineRows) -> impl IntoView {
    view! {
        <div
            class="config-editor__gutter"
            class:config-editor__gutter-folding=state.config.settings.folding
            // As wide as the longest line number (at least three digits);
            // the stylesheet adds the marker column and the padding.
            style:--config-editor-gutter-digits=move || {
                state.doc.text.with(|text| line_count(text)).to_string().len().max(3).to_string()
            }
        >
            <div
                class="config-editor__gutter-content"
                style:transform=move || format!("translateY(-{}px)", state.view.scroll_y.get())
            >
                <RowSpacer state=state />
                <KeyedList
                    each=move || state.fold.visible_lines.get()
                    key=|index| *index
                    children=move |index| view! { <LineNumber state=state rows=rows index=index /> }
                />
            </div>
        </div>
    }
}

#[component]
fn LineNumber(state: EditorState, rows: LineRows, index: usize) -> impl IntoView {
    let line = index + 1;
    let row = row_state_for(state, rows, index);
    let select_line = move |_| {
        // A line number selects its whole line, as in VS Code.
        let Some(offset) = state.focus_at(line, 1) else {
            return;
        };
        let (_, line_end) = state
            .doc
            .text
            .with_untracked(|text| line_range(text, offset));
        if let Some(element) = state.textarea() {
            let _ = element.set_selection_range(offset as u32, line_end as u32);
        }
    };
    view! {
        <div
            data-line=line
            // One binding for every state class: rows are many, and each
            // binding is a reactive node of its own.
            class=move || row.with(line_number_class)
            on:click=select_line
        >
            <span aria-hidden="true">{line}</span>
            {move || {
                // A lock, else a key beside a secret placeholder: one node.
                row.with(|row| {
                    row.marks.as_ref().and_then(|marks| match (&marks.lock, &marks.key) {
                        (Some(lock), _) => Some(("config-editor__lock-marker", lock.clone())),
                        (None, Some(key)) => Some(("config-editor__secret-marker", key.clone())),
                        (None, None) => None,
                    })
                })
                .map(|(class, label)| view! {
                    <span class=class role="img" aria-label=label.clone() title=label></span>
                })
            }}
            {move || {
                row.with(|row| row.added_peek.clone()).map(|peek| view! {
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
                row.with(|row| row.modified_peek.clone()).map(|peek| view! {
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
                row.with(|row| row.deleted_peek.clone()).map(|peek| view! {
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
                row.with(|row| row.deleted_after_peek.clone()).map(|peek| view! {
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
        </div>
    }
}

/// The state classes of a line number.
fn line_number_class(row: &RowUiState) -> &'static str {
    match (row.is_active, row.has_error || row.hidden_error, row.change) {
        (false, false, DiffKind::Unchanged) => "",
        (true, false, DiffKind::Unchanged) => "config-editor__line-number-active",
        (false, false, DiffKind::Added) => "config-editor__line-number-added",
        (true, false, DiffKind::Added) => {
            "config-editor__line-number-active config-editor__line-number-added"
        }
        (false, false, DiffKind::Modified) => "config-editor__line-number-modified",
        (true, false, DiffKind::Modified) => {
            "config-editor__line-number-active config-editor__line-number-modified"
        }
        (false, true, DiffKind::Unchanged) => "config-editor__line-number-error",
        (true, true, DiffKind::Unchanged) => {
            "config-editor__line-number-active config-editor__line-number-error"
        }
        (false, true, DiffKind::Added) => {
            "config-editor__line-number-error config-editor__line-number-added"
        }
        (true, true, DiffKind::Added) => {
            "config-editor__line-number-active config-editor__line-number-error config-editor__line-number-added"
        }
        (false, true, DiffKind::Modified) => {
            "config-editor__line-number-error config-editor__line-number-modified"
        }
        (true, true, DiffKind::Modified) => {
            "config-editor__line-number-active config-editor__line-number-error config-editor__line-number-modified"
        }
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
