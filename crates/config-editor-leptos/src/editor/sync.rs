//! Keeping the editor and its host in step: the controlled `value`, the
//! debounced copy validation reads, and Edit-as (converting edits back to
//! the host's language, and switching the language on screen).

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use std::sync::Arc;

use super::bridge::{self, Bridge};
use super::codec;
use super::diagnostics::{DIAGNOSTICS_DEBOUNCE_MS, Diagnostic, SchemaBundle};
use super::history::EditHistory;
use super::language::ConfigLanguage;
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

/// The host-and-Edit-as part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct SyncState {
    /// The language on screen; `edit_as` switches change it.
    pub(super) edit_language: RwSignal<Language>,
    /// Edits in the edit language; `None` while editing as another
    /// language (the conversion here reports them).
    pub(super) on_change: Option<Callback<String>>,
    pub(super) host_on_change: Option<Callback<String>>,
    /// Last host text received or sent.
    pub(super) host_text: StoredValue<String>,
    /// The edited text matching `host_text`.
    pub(super) synced_edit: StoredValue<String>,
    /// Edited texts already saved, each with the host text it saved as: an
    /// undo back to one restores that host exactly, comments a fresh save
    /// could not bring back included. Cleared when the host or the edit
    /// language changes.
    pub(super) synced_hosts: StoredValue<Vec<(String, String)>>,
    /// An `edit_as` switch waiting until the document can convert.
    pub(super) pending_switch: RwSignal<Option<Language>>,
    /// The last host text converted for display: (language, host, shown).
    pub(super) shown_cache: StoredValue<Option<(Language, String, String)>>,
    pub(super) load_problems: RwSignal<Vec<Diagnostic>>,
    pub(super) conversion_problems: RwSignal<Vec<Diagnostic>>,
}

impl SyncState {
    /// The host's text loaded for editing as `edit_language`: the state,
    /// the text to show, and a switch to retry once the document can
    /// convert (until then it is shown in the host language).
    pub(super) fn load(
        host_language: Language,
        edit_language: Language,
        schema: StoredValue<Option<Arc<SchemaBundle>>>,
        host_text: String,
        host_on_change: Option<Callback<String>>,
    ) -> (Self, String, Option<Language>) {
        let mut load_problems = Vec::new();
        let mut pending = None;
        let mut shown = host_text.clone();
        if let Some(bridge) = Bridge::new(host_language, edit_language, schema.get_value()) {
            match bridge.to_edit(&host_text) {
                Ok(conversion) => {
                    load_problems = bridge.view_problems(&conversion);
                    shown = conversion.text;
                }
                Err(_) => pending = Some(edit_language),
            }
        }
        let edit_language = RwSignal::new(if pending.is_some() {
            host_language
        } else {
            edit_language
        });
        let on_change = host_on_change.map(|callback| {
            Callback::new(move |text: String| {
                let bridge = Bridge::new(
                    host_language,
                    edit_language.get_untracked(),
                    schema.get_value(),
                );
                if bridge.is_none() {
                    callback.run(text);
                }
            })
        });
        let sync = Self {
            edit_language,
            on_change,
            host_on_change,
            // The baseline as shown, in the language on screen.
            shown_cache: StoredValue::new(Some((
                edit_language.get_untracked(),
                host_text.clone(),
                shown.clone(),
            ))),
            host_text: StoredValue::new(host_text),
            synced_edit: StoredValue::new(shown.clone()),
            synced_hosts: StoredValue::new(Vec::new()),
            pending_switch: RwSignal::new(None),
            load_problems: RwSignal::new(load_problems),
            conversion_problems: RwSignal::new(Vec::new()),
        };
        (sync, shown, pending)
    }
}

impl EditorState {
    /// `host` as shown in the language on screen; a text that cannot convert
    /// is shown as is, with the problem reported.
    pub(super) fn shown(&self, host: &str) -> String {
        let Some(bridge) = self.bridge() else {
            return host.to_string();
        };
        match bridge.to_edit(host) {
            Ok(conversion) => {
                self.sync
                    .load_problems
                    .set(bridge.view_problems(&conversion));
                conversion.text
            }
            Err(error) => {
                self.sync
                    .load_problems
                    .set(vec![bridge.load_problem(&error)]);
                host.to_string()
            }
        }
    }

