//! The suggestion list under the caret.

use leptos::prelude::*;

use super::super::completion::{Completion, completion_popup_style};
use super::super::state::EditorState;

#[component]
pub(crate) fn CompletionList(state: EditorState) -> impl IntoView {
    view! {
        <div
            class="config-editor__completions"
            class:hidden=move || state.suggestions.get().is_empty()
            role="listbox"
            aria-label=move || format!("{} suggestions", state.label())
            node_ref=state.completion_ref
            style=move || {
                state.layout_revision.get();
                state.scroll_x.get();
                state.scroll_y.get();
                // Return before reading source and caret so typing with the
                // list hidden forces no layout.
                let count = state.suggestions.with(Vec::len);
                if count == 0 {
                    return "visibility:hidden".to_string();
                }
                state
                    .editor_ref
                    .get()
                    .map(|editor| completion_popup_style(&editor, &state.source.get(), state.cursor.get(), count))
                    .unwrap_or_else(|| "visibility:hidden".to_string())
            }
        >
            {move || {
                state
                    .suggestions
                    .get()
                    .into_iter()
                    .enumerate()
                    .map(|(index, option)| view! { <Suggestion state=state index=index option=option /> })
                    .collect_view()
            }}
        </div>
    }
}

#[component]
fn Suggestion(state: EditorState, index: usize, option: Completion) -> impl IntoView {
    let selected = move || state.active_completion.get() == index;
    let label = option
        .label
        .chars()
        .enumerate()
        .map(|(index, character)| {
            let matched = option
                .matched
                .iter()
                .any(|(start, end)| (*start..*end).contains(&index));
            if matched {
                view! { <span class="config-editor__completion-match">{character.to_string()}</span> }
                    .into_any()
            } else {
                view! { <span>{character.to_string()}</span> }.into_any()
            }
        })
        .collect_view();
    let detail = option.detail.clone();
    let icon = if option.is_value { "V" } else { "K" };
    let type_label = option.type_label.clone();
    view! {
        <button
            type="button"
            role="option"
            aria-selected=selected
            class="config-editor__completion"
            class:config-editor__completion-active=selected
            on:mousedown=move |event| event.prevent_default()
            on:mouseenter=move |_| state.active_completion.set(index)
            on:click=move |_| state.accept(&option, false)
        >
            <span class="config-editor__completion-icon" aria-hidden="true">{icon}</span>
            <span class="config-editor__completion-content">
                <span class="config-editor__completion-primary">
                    <span class="config-editor__completion-key">{label}</span>
                    <span class="config-editor__completion-type">{type_label}</span>
                </span>
                {(!detail.is_empty()).then(|| view! {
                    <span class="config-editor__completion-detail" title=detail.clone()>{detail.clone()}</span>
                })}
            </span>
        </button>
    }
}
