//! Schema docs for the key or value under the pointer, under the problems
//! on its line (see `hover_problems`).

use leptos::html::Div;
use leptos::prelude::*;

use super::super::completion::{ChoiceHover, HoverValue, KeyHover};
use super::super::state::EditorState;
use super::diagnostics::{Entry, entries_view, problems_at};
use crate::cssom::cssom;
use crate::reactive::{Relay, later};
use crate::strings::fill;
use wasm_bindgen::JsCast;

/// A hover card being shown: what it documents and where.
#[derive(Clone, PartialEq)]
pub(crate) struct HoverCard {
    pub(crate) info: KeyHover,
    pub(crate) line: usize,
    /// The one-based place at the caret it was shown at (`Ctrl+K Ctrl+I`,
    /// a tap), whose problems it lists.
    pub(crate) caret: Option<(usize, usize)>,
    pub(crate) style: crate::cssom::Declarations,
}

#[component]
pub(crate) fn HoverCardView(state: EditorState) -> impl IntoView {
    let problems = Memo::new(move |_| {
        state
            .card_problem_place()
            .map(|place| problems_at(state, place))
            .unwrap_or_default()
    });
    let card_ref = state.hover().card_ref;
    let focused = Relay::new(move || state.hover().card_focused.get());
    // Taller than its room (it scrolls): measured once drawn.
    let scrolls = RwSignal::new(false);
    Effect::new(move |_| {
        state.hover().card.track();
        problems.track();
        later::next_frame(scrolls, move |scrolls| measure(state, card_ref, scrolls));
    });
    move || {
        state
            .hover()
            .card
            .get()
            .filter(|_| state.completion().suggestions.get().is_empty())
            .map(|card| {
                let info = card.info;
                let examples = info.examples.join(", ");
                let problems = problems.get();
                // A card with a link, or one taller than its room, takes the
                // pointer, so the link can be reached and clicked and the
                // card scrolled; others let it through to the text.
                let links = info.docs_url.is_some()
                    || problems.iter().any(|entry| entry.docs_url.is_some());
                let interactive = move || links || scrolls.get();
                let focused = focused.read();
                // A dialog while it has the focus (to scroll it, or on its
                // links), named after its key; a tooltip otherwise.
                let role = move || if focused.get() { "dialog" } else { "tooltip" };
                let label = card_label(state, info.key.clone());
                view! {
                    <div
                        class="config-editor__hover"
                        class:config-editor__hover--interactive=interactive
                        class:config-editor__hover--scrolls=move || scrolls.get()
                        role=role
                        aria-label=label
                        // Focused by a second Ctrl+K Ctrl+I, to scroll it.
                        tabindex="-1"
                        node_ref=state.hover().card_ref
                        style=cssom(card.style)
                        on:keydown=move |event| card_key(state, &event)
                        on:focusin=move |_| state.note_card_focus(true)
                        on:focusout=move |event| leave_card_focus(state, &event)
                        on:mouseenter=move |_| enter_card(state)
                        on:mouseleave=move |_| leave_card(state)
                        // Clicking the link keeps the caret (and the card).
                        on:mousedown=move |event| event.prevent_default()
                    >
                        {problems_section(state, problems)}
                        {info.value.map(value_section)}
                        <div class="config-editor__hover-header">
                            <code class="config-editor__hover-key">{info.key}</code>
                            <span class="config-editor__hover-type">{info.type_label}</span>
                            {info.required.then(|| view! {
                                <span class="config-editor__hover-required">{state.text(|strings| &strings.required)}</span>
                            })}
                            {info.deprecated.then(|| view! {
                                <span class="config-editor__hover-deprecated">{state.text(|strings| &strings.deprecated)}</span>
                            })}
                        </div>
                        {info.deprecation.map(|message| view! {
                            <p class="config-editor__hover-deprecation">{message}</p>
                        })}
                        {info.title.map(|title| view! {
                            <p class="config-editor__hover-title">{title}</p>
                        })}
                        {info.description.map(|description| view! {
                            <p class="config-editor__hover-description">{description}</p>
                        })}
                        {info.choice.map(|choice| choice_section(state, choice))}
                        {info.default.map(|default| view! {
                            <div class="config-editor__hover-meta">
                                <span>{state.text(|strings| &strings.default_value)}</span>
                                <code>{default}</code>
                            </div>
                        })}
                        {(!examples.is_empty()).then(|| view! {
                            <div class="config-editor__hover-meta">
                                <span>{state.text(|strings| &strings.examples)}</span>
                                <code>{examples}</code>
                            </div>
                        })}
                        {info.docs_url.map(|url| view! {
                            <a
                                class="config-editor__hover-link"
                                href=url
                                target="_blank"
                                rel="noopener noreferrer"
                            >
                                {state.text(|strings| &strings.documentation)}
                            </a>
                        })}
                    </div>
                }
            })
    }
}

