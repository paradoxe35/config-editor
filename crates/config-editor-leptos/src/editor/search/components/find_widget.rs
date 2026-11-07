//! The find widget over the top right of the text: the query with its
//! match options, the match count, previous/next/close, and (for `Ctrl+H`)
//! the replace row.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::super::state::EditorState;
use super::super::{self as search, SearchOptions};

#[component]
pub(crate) fn FindWidget(state: EditorState) -> impl IntoView {
    let search = state.search;
    let editable = move || !state.config.read_only.get();
    let invalid = move || search.results.with(|results| results.error.is_some());
    view! {
        <div
            class="config-editor__find"
            role="search"
            aria-label="Find and replace"
            node_ref=search.widget_ref
            prop:hidden=move || !search.open.get()
            on:keydown=move |event| widget_keys(state, &event)
        >
            <div class="config-editor__find-row">
                <button
                    type="button"
                    class="config-editor__find-button config-editor__find-expand"
                    class:config-editor__find-expand-open=move || search.replacing.get()
                    aria-label="Toggle replace"
                    title="Toggle replace (Ctrl+H)"
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
                        aria-label="Find"
                        placeholder="Find"
                        spellcheck="false"
                        autocomplete="off"
                        aria-invalid=move || invalid().to_string()
                        title=move || search.results.with(|results| results.error.clone())
                        node_ref=search.query_ref
                        prop:value=move || search.query.get()
                        on:input=move |event| query_input(state, event_target_value(&event))
                        on:keydown=move |event| query_keys(state, &event)
                    />
                    <OptionToggle state=state label="Match case" text="Aa" shortcut="Alt+C" read=|options| options.case_sensitive flip=flip_case />
                    <OptionToggle state=state label="Match whole word" text="ab" shortcut="Alt+W" read=|options| options.whole_word flip=flip_word />
                    <OptionToggle state=state label="Use regular expression" text=".*" shortcut="Alt+R" read=|options| options.regex flip=flip_regex />
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
                        aria-label="Previous match"
                        title="Previous match (Shift+Enter)"
                        on:click=move |_| search::step(state, true)
                    >
                        "↑"
                    </button>
                    <button
                        type="button"
                        class="config-editor__find-button"
                        aria-label="Next match"
                        title="Next match (Enter)"
                        on:click=move |_| search::step(state, false)
                    >
                        "↓"
                    </button>
                    <button
                        type="button"
                        class="config-editor__find-button"
                        aria-label="Close"
                        title="Close (Escape)"
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
                    aria-label="Replace"
                    placeholder="Replace"
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
                    title="Replace (Enter)"
                    on:click=move |_| search::replace_current(state)
                >
                    "Replace"
                </button>
                <button
                    type="button"
                    class="config-editor__find-button config-editor__find-text-button"
                    aria-label="Replace all"
                    title="Replace all (Ctrl+Alt+Enter)"
                    on:click=move |_| search::replace_all(state)
                >
                    "All"
                </button>
            </div>
        </div>
    }
}

/// A match option as a pressed/unpressed button in the query field.
#[component]
fn OptionToggle(
    state: EditorState,
    label: &'static str,
    text: &'static str,
    shortcut: &'static str,
    read: fn(&SearchOptions) -> bool,
    flip: fn(&mut SearchOptions),
) -> impl IntoView {
    let pressed = move || state.search.options.with(read);
    view! {
        <button
            type="button"
            class="config-editor__find-toggle"
            class:config-editor__find-toggle-on=pressed
            aria-label=label
            title=format!("{label} ({shortcut})")
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
    let search = state.search;
    if let Some(notice) = search.notice.get() {
        return notice;
    }
    let query_empty = search.query.with(String::is_empty);
    let current = search.current.get();
    search
        .results
        .with(|results| search::count_label(results, current, query_empty))
}

fn query_input(state: EditorState, value: String) {
    state.search.query.set(value);
    search::select_nearest(state, true);
}

fn toggle_replace(state: EditorState) {
    let search = state.search;
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
        let _ = input.focus();
    }
}

/// Keys anywhere in the widget: Escape closes it, `Alt+C`/`W`/`R` flip
/// the options. (`Ctrl+F`, `Ctrl+H` and `F3` reach the editor root.)
fn widget_keys(state: EditorState, event: &KeyboardEvent) {
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