    /// The language on screen.
    pub(super) fn behavior(&self) -> &'static dyn ConfigLanguage {
        self.sync.edit_language.get_untracked().behavior()
    }

    /// The bridge between the host and the language on screen, when they
    /// differ.
    pub(super) fn bridge(&self) -> Option<Bridge> {
        self.bridge_to(self.sync.edit_language.get_untracked())
    }

    pub(super) fn bridge_to(&self, edit: Language) -> Option<Bridge> {
        Bridge::new(
            self.config.host_language,
            edit,
            self.config.schema.get_value(),
        )
    }

    pub(super) fn label(&self) -> &'static str {
        self.sync.edit_language.get().label()
    }
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
        take_host_value(state, normalize_newlines(value.get()));
    });
}

/// Shows `external` (normalized), a new text from the host.
pub(super) fn take_host_value(state: EditorState, external: String) {
    // Ignore the host echoing our own edit; reconverting would reformat
    // the text under the caret.
    if external == state.sync.host_text.get_value() {
        return;
    }
    state.sync.host_text.set_value(external.clone());
    let edit = state.shown(&external);
    state.sync.synced_edit.set_value(edit.clone());
    state.sync.synced_hosts.update_value(Vec::clear);
    let current = state.doc.text.get_untracked();
    if current != edit {
        // Keep the caret on the same text and the view where it was:
        // write the new value here and map the caret through the change,
        // so the later `prop:value` (identical text) leaves the selection
        // alone. Writing the value scrolls the textarea, so the scroll
        // position is put back.
        let kept = state.textarea().map(|element| {
            let selection = textarea_selection(&element);
            (selection, element.scroll_top(), element.scroll_left())
        });
        state.write_text(edit.clone());
        if let (Some(element), Some(((start, end), top, left))) = (state.textarea(), kept) {
            let map = |offset: usize| map_through_change(&current, &edit, offset);
            let _ = element.set_selection_range(map(start) as u32, map(end) as u32);
            element.set_scroll_top(top);
            element.set_scroll_left(left);
            state.view.scroll_x.set(f64::from(element.scroll_left()));
            state.view.scroll_y.set(f64::from(element.scroll_top()));
        }
        state.redetect_indent();
        // A new text from the host: its ranges and placeholders anew. Not
        // on an echo of the editor's own edit, where they are followed
        // (a pasted look-alike there never becomes the host's).
        state.place_ranges();
        state.rescan_tokens(true);
    }
}

