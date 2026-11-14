//! The find widget over the top right of the text: the query with its
//! match options, the match count, previous/next/close, and (for `Ctrl+H`)
//! the replace row.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::super::state::EditorState;
use super::super::{self as search, SearchOptions};
use crate::EditorStrings;
use crate::reactive::size_observer;

#[component]
pub(crate) fn FindWidget(state: EditorState) -> impl IntoView {
    let search = state.search();
    let editable = move || !state.config().read_only.get();
    let invalid = move || search.results.with(|results| results.error.is_some());
    // With no match, the buttons that step through or replace matches
    // show and say they are unavailable, and do nothing; they keep the
    // focus (a `disabled` button would drop it, as after "All").
    let unavailable = move || no_matches(state).then_some("true");
    // A message that wraps the row ("Replaced 3 occurrences") moves the
    // button just pressed: it stays in view, at any zoom. The rows are
    // watched, not the widget: one capped to the window (see `room`) keeps
    // its size while what it holds grows, and scrolls instead.
    Effect::new(move |_| {
        if let Some(widget) = search.widget_ref.get() {
            let children = widget.children();
            for index in 0..children.length() {
                if let Some(row) = children.item(index) {
                    let watched = widget.clone();
                    untrack(|| size_observer(&row, move || crate::focus::keep_in_view(&watched)));
                }
            }
        }
    });
    view! {
        <div
            class="config-editor__find"
            role="search"
            aria-label=state.text(|strings| &strings.find_widget)
            node_ref=search.widget_ref
            prop:hidden=move || !search.open.get()
            style:max-height=move || room(state)
            on:keydown=move |event| widget_keys(state, &event)
        >
            <div class="config-editor__find-row">
                <button
                    type="button"
                    class="config-editor__find-button config-editor__find-expand"
                    class:config-editor__find-expand-open=move || search.replacing.get()
                    aria-label=state.text(|strings| &strings.toggle_replace)
                    title=state.shortcut_text(|strings| &strings.toggle_replace_title)
                    aria-expanded=move || search.replacing.get().to_string()
                    prop:hidden=move || !editable()
                    on:click=move |_| toggle_replace(state)
                >
                    "›"
                </button>
                <div class="config-editor__find-field" class:config-editor__find-field-invalid=invalid>
                    <input
                        type="text"
                        class="config-editor__find-input"
                        aria-label=state.text(|strings| &strings.find)
                        placeholder=state.text(|strings| &strings.find)
                        spellcheck="false"
                        autocomplete="off"
                        aria-invalid=move || invalid().to_string()
                        title=move || search.results.with(|results| results.error.clone())
                        node_ref=search.query_ref
                        prop:value=move || search.query.get()
                        on:input=move |event| query_input(state, event_target_value(&event))
                        on:keydown=move |event| query_keys(state, &event)
                    />
                    <OptionToggle state=state label=|strings| &strings.match_case title=|strings| &strings.match_case_title text="Aa" read=|options| options.case_sensitive flip=flip_case />
                    <OptionToggle state=state label=|strings| &strings.whole_word title=|strings| &strings.whole_word_title text="ab" read=|options| options.whole_word flip=flip_word />
                    <OptionToggle state=state label=|strings| &strings.use_regex title=|strings| &strings.use_regex_title text=".*" read=|options| options.regex flip=flip_regex />
                </div>
                <span
                    class="config-editor__find-count"
                    class:config-editor__find-count-empty=move || {
                        search.results.with(|results| results.matches.is_empty()) && !search.query.with(String::is_empty)
                    }
                    role="status"
                    aria-live="polite"
                >
                    {move || count_text(state)}
                </span>
                <div class="config-editor__find-actions">
                    <button
                        type="button"
                        class="config-editor__find-button"
                        aria-label=state.text(|strings| &strings.previous_match)
                        title=state.text(|strings| &strings.previous_match_title)
                        aria-disabled=unavailable
                        on:click=move |_| previous(state)
                    >
                        "↑"
                    </button>
                    <button
                        type="button"
                        class="config-editor__find-button"
                        aria-label=state.text(|strings| &strings.next_match)
                        title=state.text(|strings| &strings.next_match_title)
                        aria-disabled=unavailable
                        on:click=move |_| next(state)
                    >
                        "↓"
                    </button>
                    <button
                        type="button"
                        class="config-editor__find-button"
                        aria-label=state.text(|strings| &strings.close)
                        title=state.text(|strings| &strings.close_title)
                        on:click=move |_| search::close(state)
                    >
                        "×"
                    </button>
                </div>
            </div>
            <div
                class="config-editor__find-row config-editor__replace-row"
                node_ref=search.replace_row_ref
                prop:hidden=move || !(search.replacing.get() && editable())
            >
                <input
                    type="text"
                    class="config-editor__find-input config-editor__replace-input"
                    aria-label=state.text(|strings| &strings.replace)
                    placeholder=state.text(|strings| &strings.replace)
                    spellcheck="false"
                    autocomplete="off"
                    node_ref=search.replace_ref
                    prop:value=move || search.replacement.get()
                    on:input=move |event| search.replacement.set(event_target_value(&event))
                    on:keydown=move |event| replace_keys(state, &event)
                />
                <button
                    type="button"
                    class="config-editor__find-button config-editor__find-text-button"
                    title=state.text(|strings| &strings.replace_title)
                    aria-disabled=unavailable
                    on:click=move |_| replace_one(state)
                >
                    {state.text(|strings| &strings.replace)}
                </button>
                <button
                    type="button"
                    class="config-editor__find-button config-editor__find-text-button"
                    aria-label=state.text(|strings| &strings.replace_all_label)
                    title=state.shortcut_text(|strings| &strings.replace_all_title)
                    aria-disabled=unavailable
                    on:click=move |_| replace_every(state)
                >
                    {state.text(|strings| &strings.replace_all)}
                </button>
            </div>
        </div>
    }
}

