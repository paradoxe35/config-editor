//! Schema docs for the key or value under the pointer, under the problems
//! on its line (see `hover_problems`).

use leptos::prelude::*;

use super::super::completion::{ChoiceHover, HoverValue, KeyHover};
use super::super::state::EditorState;
use super::diagnostics::{Entry, entries_view, problems_at};
use crate::cssom::cssom;

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
                // A card with a link takes the pointer, so the link can be
                // reached and clicked; others let it through to the text.
                let interactive = info.docs_url.is_some()
                    || problems.iter().any(|entry| entry.docs_url.is_some());
                view! {
                    <div
                        class="config-editor__hover"
                        class:config-editor__hover--interactive=interactive
                        role="tooltip"
                        node_ref=state.hover().card_ref
                        style=cssom(card.style)
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

/// The pointer is over the card: it stays, with the problems it lists.
fn enter_card(state: EditorState) {
    state.hover().card_entered.set_value(true);
    state.problems().tip_entered.set_value(true);
}

/// The pointer left the card: it closes, as when it leaves a key, and so
/// do the problems it listed.
fn leave_card(state: EditorState) {
    state.hover().card_entered.set_value(false);
    state.problems().tip_entered.set_value(false);
    if state.card_has_focus() {
        return;
    }
    state.hide_hover();
    state.leave_problem();
}

/// The focus left a link in the card: it closes, unless the focus went to
/// another of its links or back to the text.
fn leave_card_focus(state: EditorState, event: &web_sys::FocusEvent) {
    if !state.keep_card_for(event.related_target().as_ref()) {
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
