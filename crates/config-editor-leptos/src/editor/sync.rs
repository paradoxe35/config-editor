//! Keeping the editor and its host in step: the controlled `value`, the
//! debounced copy validation reads, and Edit-as (converting edits back to
//! the host's language, and switching the language on screen).

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::bridge;
use super::codec;
use super::diagnostics::DIAGNOSTICS_DEBOUNCE_MS;
use super::history::EditHistory;
use super::languages::Language;
use super::state::{EditorState, normalize_newlines, textarea_selection};

/// The host's side of the connection (from the component's props).
pub(super) struct Host {
    pub(super) value: Option<Signal<String>>,
    pub(super) edit_as: MaybeProp<Language>,
    pub(super) on_edit_as_change: Option<Callback<Language>>,
    /// A switch the document could not make when mounted.
    pub(super) initial_pending: Option<Language>,
}

pub(super) fn connect(state: EditorState, host: Host) {
    if let Some(value) = host.value {
        follow_value(state, value);
    }
    debounce_validation(state);
    convert_edits(state);
    save_on_unmount(state);
    switch_languages(state, host);
}

/// A controlled `value` flows into the editor, at once: input the browser
/// sends right after a host sets the value (in the same task) lands on the
/// new text, not on the old one the effect would replace later.
fn follow_value(state: EditorState, value: Signal<String>) {
    leptos::reactive::effect::ImmediateEffect::new_scoped(move || {
        let external = normalize_newlines(value.get());
        // Ignore the host echoing our own edit; reconverting would reformat
        // the text under the caret.
        if external == state.host_text.get_value() {
            return;
        }
        state.host_text.set_value(external.clone());
        let edit = state.shown(&external);
        state.synced_edit.set_value(edit.clone());
        state.synced_hosts.update_value(Vec::clear);
        let current = state.source.get_untracked();
        if current != edit {
            // Keep the caret on the same text: write the new value here and
            // map the caret through the change, so the later `prop:value`
            // (identical text) leaves the selection alone.
            if let Some(element) = state.textarea() {
                let (start, end) = textarea_selection(&element);
                let map = |offset: usize| map_through_change(&current, &edit, offset);
                element.set_value(&edit);
                let _ = element.set_selection_range(map(start) as u32, map(end) as u32);
            }
            state.source.set(edit);
            state.redetect_indent();
        }
        state.place_ranges();
    });
}

/// Debounced so half-typed text does not flash squiggles; everything else
/// reads the live source.
fn debounce_validation(state: EditorState) {
    let token = StoredValue::new(0u64);
    Effect::new(move |_| {
        let text = state.source.get();
        token.update_value(|token| *token += 1);
        let current = token.get_value();
        let diagnosed = state.diagnosed_text;
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                wasm_bindgen::closure::Closure::once_into_js(move || {
                    // The editor may have been unmounted while this waited.
                    if token.try_get_value() == Some(current) {
                        let _ = diagnosed.try_set(text);
                    }
                })
                .unchecked_ref(),
                DIAGNOSTICS_DEBOUNCE_MS,
            );
        } else {
            diagnosed.set(text);
        }
    });
}

/// While editing as another language, each pause in typing converts the
/// text back and sends it to the host when it converts.
fn convert_edits(state: EditorState) {
    Effect::new(move |_| {
        let text = state.diagnosed_text.get();
        convert_now(state, text, state.edit_language.get());
    });
    // `flush_pending_edits` converts an edit still waiting out the pause.
    let id = NEXT_EDITOR.with(|next| {
        let id = next.get();
        next.set(id + 1);
        id
    });
    CONVERSIONS.with(|conversions| {
        conversions.borrow_mut().push((
            id,
            std::rc::Rc::new(move || {
                if let (Some(text), Some(language)) = (
                    state.source.try_get_untracked(),
                    state.edit_language.try_get_untracked(),
                ) {
                    convert_now(state, text, language);
                }
            }),
        ));
    });
    on_cleanup(move || {
        let _ = CONVERSIONS
            .try_with(|conversions| conversions.borrow_mut().retain(|(known, _)| *known != id));
    });
}

/// An editor's id and its way to convert its pending edit now.
type Conversion = (u64, std::rc::Rc<dyn Fn()>);

thread_local! {
    static NEXT_EDITOR: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Each mounted editor's way to convert its pending edit now.
    static CONVERSIONS: std::cell::RefCell<Vec<Conversion>> = std::cell::RefCell::new(Vec::new());
}