/// A match option as a pressed/unpressed button in the query field.
#[component]
fn OptionToggle(
    state: EditorState,
    label: fn(&EditorStrings) -> &String,
    title: fn(&EditorStrings) -> &String,
    /// The toggle's face: a symbol (`Aa`, `.*`), not a word.
    text: &'static str,
    read: fn(&SearchOptions) -> bool,
    flip: fn(&mut SearchOptions),
) -> impl IntoView {
    let pressed = move || state.search().options.with(read);
    view! {
        <button
            type="button"
            class="config-editor__find-toggle"
            class:config-editor__find-toggle-on=pressed
            aria-label=state.text(label)
            title=state.text(title)
            aria-pressed=move || pressed().to_string()
            on:click=move |_| search::toggle_option(state, flip)
        >
            {text}
        </button>
    }
}

fn flip_case(options: &mut SearchOptions) {
    options.case_sensitive = !options.case_sensitive;
}

fn flip_word(options: &mut SearchOptions) {
    options.whole_word = !options.whole_word;
}

fn flip_regex(options: &mut SearchOptions) {
    options.regex = !options.regex;
}

fn count_text(state: EditorState) -> String {
    let search = state.search();
    if let Some(notice) = search.notice.get() {
        return state.strings(|strings| notice.text(strings));
    }
    let query_empty = search.query.with(String::is_empty);
    let current = search.current.get();
    state.strings(|strings| {
        search
            .results
            .with(|results| search::count_label(strings, results, current, query_empty))
    })
}

/// Whether the query finds nothing (tracked): stepping and replacing are
/// unavailable.
fn no_matches(state: EditorState) -> bool {
    state
        .search()
        .results
        .with(|results| results.matches.is_empty())
}

/// The buttons' actions: nothing while there is no match.
fn previous(state: EditorState) {
    if !untrack(|| no_matches(state)) {
        search::step(state, true);
    }
}

