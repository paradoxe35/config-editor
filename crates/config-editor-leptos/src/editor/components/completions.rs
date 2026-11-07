//! The suggestion list under the caret.

use leptos::prelude::*;

use super::super::completion::{Completion, completion_popup_style};
use super::super::state::EditorState;
use crate::reactive::Relay;

#[component]
pub(crate) fn CompletionList(state: EditorState) -> impl IntoView {
    // The items are rebuilt as the list filters, so they read the selection
    // through a relay (see `reactive`).
    let active = Relay::new(move || state.completion.active.get());
    view! {
        <div
            class="config-editor__completions"
            class:hidden=move || state.completion.suggestions.get().is_empty()
            role="listbox"
            aria-label=move || format!("{} suggestions", state.label())
            node_ref=state.view.completion_ref
            style=move || {
                state.view.layout_revision.get();
                state.view.scroll_x.get();
                state.view.scroll_y.get();
                // Return before reading source and caret so typing with the
                // list hidden forces no layout.
                let count = state.completion.suggestions.with(Vec::len);
                if count == 0 {
                    return "visibility:hidden".to_string();
                }
                state
                    .view.textarea_ref
                    .get()
                    .map(|editor| {
                        state.fold.projection.with(|projection| {
                            completion_popup_style(&editor, projection, &state.doc.text.get(), state.doc.caret.get(), count)
                        })
                    })
                    .unwrap_or_else(|| "visibility:hidden".to_string())
            }
        >
            {move || {
                let active = active.read();
                state
                    .completion.suggestions
                    .get()
                    .into_iter()
                    .enumerate()
                    .map(|(index, option)| view! { <Suggestion state=state active=active index=index option=option /> })
                    .collect_view()
            }}
        </div>
    }
}

#[component]
fn Suggestion(
    state: EditorState,
    /// The selected item's index.
    active: Signal<usize>,
    index: usize,
    option: Completion,
) -> impl IntoView {
    let selected = move || active.get() == index;
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
    let deprecated = option.deprecated;
    view! {
        <button
            type="button"
            role="option"
            aria-selected=selected
            class="config-editor__completion"
            class:config-editor__completion-active=selected
            class:config-editor__completion--deprecated=deprecated
            on:mousedown=move |event| event.prevent_default()
            on:mousemove=move |event| hover_selects(state, index, &event)
            on:click=move |_| state.accept(&option, false)
        >
            <span class="config-editor__completion-icon" aria-hidden="true">{icon}</span>
            <span class="config-editor__completion-content">
                <span class="config-editor__completion-primary">
                    <span class="config-editor__completion-key">{label}</span>
                    {deprecated.then(|| view! {
                        <span class="config-editor__completion-deprecated">"deprecated"</span>
                    })}
                    <span class="config-editor__completion-type">{type_label}</span>
                </span>
                {(!detail.is_empty()).then(|| view! {
                    <span class="config-editor__completion-detail" title=detail.clone()>{detail.clone()}</span>
                })}
            </span>
        </button>
    }
}

/// The pointer selects the item it moves over. The list opening under a
/// pointer that stays still keeps the keyboard's selection.
fn hover_selects(state: EditorState, index: usize, event: &web_sys::MouseEvent) {
    let moved = event.movement_x() != 0 || event.movement_y() != 0;
    if moved && state.completion.active.get_untracked() != index {
        state.completion.active.set(index);
    }
}
