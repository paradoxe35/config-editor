//! The list of a name's uses (Find uses, see `key_names`): under the
//! name, a listbox grouped by document, each row a use's line; the focus
//! stays in the text, which points at the row the arrows are on
//! (`aria-activedescendant`), as with the suggestions.

use leptos::html::Div;
use leptos::prelude::*;

use wasm_bindgen::JsCast;

use super::super::key_names::{UseGroup, empty_text, title};
use super::super::popups::caret_tip_style;
use super::super::state::EditorState;
use crate::cssom::cssom;
use crate::editor::text::utf16_of;
use crate::reactive::Relay;

#[component]
pub(crate) fn UsesListView(state: EditorState) -> impl IntoView {
    let uses = state.uses();
    // The rows come and go with each list, so they read the row the
    // keyboard is on through a relay (see `reactive`).
    let active = Relay::new(move || uses.active.get());
    let body: NodeRef<Div> = NodeRef::new();
    Effect::new(move |_| {
        let index = uses.active.get();
        uses.list.track();
        scroll_to_row(state, body, index);
    });
    move || {
        let list = uses.list.get()?;
        let offset = state
            .doc()
            .text
            .with_untracked(|text| utf16_of(text, list.anchor));
        let style = caret_tip_style(state, offset)?;
        let label = state.strings(|strings| title(&list, strings));
        let heading = label.clone();
        let empty =
            (list.count() == 0).then(|| state.strings(|strings| empty_text(&list, strings)));
        let searching = list.searching && list.count() > 0;
        let firsts = list.firsts();
        let groups = list
            .groups
            .into_iter()
            .zip(firsts)
            .map(|(group, first)| group_view(state, active, group, first))
            .collect_view();
        Some(view! {
            <div class="config-editor__uses" style=cssom(style)>
                <div class="config-editor__uses-title" aria-hidden="true">{heading}</div>
                <div
                    class="config-editor__uses-body"
                    role="listbox"
                    id=state.a11y().uses_id()
                    aria-label=label
                    node_ref=body
                >
                    {groups}
                </div>
                {empty.map(|text| view! { <div class="config-editor__uses-empty">{text}</div> })}
                {searching.then(|| view! {
                    <div class="config-editor__uses-empty">{state.text(|strings| &strings.uses_searching)}</div>
                })}
            </div>
        })
    }
}

/// One document's group: its title, then its rows (numbered from `first`
/// over the whole list).
fn group_view(
    state: EditorState,
    active: Relay<usize>,
    group: UseGroup,
    first: usize,
) -> impl IntoView {
    let rows = group
        .rows
        .into_iter()
        .enumerate()
        .map(|(at, row)| {
            let index = first + at;
            let active = active.read();
            let selected = move || active.get() == index;
            let line = row.line.map(|line| line.to_string());
            view! {
                <div
                    role="option"
                    id=state.a11y().use_id(index)
                    aria-selected=move || selected().to_string()
                    class="config-editor__use"
                    class:config-editor__use--active=selected
                    on:mousedown=keep_focus
                    on:mousemove=move |event| point_at(state, index, &event)
                    on:click=move |_| choose(state, index)
                >
                    {line.map(|line| view! { <span class="config-editor__use-line">{line}</span> })}
                    <code class="config-editor__use-text">{row.text}</code>
                </div>
            }
        })
        .collect_view();
    let file = group.label.clone();
    view! {
        <div role="group" class="config-editor__uses-group" aria-label=group.label>
            <div class="config-editor__uses-file" aria-hidden="true">{file}</div>
            {rows}
        </div>
    }
}

/// A press on a row keeps the focus in the text (the click goes there).
fn keep_focus(event: web_sys::MouseEvent) {
    event.prevent_default();
}

/// The pointer moving over a row puts the list on it. The list opening
/// under a pointer that stays still keeps the keyboard's row.
fn point_at(state: EditorState, index: usize, event: &web_sys::MouseEvent) {
    let moved = event.movement_x() != 0 || event.movement_y() != 0;
    if moved && state.uses().active.get_untracked() != index {
        state.uses().active.set(index);
    }
}

/// A click (or tap) on a row goes to its use.
fn choose(state: EditorState, index: usize) {
    state.go_to_use(index);
}

/// Scrolls the list's body to show row `index`, inside the list only (the
/// page never scrolls for it).
fn scroll_to_row(state: EditorState, body: NodeRef<Div>, index: usize) {
    let Some(body) = body.get_untracked() else {
        return;
    };
    let Ok(Some(row)) = body.query_selector(&format!("#{}", state.a11y().use_id(index))) else {
        return;
    };
    let Some(row) = row.dyn_ref::<web_sys::HtmlElement>() else {
        return;
    };
    let (top, height) = (row.offset_top(), row.offset_height());
    let (scroll, room) = (body.scroll_top(), body.client_height());
    if top < scroll {
        body.set_scroll_top(top);
    } else if top + height > scroll + room {
        body.set_scroll_top(top + height - room);
    }
}