fn next(state: EditorState) {
    if !untrack(|| no_matches(state)) {
        search::step(state, false);
    }
}

fn replace_one(state: EditorState) {
    if !untrack(|| no_matches(state)) {
        search::replace_current(state);
    }
}

fn replace_every(state: EditorState) {
    if !untrack(|| no_matches(state)) {
        search::replace_all(state);
    }
}

fn query_input(state: EditorState, value: String) {
    let was_empty = state.search().query.with_untracked(String::is_empty);
    state.search().query.set(value);
    search::select_nearest(state, true);
    search::say_no_results_after_empty(state, was_empty);
}

fn toggle_replace(state: EditorState) {
    let search = state.search();
    let replacing = !search.replacing.get_untracked();
    search.replacing.set(replacing);
    let target = if replacing {
        search.replace_ref.get_untracked()
    } else {
        search.query_ref.get_untracked()
    };
    if let Some(row) = search.replace_row_ref.get_untracked() {
        row.set_hidden(!replacing);
    }
    if let Some(input) = target {
        let _ = crate::focus::in_view(&input);
    }
}

/// Keys anywhere in the widget: Escape closes it, `Alt+C`/`W`/`R` flip
/// the options. (`Ctrl+F`, `Ctrl+H` and `F3` reach the editor root.)
fn widget_keys(state: EditorState, event: &KeyboardEvent) {
    if crate::ime::composing(event) {
        return;
    }
    if event.key() == "Escape" {
        event.prevent_default();
        event.stop_propagation();
        search::close(state);
        return;
    }
    if !event.alt_key() || event.ctrl_key() || event.meta_key() {
        return;
    }
    let flip: fn(&mut SearchOptions) = match event.code().as_str() {
        "KeyC" => flip_case,
        "KeyW" => flip_word,
        "KeyR" => flip_regex,
        _ => return,
    };
    event.prevent_default();
    search::toggle_option(state, flip);
}

/// In the query: Enter for the next match, Shift+Enter for the previous,
/// Ctrl+Alt+Enter replaces all.
fn query_keys(state: EditorState, event: &KeyboardEvent) {
    if crate::ime::composing(event) {
        return;
    }
    if event.key() != "Enter" {
        return;
    }
    event.prevent_default();
    if replace_all_chord(event) {
        search::replace_all(state);
    } else {
        search::step(state, event.shift_key());
    }
}

/// In the replacement: Enter replaces the current match, Ctrl+Alt+Enter
/// all of them.
fn replace_keys(state: EditorState, event: &KeyboardEvent) {
    if crate::ime::composing(event) {
        return;
    }
    if event.key() != "Enter" {
        return;
    }
    event.prevent_default();
    if replace_all_chord(event) {
        search::replace_all(state);
    } else {
        search::replace_current(state);
    }
}

fn replace_all_chord(event: &KeyboardEvent) -> bool {
    (event.ctrl_key() || event.meta_key()) && event.alt_key()
}

/// The widget's height cap: the part of the text area inside the window
/// (the stylesheet's 100% of the text area, when all of it is shown), so a
/// text area that runs past the window's bottom keeps the widget, which
/// scrolls inside itself, in the window. Measured again as the window or
/// the page scrolls or resizes.
fn room(state: EditorState) -> String {
    let search = state.search();
    if !search.open.get() {
        return String::new();
    }
    state.view().layout_revision.track();
    let Some(area) = search
        .widget_ref
        .get()
        .and_then(|widget| widget.parent_element())
    else {
        return String::new();
    };
    let window_height = web_sys::window()
        .and_then(|window| window.inner_height().ok())
        .and_then(|height| height.as_f64())
        .unwrap_or(f64::INFINITY);
    let rect = area.get_bounding_client_rect();
    let shown = rect.bottom().min(window_height) - rect.top().max(0.0);
    if shown >= rect.height() {
        return String::new();
    }
    format!("{}px", shown.max(0.0).floor())
}
