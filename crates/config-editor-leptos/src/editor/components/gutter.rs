//! Line numbers with error and diff markers; a modified or deleted marker
//! opens the change preview.

use leptos::prelude::*;

use super::super::commands::line_range;
use super::super::diff::{DiffKind, DiffPeek};
use super::super::line_rows::{LineRows, RowFolds};
use super::super::state::{EditorState, line_count};
use super::RowSpacer;

#[component]
pub(crate) fn Gutter(state: EditorState, rows: LineRows) -> impl IntoView {
    view! {
        <div
            class="config-editor__gutter"
            class:config-editor__gutter-folding=state.features.folding
            // Wide enough for the longest line number beside the diagnostic
            // dot (52px fits four digits) and the fold chevrons.
            style:width=move || {
                let digits = state.source.with(|text| line_count(text)).to_string().len();
                (digits > 4).then(|| {
                    format!(
                        "calc(52px + var(--config-editor-fold-gutter, 0px) + {}ch)",
                        digits - 4
                    )
                })
            }
        >
            <div
                class="config-editor__gutter-content"
                style:transform=move || format!("translateY(-{}px)", state.scroll_y.get())
            >
                <RowSpacer state=state />
                <For
                    each=move || state.folds().visible_lines.get()
                    key=|index| *index
                    children=move |index| view! { <LineNumber state=state rows=rows.clone() index=index /> }
                />
            </div>
        </div>
    }
}

#[component]
fn LineNumber(state: EditorState, rows: LineRows, index: usize) -> impl IntoView {
    let line = index + 1;
    // Subscribe only to the row's own signal (see `line_rows`).
    let active = state.current_line.get_untracked();
    // Borrow, never clone: cloning the whole document per new row made large
    // pastes quadratic.
    let row = state.lines.with_untracked(|lines| {
        state
            .diff
            .with_untracked(|diff| rows.state_for(index, lines, diff, active, RowFolds::default()))
    });
    let select_line = move |_| {
        // A line number selects its whole line, as in VS Code.
        let Some(offset) = state.focus_at(line, 1) else {
            return;
        };
        let (_, line_end) = state.source.with_untracked(|text| line_range(text, offset));
        if let Some(element) = state.textarea() {
            let _ = element.set_selection_range(offset as u32, line_end as u32);
        }
    };
    view! {
        <div
            data-line=line
            class:config-editor__line-number-active=move || row.with(|row| row.is_active)
            class:config-editor__line-number-error=move || row.with(|row| row.has_error || row.hidden_error)
            class:config-editor__line-number-added=move || row.with(|row| row.change == DiffKind::Added)
            class:config-editor__line-number-modified=move || row.with(|row| row.change == DiffKind::Modified)
            on:click=select_line
        >
            <span aria-hidden="true">{line}</span>
            {move || {
                row.with(|row| row.added_peek.clone()).map(|peek| view! {
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
                row.with(|row| row.modified_peek.clone()).map(|peek| view! {
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
                row.with(|row| row.deleted_peek.clone()).map(|peek| view! {
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
                row.with(|row| row.deleted_after_peek.clone()).map(|peek| view! {
                    <PeekTrigger
                        state=state
                        peek=peek
                        class="config-editor__diff-deleted config-editor__diff-deleted-after"
                        label="Show removed lines at end of document".to_string()
                        title="Show removed lines"
                    />
                })
            }}
        </div>
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
        state.selected_diff_peek.update(|selected| {
            *selected = if selected.as_ref() == Some(&peek) {
                None
            } else {
                Some(peek.clone())
            };
        });
    };
    let keydown = move |event: web_sys::KeyboardEvent| {
        if event.key() == "Escape" {
            state.selected_diff_peek.set(None);
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