/// Debounced so half-typed text does not flash squiggles; everything else
/// reads the live source.
fn debounce_validation(state: EditorState) {
    let token = StoredValue::new(0u64);
    Effect::new(move |_| {
        let text = state.doc.text.get();
        token.update_value(|token| *token += 1);
        let current = token.get_value();
        let diagnosed = state.problems.diagnosed_text;
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
        let text = state.problems.diagnosed_text.get();
        convert_now(state, text, state.sync.edit_language.get());
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
                    state.doc.text.try_get_untracked(),
                    state.sync.edit_language.try_get_untracked(),
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
pub(super) fn convert_now(state: EditorState, text: String, language: Language) {
    {
        let Some(bridge) = state.bridge_to(language) else {
            state.sync.conversion_problems.set(Vec::new());
            return;
        };
        if text == state.sync.synced_edit.get_value() {
            state.sync.conversion_problems.set(Vec::new());
            return;
        }
        remember_synced(
            state,
            state.sync.synced_edit.get_value(),
            state.sync.host_text.get_value(),
        );
        let previous = state.sync.host_text.get_value();
        let mut hosts_own = false;
        let result = bridge.to_host(&text, &previous).map(|mut conversion| {
            // Back to a text saved before (an undo), or the host's own (a
            // reloaded baseline): its host text exactly.
            if let Some(host) = state.sync.synced_hosts.with_value(|saved| {
                saved
                    .iter()
                    .find(|(edit, _)| *edit == text)
                    .map(|(_, host)| host.clone())
            }) {
                conversion.text = host;
                hosts_own = true;
            }
            conversion
        });
        // However the edit got there, a save never changes a read-only
        // value: it is refused like any other unsafe save. A text the host
        // itself had is its own to restore.
        let result = result.and_then(|conversion| {
            if hosts_own {
                return Ok(conversion);
            }
            state
                .check_host_save(&previous, &conversion.text)
                .map(|()| conversion)
                .map_err(codec::ConvertError::Unsupported)
        });
        state
            .sync
            .conversion_problems
            .set(bridge.problems(&text, &result));
        if let Ok(conversion) = result {
            state.sync.synced_edit.set_value(text);
            if conversion.text != state.sync.host_text.get_value() {
                state.sync.host_text.set_value(conversion.text.clone());
                if let Some(on_change) = state.sync.host_on_change {
                    on_change.run(conversion.text);
                }
            }
        }
    }
}

/// Edited texts remembered with their host text (see `synced_hosts`).
const SYNCED_HOSTS_KEPT: usize = 64;

pub(super) fn remember_synced(state: EditorState, edit: String, host: String) {
    state.sync.synced_hosts.update_value(|saved| {
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
            state.doc.text.try_get_untracked(),
            state.sync.synced_edit.try_get_value(),
            state.sync.host_text.try_get_value(),
            state.sync.edit_language.try_get_untracked(),
            state.config.schema.try_get_value(),
        ) else {
            return;
        };
        if text == synced {
            return;
        }
        let Some(bridge) = bridge::Bridge::new(state.config.host_language, edit, schema) else {
            return;
        };
        if let Ok(conversion) = bridge.to_host(&text, &host)
            && conversion.text != host
            && state.check_host_save(&host, &conversion.text).is_ok()
            && let Some(on_change) = state.sync.host_on_change
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
        && state.sync.edit_language.get_untracked() != state.config.host_language
    {
        // Mounted already editing as another language: that switch completed
        // too. Reported after mounting, so a host's callback can use the
        // editor it is creating.
        let edit = state.sync.edit_language.get_untracked();
        Effect::new(move |previous: Option<()>| {
            if previous.is_none() {
                callback.run(edit);
            }
        });
    }
    Effect::new(move |previous: Option<Language>| {
        let next = edit_as.get().unwrap_or(state.config.host_language);
        // Changed before this first ran (`setEditAs` right after mounting):
        // that is a switch too.
        let changed_before_mount = previous.is_none()
            && next != state.sync.edit_language.get_untracked()
            && state.sync.pending_switch.get_untracked() != Some(next);
        if changed_before_mount {
            try_switch(next);
        } else if previous.is_some() {
            if next == state.sync.edit_language.get_untracked() {
                // Back to the current language: nothing to convert.
                if state.sync.pending_switch.get_untracked().is_some() {
                    state.sync.pending_switch.set(None);
                    state.sync.load_problems.set(Vec::new());
                }
            } else {
                try_switch(next);
            }
        }
        next
    });
    Effect::new(move |_| {
        state.problems.diagnosed_text.track();
        // Not on a step back through undo: the switch starts the history
        // over, and the steps the user may redo would be lost. The next edit
        // (which ends redo anyway) makes it.
        if state.doc.history.with_untracked(EditHistory::can_redo) {
            return;
        }
        if let Some(next) = state.sync.pending_switch.get_untracked() {
            try_switch(next);
        }
    });
}

fn switch_to(state: EditorState, next: Language, on_edit_as_change: Option<Callback<Language>>) {
    let current = state.sync.edit_language.get_untracked();
    let text = state.doc.text.get_untracked();
    let host_text = state.sync.host_text.get_value();
    let host_text_before = host_text.clone();
    state.sync.synced_hosts.update_value(Vec::clear);
    let unsaved = text != state.sync.synced_edit.get_value();
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
            .filter(|_| next == state.config.host_language)
        {
            Some(bridge) => bridge.to_host(&text, &host_text).and_then(|conversion| {
                state
                    .check_host_save(&host_text, &conversion.text)
                    .map(|()| conversion.text)
                    .map_err(codec::ConvertError::Unsupported)
            }),
            None => state.config.schema.with_value(|schema| {
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
                state.sync.synced_edit.set_value(converted.clone());
                state.sync.shown_cache.set_value(Some((
                    next,
                    host_text_before.clone(),
                    converted.clone(),
                )));
            } else if next == state.config.host_language {
                // Back to the host language with an edit still waiting out
                // the typing pause: the converted text is the host's now.
                state.sync.synced_edit.set_value(converted.clone());
                if converted != host_text_before {
                    state.sync.host_text.set_value(converted.clone());
                    if let Some(on_change) = state.sync.host_on_change {
                        on_change.run(converted.clone());
                    }
                }
            }
            state.sync.pending_switch.set(None);
            state.sync.load_problems.set(view_warnings);
            state.doc.history.set(EditHistory::default());
            state.hide_hover();
            state.close_suggestions();
            state.sync.edit_language.set(next);
            if let Some(callback) = on_edit_as_change {
                callback.run(next);
            }
            state.problems.diagnosed_text.set(converted.clone());
            // Another language's lines: the folds do not carry over.
            state.fold.folded.set(Vec::new());
            state.write_text(converted);
            state.redetect_indent();
            state.place_ranges();
            state.rescan_tokens(false);
        }
        Err(error) => {
            state.sync.pending_switch.set(Some(next));
            let message = format!("Cannot switch to {next} yet: {error}");
            state.sync.load_problems.set(vec![bridge::locate_error(
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