/// Whether the card is taller than its room (its `max-height`), into
/// `scrolls` (and the hover state, for the way to it).
fn measure(state: EditorState, card_ref: NodeRef<Div>, scrolls: RwSignal<bool>) {
    let overflows = card_ref
        .get_untracked()
        .is_some_and(|card| card.scroll_height() > card.client_height());
    state.hover().scrolls.set_value(overflows);
    if scrolls.get_untracked() != overflows {
        scrolls.set(overflows);
    }
}

/// The pointer is over the card: it stays, with the problems it lists.
fn enter_card(state: EditorState) {
    state.hover().card_entered.set_value(true);
    state.problems().tip_entered.set_value(true);
}

/// The pointer left the card: it closes, as when it leaves a key, and so
/// do the problems it listed (not on to the problem tip: the pointer back
/// on them shows them again).
fn leave_card(state: EditorState) {
    state.hover().card_entered.set_value(false);
    state.problems().tip_entered.set_value(false);
    if state.card_has_focus() {
        return;
    }
    state.hide_hover();
    state.hide_problem_tip();
}

/// The card's accessible name, after the key it documents.
fn card_label(state: EditorState, key: String) -> impl Fn() -> String + Send + Sync + 'static {
    let strings = state.strings_signal();
    move || strings.with(|strings| fill(&strings.hover_label, &[("key", &key)]))
}

/// A key with the focus in the card: `Enter` on the card itself, or
/// `Alt+Enter` anywhere in it, opens its docs (as `Alt+Enter` does from
/// the text); the scroll keys scroll it.
fn card_key(state: EditorState, event: &web_sys::KeyboardEvent) {
    if let Some(card) = state.hover().card_ref.get_untracked() {
        if super::super::hover_focus::opens_docs_from(state, &card, event) {
            return;
        }
        super::super::hover_focus::card_keys(&card, event);
    }
}

/// The focus left the card or a link in it: it closes, unless the focus
/// went to another of its links or back to the text.
fn leave_card_focus(state: EditorState, event: &web_sys::FocusEvent) {
    let to = event.related_target();
    let in_card = state.hover().card_ref.get_untracked().is_some_and(|card| {
        to.as_ref()
            .and_then(|to| to.dyn_ref::<web_sys::Node>())
            .is_some_and(|to| card.contains(Some(to)))
    });
    if !in_card {
        state.note_card_focus(false);
    }
    if !state.keep_card_for(to.as_ref()) {
        state.hide_hover();
    }
}

/// The problems on the card's line, first, as the problem tip lists them.
fn problems_section(state: EditorState, problems: Vec<Entry>) -> Option<impl IntoView> {
    (!problems.is_empty()).then(|| {
        view! {
            <div class="config-editor__hover-problems">{entries_view(state, problems)}</div>
        }
    })
}

/// The `oneOf`/`anyOf` option the field's object is in, or the titles of
/// all of them while none matches.
fn choice_section(state: EditorState, choice: ChoiceHover) -> impl IntoView {
    match choice {
        ChoiceHover::Matched(doc) => view! {
            <div class="config-editor__hover-choice">
                {doc.title.map(|title| view! {
                    <strong class="config-editor__hover-choice-title">{title}</strong>
                })}
                {doc.description.map(|description| view! {
                    <p class="config-editor__hover-description">{description}</p>
                })}
            </div>
        }
        .into_any(),
        ChoiceHover::OneOf(titles) => view! {
            <div class="config-editor__hover-meta config-editor__hover-choices">
                <span>{state.text(|strings| &strings.one_of)}</span>
                <code>{titles.join(", ")}</code>
            </div>
        }
        .into_any(),
    }
}

/// The hovered value, first: its title and description, above its key's.
fn value_section(value: HoverValue) -> impl IntoView {
    view! {
        <div class="config-editor__hover-value">
            <div class="config-editor__hover-header">
                <code class="config-editor__hover-value-label">{value.label}</code>
                {value.doc.title.map(|title| view! {
                    <strong class="config-editor__hover-value-title">{title}</strong>
                })}
            </div>
            {value.doc.description.map(|description| view! {
                <p class="config-editor__hover-description">{description}</p>
            })}
        </div>
    }
}
