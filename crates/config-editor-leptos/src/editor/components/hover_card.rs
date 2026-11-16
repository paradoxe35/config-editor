//! Schema docs for the key or value under the pointer, under the problems
//! on its line (see `popups`).

use leptos::html::Div;
use leptos::prelude::*;

use super::super::completion::{ChoiceHover, HoverValue, KeyHover, ValueDoc};
use super::super::popups::{
    back_on_text, card_keys, enter_popup, opens_docs_from, popup_wheel, selects_all_in,
};
use super::super::positioning::{CardAnchor, hover_card_geometry};
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
    /// The one-based place whose problems it lists: under the pointer on
    /// its line, F8's there, or the caret's it was shown at
    /// (`Ctrl+K Ctrl+I`, a tap).
    pub(crate) problems: Option<(usize, usize)>,
    /// Where it points; its side waits for its height.
    pub(crate) anchor: CardAnchor,
}

#[component]
pub(crate) fn HoverCardView(state: EditorState) -> impl IntoView {
    let problems = Memo::new(move |_| {
        state
            .card_problem_place()
            .map(|place| problems_at(state, place))
            .unwrap_or_default()
    });
    let card = Memo::new(move |_| state.popups().card());
    let card_ref = state.popups().card_ref;
    let focused = Relay::new(move || state.popups().focused.get());
    let passing = Relay::new(move || state.popups().passing());
    // Its full height (0 until measured, unseen until then), which
    // decides its side; then whether it is taller than its room (it
    // scrolls): measured once drawn.
    let natural = state.popups().card_height;
    let scrolls = RwSignal::new(false);
    Effect::new(move |_| {
        card.track();
        problems.track();
        if natural.get_untracked() != 0.0 {
            natural.set(0.0);
        }
        // Once drawn, in the same task (a tip giving way to the card shows
        // no frame without a popup); else before it is painted.
        later::soon(scrolls, move |scrolls| {
            if measure_height(card_ref, natural) {
                later::next_frame(scrolls, move |scrolls| measure(card_ref, scrolls));
            } else {
                later::next_frame(scrolls, move |scrolls| {
                    measure_height(card_ref, natural);
                    later::next_frame(scrolls, move |scrolls| measure(card_ref, scrolls));
                });
            }
        });
    });
    let style = Relay::new(move || {
        card.with(|card| card.as_ref().map(|card| card.anchor))
            .map(|anchor| hover_card_geometry(anchor, natural.get()))
    });
    let measured = Relay::new(move || natural.get() > 0.0);
    let scrolling = Relay::new(move || scrolls.get());
    move || {
        card.get()
            .filter(|_| state.completion().suggestions.with(Vec::is_empty))
            .map(|card| {
                let info = card.info;
                let examples = info.examples.join(", ");
                let problems = problems.get();
                // The card takes the pointer: its text can be selected and
                // copied, its links clicked, and it scrolls. On a touch
                // screen only one with a link or that scrolls takes taps;
                // others let them through to the text (see the stylesheet).
                let links = info.docs_url.is_some()
                    || problems.iter().any(|entry| entry.docs_url.is_some());
                let scrolls = scrolling.read();
                let interactive = move || links || scrolls.get();
                let focused = focused.read();
                let style = style.read();
                let measured = measured.read();
                let passing = passing.read();
                // A dialog while it has the focus (to scroll it, or on its
                // links), named after its key; a tooltip otherwise.
                let role = move || if focused.get() { "dialog" } else { "tooltip" };
                let label = card_label(state, info.key.clone());
                view! {
                    <div
                        class="config-editor__hover"
                        class:config-editor__hover--interactive=interactive
                        class:config-editor__hover--scrolls=move || scrolls.get()
                        class:config-editor__hover--unmeasured=move || !measured.get()
                        // It lets the pointer through to the text under it
                        // (see `popups::under`).
                        data-passing=move || passing.get().then_some("")
                        role=role
                        aria-label=label
                        // Focused by a second Ctrl+K Ctrl+I, to scroll it.
                        tabindex="-1"
                        node_ref=card_ref
                        style=cssom(move || style.get())
                        on:keydown=move |event| card_key(state, &event)
                        on:focusin=move |_| state.note_card_focus(true)
                        on:focusout=move |event| leave_card_focus(state, &event)
                        on:mouseenter=move |event| enter_card(state, &event)
                        on:mouseleave=move |event| leave_card(state, &event)
                        on:mousedown=move |event| press_card(&event)
                        on:wheel=move |event| wheel_card(state, &event)
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
                        {info.item.map(item_section)}
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

/// The card's full height, borders included, into `natural`; whether it
/// was on the page to be measured.
fn measure_height(card_ref: NodeRef<Div>, natural: RwSignal<f64>) -> bool {
    let Some(card) = card_ref.get_untracked() else {
        return false;
    };
    if card.offset_height() == 0 {
        return false;
    }
    let height = f64::from(card.scroll_height() + card.offset_height() - card.client_height());
    if natural.get_untracked() != height {
        natural.set(height);
    }
    true
}

/// Whether the card is taller than its room (its `max-height`), into
/// `scrolls`.
fn measure(card_ref: NodeRef<Div>, scrolls: RwSignal<bool>) {
    let overflows = card_ref
        .get_untracked()
        .is_some_and(|card| card.scroll_height() > card.client_height());
    if scrolls.get_untracked() != overflows {
        scrolls.set(overflows);
    }
}

/// A press on a link keeps the caret (and the card) while it opens; one on
/// the text selects it, to be copied.
fn press_card(event: &web_sys::MouseEvent) {
    let on_link = event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|target| target.closest("a").ok().flatten())
        .is_some();
    if on_link {
        event.prevent_default();
    }
}

/// The pointer came onto the card: from its key, it stays, with the
/// problems it lists; another way, it lets the pointer through to the
/// text under it (see `popups::pointer`).
fn enter_card(state: EditorState, event: &web_sys::MouseEvent) {
    if let Some(card) = state.popups().card_ref.get_untracked() {
        enter_popup(state, &card, event);
    }
}

/// A wheel over the card: see `popups::pointer`.
fn wheel_card(state: EditorState, event: &web_sys::WheelEvent) {
    if let Some(card) = state.popups().card_ref.get_untracked() {
        popup_wheel(state, &card, event);
    }
}

/// The pointer left the card it entered: it closes, and so do the
/// problems it listed (not on to the problem tip: the pointer back on them
/// shows them again). Back onto the text, as when it leaves a key: after
/// its moment, unless the pointer is on the card's key again (it stays,
/// without closing to open again).
fn leave_card(state: EditorState, event: &web_sys::MouseEvent) {
    if !state.left_popup() || state.card_has_focus() {
        return;
    }
    state.leave_hover();
    if !back_on_text(state, event) {
        state.hide_hover();
    }
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
    if let Some(card) = state.popups().card_ref.get_untracked() {
        if opens_docs_from(state, &card, event) || selects_all_in(&card, event) {
            return;
        }
        card_keys(&card, event);
    }
}

/// The focus left the card or a link in it: it closes, unless the focus
/// went to another of its links or back to the text.
fn leave_card_focus(state: EditorState, event: &web_sys::FocusEvent) {
    let to = event.related_target();
    let in_card = state.popups().card_ref.get_untracked().is_some_and(|card| {
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

/// A list item's own title and description, under the list's.
fn item_section(item: ValueDoc) -> impl IntoView {
    view! {
        <div class="config-editor__hover-item">
            {item.title.map(|title| view! {
                <strong class="config-editor__hover-item-title">{title}</strong>
            })}
            {item.description.map(|description| view! {
                <p class="config-editor__hover-description">{description}</p>
            })}
        </div>
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