/// Converts edits made in another language (Edit-as) that are still waiting
/// out the typing pause and sends them to the host now, in every editor.
/// Call it before reading the host's document from code (a Save button) or
/// replacing it, so neither works on the text before the last keystrokes.
pub fn flush_pending_edits() {
    super::input::flush_pending_input();
    // Cloned out first: a conversion's `onChange` may mount or unmount
    // editors, which changes the list.
    let conversions: Vec<std::rc::Rc<dyn Fn()>> = CONVERSIONS.with(|conversions| {
        conversions
            .borrow()
            .iter()
            .map(|(_, convert)| convert.clone())
            .collect()
    });
    for convert in conversions {
        convert();
    }
}

/// [`convert_edits`]' work for `text` edited as `language`.
fn convert_now(state: EditorState, text: String, language: Language) {
    {
        let Some(bridge) = state.bridge_to(language) else {
            state.conversion_problems.set(Vec::new());
            return;
        };
        if text == state.synced_edit.get_value() {
            state.conversion_problems.set(Vec::new());
            return;
        }
        remember_synced(
            state,
            state.synced_edit.get_value(),
            state.host_text.get_value(),
        );
        let previous = state.host_text.get_value();
        let result = bridge.to_host(&text, &previous).map(|mut conversion| {
            // Back to a text saved before (an undo): its host text exactly.
            if let Some(host) = state.synced_hosts.with_value(|saved| {
                saved
                    .iter()
                    .find(|(edit, _)| *edit == text)
                    .map(|(_, host)| host.clone())
            }) {
                conversion.text = host;
            }
            conversion
        });
        // However the edit got there, a save never changes a read-only
        // value: it is refused like any other unsafe save.
        let result = result.and_then(|conversion| {
            state
                .check_host_save(&previous, &conversion.text)
                .map(|()| conversion)
                .map_err(codec::ConvertError::Unsupported)
        });
        state
            .conversion_problems
            .set(bridge.problems(&text, &result));
        if let Ok(conversion) = result {
            state.synced_edit.set_value(text);
            if conversion.text != state.host_text.get_value() {
                state.host_text.set_value(conversion.text.clone());
                if let Some(on_change) = state.host_on_change {
                    on_change.run(conversion.text);
                }
            }
        }
    }
}

/// Edited texts remembered with their host text (see `synced_hosts`).
const SYNCED_HOSTS_KEPT: usize = 64;

fn remember_synced(state: EditorState, edit: String, host: String) {
    state.synced_hosts.update_value(|saved| {
        saved.retain(|(known, _)| *known != edit);
        saved.push((edit, host));
        if saved.len() > SYNCED_HOSTS_KEPT {
            saved.remove(0);
        }
    });
}

/// An edit still waiting out the typing pause is saved when the editor goes
/// away, so unmounting right after typing loses nothing.
fn save_on_unmount(state: EditorState) {
    on_cleanup(move || {
        let (Some(text), Some(synced), Some(host), Some(edit), Some(schema)) = (
            state.source.try_get_untracked(),
            state.synced_edit.try_get_value(),
            state.host_text.try_get_value(),
            state.edit_language.try_get_untracked(),
            state.schema.try_get_value(),
        ) else {
            return;
        };
        if text == synced {
            return;
        }
        let Some(bridge) = bridge::Bridge::new(state.host_language, edit, schema) else {
            return;
        };
        if let Ok(conversion) = bridge.to_host(&text, &host)
            && conversion.text != host
            && state.check_host_save(&host, &conversion.text).is_ok()
            && let Some(on_change) = state.host_on_change
        {
            on_change.run(conversion.text);
        }
    });
}

