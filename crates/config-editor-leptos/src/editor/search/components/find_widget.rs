//! The find widget over the top right of the text: the query with its
//! match options, the match count, previous/next/close, and (for `Ctrl+H`)
//! the replace row.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::super::state::EditorState;
use super::super::{self as search, SearchOptions};
use crate::EditorStrings;

#[component]
pub(crate) fn FindWidget(state: EditorState) -> impl IntoView {
    let search = state.search();
    let editable = move || !state.config().read_only.get();
    let invalid = move || search.results.with(|results| results.error.is_some());
    view! {
        <div
            class="config-editor__find"
            role="search"
            aria-label=state.text(|strings| &strings.find_widget)
            node_ref=search.widget_ref
            prop:hidden=move || !search.open.get()
            on:keydown=move |event| widget_keys(state, &event)
        >
            <div class="config-editor__find-row">
                <button
                    type="button"
                    class="config-editor__find-button config-editor__find-expand"
                    class:config-editor__find-expand-open=move || search.replacing.get()
                    aria-label=state.text(|strings| &strings.toggle_replace)
                    title=state.text(|strings| &strings.toggle_replace_title)
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
                        on:click=move |_| search::step(state, true)
                    >
                        "↑"
                    </button>
                    <button
                        type="button"
                        class="config-editor__find-button"
                        aria-label=state.text(|strings| &strings.next_match)
                        title=state.text(|strings| &strings.next_match_title)
                        on:click=move |_| search::step(state, false)
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
                    on:click=move |_| search::replace_current(state)
                >
                    {state.text(|strings| &strings.replace)}
                </button>
                <button
                    type="button"
                    class="config-editor__find-button config-editor__find-text-button"
                    aria-label=state.text(|strings| &strings.replace_all_label)
                    title=state.text(|strings| &strings.replace_all_title)
                    on:click=move |_| search::replace_all(state)
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

fn query_input(state: EditorState, value: String) {
    state.search().query.set(value);
    search::select_nearest(state, true);
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
        let _ = crate::focus::in_place(&input);
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
