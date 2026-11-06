//! Schema docs for the key under the pointer.

use leptos::prelude::*;

use super::super::completion::KeyHover;
use super::super::state::EditorState;

/// A hover card being shown: what it documents and where.
#[derive(Clone, PartialEq)]
pub(crate) struct HoverCard {
    pub(crate) info: KeyHover,
    pub(crate) line: usize,
    pub(crate) style: String,
}

#[component]
pub(crate) fn HoverCardView(state: EditorState) -> impl IntoView {
    move || {
        state
            .hover_card
            .get()
            .filter(|_| state.suggestions.get().is_empty())
            .map(|card| {
                let info = card.info;
                let examples = info.examples.join(", ");
                view! {
                    <div class="config-editor__hover" role="tooltip" style=card.style>
                        <div class="config-editor__hover-header">
                            <code class="config-editor__hover-key">{info.key}</code>
                            <span class="config-editor__hover-type">{info.type_label}</span>
                            {info.required.then(|| view! {
                                <span class="config-editor__hover-required">"required"</span>
                            })}
                        </div>
                        {info.description.map(|description| view! {
                            <p class="config-editor__hover-description">{description}</p>
                        })}
                        {info.default.map(|default| view! {
                            <div class="config-editor__hover-meta">
                                <span>"Default"</span>
                                <code>{default}</code>
                            </div>
                        })}
                        {(!examples.is_empty()).then(|| view! {
                            <div class="config-editor__hover-meta">
                                <span>"Examples"</span>
                                <code>{examples}</code>
                            </div>
                        })}
                    </div>
                }
            })
    }
}