/// Switching `edit_as` converts the open document in place. Without unsaved
/// edits the text is rebuilt from the host's own text, so returning to
/// `language` restores it exactly (comments included); unsaved edits are
/// converted directly. Undo history is per language, so it starts over. A
/// switch that cannot convert yet (TOML has no `null`) stays pending and is
/// retried after every edit, so fixing the document completes it.
fn switch_languages(state: EditorState, host: Host) {
    let Host {
        edit_as,
        on_edit_as_change,
        initial_pending,
        ..
    } = host;
    let try_switch = move |next: Language| switch_to(state, next, on_edit_as_change);
    if let Some(next) = initial_pending {
        try_switch(next);
    } else if let Some(callback) = on_edit_as_change
        && state.edit_language.get_untracked() != state.host_language
    {
        // Mounted already editing as another language: that switch completed
        // too. Reported after mounting, so a host's callback can use the
        // editor it is creating.
        let edit = state.edit_language.get_untracked();
        Effect::new(move |previous: Option<()>| {
            if previous.is_none() {
                callback.run(edit);
            }
        });
    }
    Effect::new(move |previous: Option<Language>| {
        let next = edit_as.get().unwrap_or(state.host_language);
        // Changed before this first ran (`setEditAs` right after mounting):
        // that is a switch too.
        let changed_before_mount = previous.is_none()
            && next != state.edit_language.get_untracked()
            && state.pending_switch.get_untracked() != Some(next);
        if changed_before_mount {
            try_switch(next);
        } else if previous.is_some() {
            if next == state.edit_language.get_untracked() {
                // Back to the current language: nothing to convert.
                if state.pending_switch.get_untracked().is_some() {
                    state.pending_switch.set(None);
                    state.load_problems.set(Vec::new());
                }
            } else {
                try_switch(next);
            }
        }
        next
    });
    Effect::new(move |_| {
        state.diagnosed_text.track();
        // Not on a step back through undo: the switch starts the history
        // over, and the steps the user may redo would be lost. The next edit
        // (which ends redo anyway) makes it.
        if state.history.with_untracked(EditHistory::can_redo) {
            return;
        }
        if let Some(next) = state.pending_switch.get_untracked() {
            try_switch(next);
        }
    });
}

fn switch_to(state: EditorState, next: Language, on_edit_as_change: Option<Callback<Language>>) {
    let current = state.edit_language.get_untracked();
    let text = state.source.get_untracked();
    let host_text = state.host_text.get_value();
    let host_text_before = host_text.clone();
    state.synced_hosts.update_value(Vec::clear);
    let unsaved = text != state.synced_edit.get_value();
    let mut view_warnings = Vec::new();
    let converted = if !unsaved {
        match state.bridge_to(next) {
            Some(bridge) => bridge.to_edit(&host_text).map(|conversion| {
                view_warnings = bridge.view_problems(&conversion);
                conversion.text
            }),
            None => Ok(host_text),
        }
    } else {
        match state
            .bridge_to(current)
            .filter(|_| next == state.host_language)
        {
            Some(bridge) => bridge.to_host(&text, &host_text).and_then(|conversion| {
                state
                    .check_host_save(&host_text, &conversion.text)
                    .map(|()| conversion.text)
                    .map_err(codec::ConvertError::Unsupported)
            }),
            None => state.schema.with_value(|schema| {
                codec::convert(
                    &text,
                    current.behavior(),
                    next.behavior(),
                    None,
                    schema.as_deref().map(|bundle| &bundle.schema),
                )
                .map(|conversion| conversion.text)
            }),
        }
    };
    match converted {
        Ok(converted) => {
            if !unsaved {
                state.synced_edit.set_value(converted.clone());
                state.shown_cache.set_value(Some((
                    next,
                    host_text_before.clone(),
                    converted.clone(),
                )));
            } else if next == state.host_language {
                // Back to the host language with an edit still waiting out
                // the typing pause: the converted text is the host's now.
                state.synced_edit.set_value(converted.clone());
                if converted != host_text_before {
                    state.host_text.set_value(converted.clone());
                    if let Some(on_change) = state.host_on_change {
                        on_change.run(converted.clone());
                    }
                }
            }
            state.pending_switch.set(None);
            state.load_problems.set(view_warnings);
            state.history.set(EditHistory::default());
            state.hide_hover();
            state.close_suggestions();
            state.edit_language.set(next);
            if let Some(callback) = on_edit_as_change {
                callback.run(next);
            }
            state.diagnosed_text.set(converted.clone());
            state.source.set(converted);
            state.redetect_indent();
            state.place_ranges();
        }
        Err(error) => {
            state.pending_switch.set(Some(next));
            let message = format!("Cannot switch to {next} yet: {error}");
            state.load_problems.set(vec![bridge::locate_error(
                current.behavior(),
                &text,
                &error,
                message,
            )]);
        }
    }
}

/// Where UTF-16 `offset` in `old` lands in `new`, when one region between a
/// common prefix and suffix changed: before it stays, after it shifts,
/// inside it goes to the region's start.
pub(super) fn map_through_change(old: &str, new: &str, offset: usize) -> usize {
    let old: Vec<u16> = old.encode_utf16().collect();
    let new: Vec<u16> = new.encode_utf16().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    if offset <= prefix {
        offset
    } else if offset >= old.len() - suffix {
        offset + new.len() - old.len()
    } else {
        prefix
    }
}
