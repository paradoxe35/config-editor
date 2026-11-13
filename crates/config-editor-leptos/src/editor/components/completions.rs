//! The suggestion list under the caret: a listbox whose rows around its
//! scroll position are rendered, with spacers for the others, and the
//! highlighted row wherever it is (see `completion::window`).

use leptos::prelude::*;

use super::super::completion::RowSize;
use super::super::completion::{
    ShownOption, completion_list_scrolled, completion_popup_style, most_option_rows,
};
use super::super::state::EditorState;
use crate::cssom::{Declarations, cssom};
use crate::reactive::{KeyedList, Relay, RowSpacer};

#[component]
pub(crate) fn CompletionList(state: EditorState) -> impl IntoView {
    // The rows come and go as the list filters and scrolls, so they read
    // the selection through a relay (see `reactive`).
    let active = Relay::new(move || state.completion().active.get());
    let rows_after = move || {
        let count = state.completion().suggestions.with(Vec::len);
        count - state.shown_rows().end.min(count)
    };
    view! {
        <div
            class="config-editor__completions"
            class:config-editor__completions--compact=move || state.completion().row_size.get() == RowSize::Compact
            class:hidden=move || state.completion().suggestions.with(Vec::is_empty)
            role="listbox"
            id=state.a11y().suggestions_id()
            aria-label=state.language_text(|strings| &strings.suggestions_label)
            node_ref=state.view().completion_ref
            on:scroll=move |_| completion_list_scrolled(state)
            style=cssom(move || {
                state.view().layout_revision.get();
                state.view().scroll_x.get();
                state.view().scroll_y.get();
                // Return before reading source and caret so typing with the
                // list hidden forces no layout.
                let count = state.completion().suggestions.with(Vec::len);
                if count == 0 {
                    return hidden();
                }
                state
                    .view().textarea_ref
                    .get()
                    .map(|editor| {
                        state.view().projection.with(|projection| {
                            completion_popup_style(state, &editor, projection, &state.doc().text.get(), state.doc().caret.get(), count, state.completion().row_size.get())
                        })
                    })
                    .unwrap_or_else(hidden)
            })
        >
            <RowSpacer rows=move || state.shown_rows().start row_height=move || state.completion().row_size.get().css_height() />
            <KeyedList
                each=move || state.shown_options()
                key=|row: &ShownOption| (row.listing, row.index)
                // A new list replaces every row: room for the old and the new.
                reserve=2 * most_option_rows(RowSize::Compact)
                children=move |row| view! { <Suggestion state=state active=active row=row pinned=false /> }
            />
            <RowSpacer rows=rows_after row_height=move || state.completion().row_size.get().css_height() />
            // The highlighted row when the list is scrolled away from it,
            // at its place: `aria-activedescendant` always finds it.
            <KeyedList
                each=move || state.pinned_option()
                key=|row: &ShownOption| (row.listing, row.index)
                children=move |row| view! { <Suggestion state=state active=active row=row pinned=true /> }
            />
        </div>
    }
}

#[component]
fn Suggestion(
    state: EditorState,
    /// The selected item's index.
    active: Relay<usize>,
    row: ShownOption,
    /// Shown out of the rows around the scroll position, at its place.
    pinned: bool,
) -> impl IntoView {
    let ShownOption {
        index,
        count,
        option,
        ..
    } = row;
    let active = active.read();
    let selected = move || active.get() == index;
    let top = if pinned {
        format!(
            "{}px",
            index as f64 * state.completion().row_size.get_untracked().height()
        )
    } else {
        String::new()
    };
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
            id=state.a11y().suggestion_id(index)
            aria-selected=move || selected().to_string()
            aria-setsize=count
            aria-posinset=index + 1
            class="config-editor__completion"
            class:config-editor__completion-active=selected
            class:config-editor__completion--deprecated=deprecated
            class:config-editor__completion--pinned=pinned
            style:top=top
            on:mousedown=move |event| event.prevent_default()
            on:mousemove=move |event| hover_selects(state, index, &event)
            on:click=move |_| state.accept(&option, false)
        >
            <span class="config-editor__completion-icon" aria-hidden="true">{icon}</span>
            <span class="config-editor__completion-content">
                <span class="config-editor__completion-primary">
                    <span class="config-editor__completion-key">{label}</span>
                    {deprecated.then(|| view! {
                        <span class="config-editor__completion-deprecated">{state.text(|strings| &strings.deprecated)}</span>
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
    if moved && state.completion().active.get_untracked() != index {
        state.completion().active.set(index);
    }
}

/// The list's style while it has nothing to show.
fn hidden() -> Declarations {
    Declarations::new().set("visibility", "hidden")
}
