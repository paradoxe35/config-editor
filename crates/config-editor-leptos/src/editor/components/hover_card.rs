//! Schema docs for the key or value under the pointer.

use leptos::prelude::*;

use super::super::completion::{ChoiceHover, HoverValue, KeyHover};
use super::super::state::EditorState;
use crate::cssom::cssom;

/// A hover card being shown: what it documents and where.
#[derive(Clone, PartialEq)]
pub(crate) struct HoverCard {
    pub(crate) info: KeyHover,
    pub(crate) line: usize,
    pub(crate) style: crate::cssom::Declarations,
}

#[component]
pub(crate) fn HoverCardView(state: EditorState) -> impl IntoView {
    move || {
        state
            .hover()
            .card
            .get()
            .filter(|_| state.completion().suggestions.get().is_empty())
            .map(|card| {
                let info = card.info;
                let examples = info.examples.join(", ");
                // A card with a link takes the pointer, so the link can be
                // reached and clicked; others let it through to the text.
                let interactive = info.docs_url.is_some();
                view! {
                    <div
                        class="config-editor__hover"
                        class:config-editor__hover--interactive=interactive
                        role="tooltip"
                        style=cssom(card.style)
                        on:mouseenter=move |_| state.hover().card_entered.set_value(true)
                        on:mouseleave=move |_| leave_card(state)
                        // Clicking the link keeps the caret (and the card).
                        on:mousedown=move |event| event.prevent_default()
                    >
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

/// The pointer left the card: it closes, as when it leaves a key.
fn leave_card(state: EditorState) {
    state.hover().card_entered.set_value(false);
    state.hide_hover();
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
