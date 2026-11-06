//! The change preview a diff marker opens: the lines before and now.

use leptos::prelude::*;

use super::super::diff::{DiffPeek, peek_popup_style, peek_survives_edit};
use super::super::state::EditorState;

#[component]
pub(crate) fn DiffPeekPopup(state: EditorState) -> impl IntoView {
    close_when_edited(state);
    move || {
        state
            .selected_diff_peek
            .get()
            .map(|peek| view! { <Peek state=state peek=peek /> })
    }
}

/// Close the preview when an edit touches its anchor.
fn close_when_edited(state: EditorState) {
    Effect::new(move |_| {
        // The preview is read untracked so closing it cannot re-trigger this.
        state.diff.track();
        state.source.track();
        let survives = state.selected_diff_peek.with_untracked(|peek| {
            peek.as_ref().is_none_or(|peek| {
                let current: Vec<String> = state
                    .source
                    .with_untracked(|text| text.split('\n').map(str::to_string).collect());
                state
                    .diff
                    .with_untracked(|line_diff| peek_survives_edit(peek, &current, line_diff))
            })
        });
        if !survives {
            state.selected_diff_peek.set(None);
        }
    });
}

#[component]
fn Peek(state: EditorState, peek: DiffPeek) -> impl IntoView {
    let DiffPeek {
        line,
        title,
        before,
        after,
        ..
    } = peek;
    let (old_count, new_count) = (before.len(), after.len());
    view! {
        <section
            class="config-editor__diff-peek"
            role="dialog"
            aria-label=format!("{title}, line {line}")
            style=move || {
                state.layout_revision.get();
                state.scroll_y.get();
                match (state.editor_ref.get(), state.root_ref.get()) {
                    (Some(editor), Some(root)) => peek_popup_style(&editor, &root, line, old_count, new_count),
                    _ => "visibility:hidden".to_string(),
                }
            }
            on:keydown=move |event: web_sys::KeyboardEvent| {
                if event.key() == "Escape" {
                    state.selected_diff_peek.set(None);
                }
            }
        >
            <header class="config-editor__diff-peek-header">
                <strong>{format!("{title} · line {line}")}</strong>
                <button
                    type="button"
                    aria-label="Close change preview"
                    on:click=move |_| state.selected_diff_peek.set(None)
                >"×"</button>
            </header>
            <div class="config-editor__diff-peek-columns">
                {(!before.is_empty()).then(|| view! {
                    <section class="config-editor__diff-peek-side">
                        <h3>"Before"</h3>
                        {peek_lines(before, "−", "config-editor__diff-peek-removed")}
                    </section>
                })}
                {(!after.is_empty()).then(|| view! {
                    <section class="config-editor__diff-peek-side">
                        <h3>"Current"</h3>
                        {peek_lines(after, "+", "config-editor__diff-peek-current")}
                    </section>
                })}
            </div>
        </section>
    }
}

fn peek_lines(lines: Vec<String>, sign: &'static str, class: &'static str) -> impl IntoView {
    lines
        .into_iter()
        .map(|line| {
            view! {
                <div class=format!("config-editor__diff-peek-line {class}")>
                    <span aria-hidden="true">{sign}</span>
                    <code>{if line.is_empty() { " ".to_string() } else { line }}</code>
                </div>
            }
        })
        .collect_view()
}
