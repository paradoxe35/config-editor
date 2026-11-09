//! Keeping the editor and its host in step: the controlled `value`, the
//! debounced copy validation reads, and Edit-as (converting edits back to
//! the host's language, and switching the language on screen).

use super::line_endings::LineEndings;
use leptos::prelude::*;

use std::sync::Arc;

use super::bridge::{self, Bridge};
use super::codec;
use super::diagnostics::{DIAGNOSTICS_DEBOUNCE_MS, SchemaBundle};
use super::edit::{Edit, Origin};
use super::edit_as::{EditAsProblem, EditAsText, SaveError};
use super::history::EditHistory;
use super::language::ConfigLanguage;
use super::languages::Language;
use super::paths::TextPosition;
use super::state::{EditorState, normalize_newlines, textarea_selection};
use crate::reactive::later;

/// The host's side of the connection (from the component's props).
pub(super) struct Host {
    pub(super) value: Option<Signal<String>>,
    pub(super) edit_as: MaybeProp<Language>,
    pub(super) on_edit_as_change: Option<Callback<Language>>,
    /// A switch the document could not make when mounted.
    pub(super) initial_pending: Option<Language>,
    pub(super) on_sync_change: Option<Callback<bool>>,
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
    /// The host text the edit session began on: the last one received (or
    /// written in the host's language), not one an Edit-as save wrote. Its
    /// styles outlast the session's saves (see `Bridge::to_host_since`).
    pub(super) session_host: StoredValue<String>,
    /// The edited text matching `host_text`.
    pub(super) synced_edit: StoredValue<String>,
    /// Edited texts already saved, each with its language and the host text
    /// it saved as: an undo back to one (across a switch too) restores that
    /// host exactly, comments a fresh save could not bring back included.
    /// Cleared when the host's text changes.
    pub(super) synced_hosts: StoredValue<Vec<(Language, String, String)>>,
    /// Told the language on screen after each switch (and undo or redo back
    /// across one); set when the editor connects.
    pub(super) on_edit_as_change: StoredValue<Option<Callback<Language>>>,
    /// An `edit_as` switch waiting until the document can convert.
    pub(super) pending_switch: RwSignal<Option<Language>>,
    /// The last host text converted for display: (language, host, shown).
    pub(super) shown_cache: StoredValue<Option<(Language, String, String)>>,
    /// Edit-as problems of the text shown (values it cannot show, a
    /// switch that waits).
    pub(super) load_problems: RwSignal<Vec<EditAsProblem>>,
    /// Edit-as problems of the last save (values the host cannot keep, a
    /// save refused).
    pub(super) conversion_problems: RwSignal<Vec<EditAsProblem>>,
    /// The host's line endings, kept for what it hears (see `line_endings`).
    pub(super) line_endings: LineEndings,
    /// Whether the text on screen is written back to the host's text: false
    /// while an Edit-as edit cannot convert (see [`EditorState::note_in_sync`]).
    pub(super) in_sync: RwSignal<bool>,
}

impl SyncState {
    /// The host's text loaded for editing as `edit_language`: the state,
    /// the text to show, and a switch to retry once the document can
    /// convert (until then it is shown in the host language). With
    /// `opening`, a text in `edit_language` the host kept from an earlier
    /// session, that text is shown instead (in `edit_language` even when the
    /// host's text cannot be), and converts back like an edit.
    pub(super) fn load(
        host_language: Language,
        edit_language: Language,
        schema: StoredValue<Option<Arc<SchemaBundle>>>,
        host_text: String,
        opening: Option<String>,
        host_on_change: Option<Callback<String>>,
        line_endings: LineEndings,
    ) -> (Self, String, Option<Language>) {
        let mut load_problems = Vec::new();
        let mut pending = None;
        let mut shown = host_text.clone();
        let mut synced = None;
        let mut in_sync = true;
        if let Some(bridge) = Bridge::new(host_language, edit_language, schema.get_value()) {
            match bridge.to_edit(&host_text) {
                Ok(conversion) => {
                    load_problems = bridge.view_problems(&conversion);
                    shown = conversion.text;
                }
                Err(_) => pending = Some(edit_language),
            }
            if let Some(opening) = opening.map(normalize_newlines) {
                // The host's text as shown is what the opening text is
                // compared with; one that cannot be shown matches no text.
                synced = Some(if pending.is_some() {
                    host_text.clone()
                } else {
                    std::mem::take(&mut shown)
                });
                in_sync = synced.as_deref() == Some(opening.as_str())
                    || bridge
                        .to_host_since(&opening, &host_text, &host_text)
                        .is_ok();
                pending = None;
                load_problems = Vec::new();
                shown = opening;
            }
        }
        let synced = synced.unwrap_or_else(|| shown.clone());
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
                synced.clone(),
            ))),
            session_host: StoredValue::new(host_text.clone()),
            host_text: StoredValue::new(host_text),
            synced_edit: StoredValue::new(synced),
            synced_hosts: StoredValue::new(Vec::new()),
            on_edit_as_change: StoredValue::new(None),
            pending_switch: RwSignal::new(None),
            load_problems: RwSignal::new(load_problems),
            conversion_problems: RwSignal::new(Vec::new()),
            line_endings,
            in_sync: RwSignal::new(in_sync),
        };
        (sync, shown, pending)
    }
}

impl EditorState {
    /// The edit pipeline's notify stage: `on_change` hears of the document
    /// (while editing in the host's language; otherwise the conversion
    /// does, after the typing pause).
    pub(super) fn notify_change(&self) {
        if let Some(on_change) = self.sync().on_change {
            on_change.run(self.doc().text.get_untracked());
        }
    }

    /// `host` as shown in the language on screen; a text that cannot convert
    /// is shown as is, with the problem reported.
    pub(super) fn shown(&self, host: &str) -> String {
        let Some(bridge) = self.bridge() else {
            return host.to_string();
        };
        match bridge.to_edit(host) {
            Ok(conversion) => {
                self.sync()
                    .load_problems
                    .set(bridge.view_problems(&conversion));
                conversion.text
            }
            Err(error) => {
                self.sync()
                    .load_problems
                    .set(vec![bridge.load_problem(&error)]);
                host.to_string()
            }
        }
    }

    /// The language on screen.
    pub(super) fn behavior(&self) -> &'static dyn ConfigLanguage {
        self.sync().edit_language.get_untracked().behavior()
    }

    /// The bridge between the host and the language on screen, when they
    /// differ.
    pub(super) fn bridge(&self) -> Option<Bridge> {
        self.bridge_to(self.sync().edit_language.get_untracked())
    }

    pub(super) fn bridge_to(&self, edit: Language) -> Option<Bridge> {
        Bridge::new(
            self.config().host_language,
            edit,
            self.config().schema.get_value(),
        )
    }

    pub(super) fn label(&self) -> &'static str {
        self.sync().edit_language.get().label()
    }

    /// Puts `language` on screen for `edit` (an Edit-as switch, or an undo
    /// or redo back across one), in the pipeline's apply stage: the popups
    /// close, validation and the conversion read the new text in it at once.
    /// Back in the host's language by undo, the text is the host's own again
    /// (`on_change` hears it); in another, the conversion finds the host text
    /// it was saved as.
    pub(super) fn show_language(&self, language: Language, edit: &Edit) {
        self.sync().pending_switch.set(None);
        self.hide_hover();
        self.close_suggestions();
        if edit.origin != Origin::Switch {
            let host = self.sync().host_text.get_value();
            match self.bridge_to(language) {
                None => {
                    self.sync().synced_edit.set_value(edit.text.clone());
                    self.sync().host_text.set_value(edit.text.clone());
                    self.sync().session_host.set_value(edit.text.clone());
                    self.sync().load_problems.set(Vec::new());
                    self.note_in_sync(true);
                }
                Some(bridge) => self.sync().load_problems.set(
                    bridge
                        .to_edit(&host)
                        .map(|shown| bridge.view_problems(&shown))
                        .unwrap_or_default(),
                ),
            }
        }
        self.sync().edit_language.set(language);
        self.problems().diagnosed_text.set(edit.text.clone());
    }

    /// Records whether the text on screen is written back to the host's
    /// text: after each conversion of an Edit-as edit, a new host text, and
    /// a switch or undo back into a language whose text is the host's.
    pub(super) fn note_in_sync(&self, in_sync: bool) {
        let signal = self.sync().in_sync;
        if signal.get_untracked() != in_sync {
            signal.set(in_sync);
        }
    }

    /// Tells the host the language on screen changed (after the edit that
    /// changed it is made).
    pub(super) fn report_language(&self, language: Language) {
        if let Some(callback) = self.sync().on_edit_as_change.get_value() {
            callback.run(language);
        }
    }
}

pub(super) fn connect(state: EditorState, host: Host) {
    state
        .sync()
        .on_edit_as_change
        .set_value(host.on_edit_as_change);
    if let Some(value) = host.value {
        follow_value(state, value);
    }
    debounce_validation(state);
    convert_edits(state);
    if let Some(callback) = host.on_sync_change {
        report_sync(state, callback);
    }
    save_on_unmount(state);
    switch_languages(state, host);
}

/// Tells the host whether the text on screen is written back to its text:
/// once on mount, then each time that changes (the state after a typing
/// pause's conversion, never the steps in between).
fn report_sync(state: EditorState, callback: Callback<bool>) {
    let in_sync = state.sync().in_sync;
    let shown = Memo::new(move |_| in_sync.get());
    Effect::new(move |_| callback.run(shown.get()));
}

/// A controlled `value` flows into the editor, at once: input the browser
/// sends right after a host sets the value (in the same task) lands on the
/// new text, not on the old one the effect would replace later.
fn follow_value(state: EditorState, value: Signal<String>) {
    leptos::reactive::effect::ImmediateEffect::new_scoped(move || {
        let value = value.get();
        state.sync().line_endings.note(&value);
        take_host_value(state, normalize_newlines(value));
    });
}

/// Shows `external` (normalized), a new text from the host.
pub(super) fn take_host_value(state: EditorState, external: String) {
    // Ignore the host echoing our own edit; reconverting would reformat
    // the text under the caret.
    if external == state.sync().host_text.get_value() {
        return;
    }
    state.sync().host_text.set_value(external.clone());
    state.sync().session_host.set_value(external.clone());
    let edit = state.shown(&external);
    state.sync().synced_edit.set_value(edit.clone());
    state.sync().synced_hosts.update_value(Vec::clear);
    state.note_in_sync(true);
    let current = state.doc().text.get_untracked();
    if current != edit {
        state.commit(Edit::replacing(current, edit, Origin::HostValue));
    }
}

/// Debounced so half-typed text does not flash squiggles; everything else
/// reads the live source.
fn debounce_validation(state: EditorState) {
    let token = StoredValue::new(0u64);
    Effect::new(move |_| {
        let text = state.doc().text.get();
        token.update_value(|token| *token += 1);
        let current = token.get_value();
        later::after(state, DIAGNOSTICS_DEBOUNCE_MS, move |state| {
            if token.get_value() == current {
                state.problems().diagnosed_text.set(text);
                // The pause in typing: the placeholders' fields too.
                state.recheck_tokens();
            }
        });
    });
}

/// While editing as another language, each pause in typing converts the
/// text back and sends it to the host when it converts.
fn convert_edits(state: EditorState) {
    Effect::new(move |_| {
        let text = state.problems().diagnosed_text.get();
        let language = state.sync().edit_language.get();
        // A text the document no longer is (the first run, after the host
        // replaced the text in the same task) is not an edit: converting it
        // would send the old data under the host's new text. The pause
        // after the new text converts that one.
        if state.doc().text.with_untracked(|current| *current != text) {
            return;
        }
        convert_now(state, text, language);
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
                    state.doc().text.try_get_untracked(),
                    state.sync().edit_language.try_get_untracked(),
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
            state.sync().conversion_problems.set(Vec::new());
            state.note_in_sync(true);
            return;
        };
        if text == state.sync().synced_edit.get_value() {
            state.sync().conversion_problems.set(Vec::new());
            state.note_in_sync(true);
            return;
        }
        remember_synced_as(
            state,
            language,
            state.sync().synced_edit.get_value(),
            state.sync().host_text.get_value(),
        );
        let previous = state.sync().host_text.get_value();
        let mut hosts_own = false;
        let origin = state.sync().session_host.get_value();
        let result = bridge
            .to_host_since(&text, &previous, &origin)
            .map(|mut conversion| {
                // Back to a text saved before (an undo), or the host's own (a
                // reloaded baseline): its host text exactly.
                if let Some(host) = state.sync().synced_hosts.with_value(|saved| {
                    saved
                        .iter()
                        .find(|(known, edit, _)| *known == language && *edit == text)
                        .map(|(_, _, host)| host.clone())
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
                .map_err(SaveError::ReadOnly)
        });
        state
            .sync()
            .conversion_problems
            .set(bridge.problems(&text, &result));
        state.note_in_sync(result.is_ok());
        if let Ok(conversion) = result {
            state.sync().synced_edit.set_value(text);
            if conversion.text != state.sync().host_text.get_value() {
                state.sync().host_text.set_value(conversion.text.clone());
                if let Some(on_change) = state.sync().host_on_change {
                    on_change.run(conversion.text);
                }
            }
        }
    }
}

/// Edited texts remembered with their host text (see `synced_hosts`).
const SYNCED_HOSTS_KEPT: usize = 64;

pub(super) fn remember_synced(state: EditorState, edit: String, host: String) {
    remember_synced_as(
        state,
        state.sync().edit_language.get_untracked(),
        edit,
        host,
    );
}

/// [`remember_synced`] for a text in `language`.
fn remember_synced_as(state: EditorState, language: Language, edit: String, host: String) {
    state.sync().synced_hosts.update_value(|saved| {
        saved.retain(|(known, text, _)| !(*known == language && *text == edit));
        saved.push((language, edit, host));
        if saved.len() > SYNCED_HOSTS_KEPT {
            saved.remove(0);
        }
    });
}

/// An edit still waiting out the typing pause is saved when the editor goes
/// away, so unmounting right after typing loses nothing; then the last parse
/// goes too (see `parse_cache`).
fn save_on_unmount(state: EditorState) {
    super::parse_cache::hold();
    on_cleanup(move || {
        save_pending(state);
        super::parse_cache::release();
    });
}

/// [`save_on_unmount`]'s save.
fn save_pending(state: EditorState) {
    let (Some(text), Some(synced), Some(host), Some(edit), Some(schema)) = (
        state.doc().text.try_get_untracked(),
        state.sync().synced_edit.try_get_value(),
        state.sync().host_text.try_get_value(),
        state.sync().edit_language.try_get_untracked(),
        state.config().schema.try_get_value(),
    ) else {
        return;
    };
    if text == synced {
        return;
    }
    let Some(bridge) = bridge::Bridge::new(state.config().host_language, edit, schema) else {
        return;
    };
    let origin = state
        .sync()
        .session_host
        .try_get_value()
        .unwrap_or_default();
    if let Ok(conversion) = bridge.to_host_since(&text, &host, &origin)
        && conversion.text != host
        && state.check_host_save(&host, &conversion.text).is_ok()
        && let Some(on_change) = state.sync().host_on_change
    {
        on_change.run(conversion.text);
    }
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
    let try_switch = move |next: Language| switch_to(state, next);
    if let Some(next) = initial_pending {
        try_switch(next);
    } else if let Some(callback) = on_edit_as_change
        && state.sync().edit_language.get_untracked() != state.config().host_language
    {
        // Mounted already editing as another language: that switch completed
        // too. Reported after mounting, so a host's callback can use the
        // editor it is creating.
        let edit = state.sync().edit_language.get_untracked();
        Effect::new(move |previous: Option<()>| {
            if previous.is_none() {
                callback.run(edit);
            }
        });
    }
    Effect::new(move |previous: Option<Language>| {
        let next = edit_as.get().unwrap_or(state.config().host_language);
        // Changed before this first ran (`setEditAs` right after mounting):
        // that is a switch too.
        let changed_before_mount = previous.is_none()
            && next != state.sync().edit_language.get_untracked()
            && state.sync().pending_switch.get_untracked() != Some(next);
        if changed_before_mount {
            try_switch(next);
        } else if previous.is_some()
            && (next == state.sync().edit_language.get_untracked()
                || state.sync().pending_switch.get_untracked() != Some(next))
        {
            // Not a switch already waiting for it (asked through the
            // handle, which tried it at once).
            state.ask_edit_as(next);
        }
        next
    });
    Effect::new(move |_| {
        state.problems().diagnosed_text.track();
        // Not on a step back through undo: the switch would end redo, and
        // the steps the user may redo would be lost. The next edit (which
        // ends redo anyway) makes it.
        if state.doc().history.with_untracked(EditHistory::can_redo) {
            return;
        }
        if let Some(next) = state.sync().pending_switch.get_untracked() {
            try_switch(next);
        }
    });
}

impl EditorState {
    /// Asks for `next` on screen: switches to it, or waits until the
    /// document can convert; the language already on screen ends a switch
    /// still waiting. Asked again for a language another one replaced (an
    /// undo back across the switch), it switches again.
    pub(super) fn ask_edit_as(&self, next: Language) {
        if next == self.sync().edit_language.get_untracked() {
            // Back to the current language: nothing to convert.
            if self.sync().pending_switch.get_untracked().is_some() {
                self.sync().pending_switch.set(None);
                self.sync().load_problems.set(Vec::new());
            }
        } else {
            switch_to(*self, next);
        }
    }
}

pub(super) fn switch_to(state: EditorState, next: Language) {
    let current = state.sync().edit_language.get_untracked();
    let host = state.config().host_language;
    // Either language unable to write documents: the switch could never
    // be saved (from the host's language) or shown (into the other), so it
    // is refused at once, not left pending.
    if next != host
        && let Some(error) = [host, next].into_iter().find_map(cannot_write)
    {
        state.sync().pending_switch.set(None);
        let text = state.doc().text.get_untracked();
        state.sync().load_problems.set(vec![EditAsProblem {
            at: bridge::locate_error(current.behavior(), &text, &SaveError::from(error.clone())),
            text: EditAsText::SwitchRefused {
                edit: next,
                error: error.to_string(),
            },
        }]);
        return;
    }
    let text = state.doc().text.get_untracked();
    let host_text = state.sync().host_text.get_value();
    let host_text_before = host_text.clone();
    let unsaved = text != state.sync().synced_edit.get_value();
    let mut view_warnings = Vec::new();
    let converted = if !unsaved {
        match state.bridge_to(next) {
            Some(bridge) => bridge
                .to_edit(&host_text)
                .map(|conversion| {
                    view_warnings = bridge.view_problems(&conversion);
                    conversion.text
                })
                .map_err(SaveError::from),
            None => Ok(host_text),
        }
    } else {
        match state
            .bridge_to(current)
            .filter(|_| next == state.config().host_language)
        {
            Some(bridge) => bridge
                .to_host_since(&text, &host_text, &state.sync().session_host.get_value())
                .and_then(|conversion| {
                    state
                        .check_host_save(&host_text, &conversion.text)
                        .map(|()| conversion.text)
                        .map_err(SaveError::ReadOnly)
                }),
            None => state.config().schema.with_value(|schema| {
                codec::convert(
                    &text,
                    current.behavior(),
                    next.behavior(),
                    None,
                    schema.as_deref().map(|bundle| &bundle.schema),
                )
                .map(|conversion| conversion.text)
                .map_err(SaveError::from)
            }),
        }
    };
    match converted {
        Ok(converted) => {
            if !unsaved {
                // Both texts stand for the host's: undo and redo across the
                // switch restore it exactly.
                remember_synced_as(state, current, text.clone(), host_text_before.clone());
                remember_synced_as(state, next, converted.clone(), host_text_before.clone());
                state.sync().synced_edit.set_value(converted.clone());
                state.sync().shown_cache.set_value(Some((
                    next,
                    host_text_before.clone(),
                    converted.clone(),
                )));
            } else if next == state.config().host_language {
                // Back to the host language with an edit still waiting out
                // the typing pause: the converted text is the host's now.
                state.sync().synced_edit.set_value(converted.clone());
                state.note_in_sync(true);
                if converted != host_text_before {
                    state.sync().host_text.set_value(converted.clone());
                    if let Some(on_change) = state.sync().host_on_change {
                        on_change.run(converted.clone());
                    }
                }
            }
            // The caret stays on the field it was on; undo puts the
            // selection back.
            let selection = state.textarea().map_or_else(
                || {
                    let caret = state.doc().caret.get_untracked();
                    (caret, caret)
                },
                |area| textarea_selection(&area),
            );
            let caret = field_caret(selection.0, (current, &text), (next, &converted));
            // Another language's lines: each fold is carried by its field; the
            // switch is an undo step back to this text (see
            // `edit::Origin::Switch`).
            state.commit(Edit::switching(text, converted, next, selection, caret));
            state.sync().load_problems.set(view_warnings);
        }
        Err(error) => {
            state.sync().pending_switch.set(Some(next));
            state.sync().load_problems.set(vec![EditAsProblem {
                at: bridge::locate_error(current.behavior(), &text, &error),
                text: EditAsText::CannotSwitch {
                    edit: next,
                    host: state.config().host_language,
                    error,
                },
            }]);
        }
    }
}

/// Why `language` cannot write documents, when it cannot (a language
/// registered with `parse` alone).
pub(super) fn cannot_write(language: Language) -> Option<codec::ConvertError> {
    language
        .serialize(&serde_json::Value::Object(serde_json::Map::new()))
        .err()
}

/// Where the field the caret is on in `before` (in its language) is written
/// in `after`: the caret's place after a switch. `None` when it is not there.
/// A caret inside the field's key keeps its place in the key.
fn field_caret(
    caret: usize,
    (from, before): (Language, &str),
    (to, after): (Language, &str),
) -> Option<usize> {
    let path = from.path_at(before, TextPosition::from_offset(before, caret))?;
    let start = to.range_of(after, path.clone())?.start.to_offset(after);
    let in_key = || {
        let pointer = path.to_pointer();
        let key = pointer
            .rsplit('/')
            .next()?
            .replace("~1", "/")
            .replace("~0", "~");
        if key.is_empty() || crate::editor::language::pointer_index(&key).is_some() {
            return None;
        }
        let from_start = from.range_of(before, path.clone())?.start.to_offset(before);
        let key_before = key_offset(before, from_start, &key)?;
        let column = caret
            .checked_sub(key_before)
            .filter(|column| *column <= key.encode_utf16().count())?;
        Some(key_offset(after, start, &key)? + column)
    };
    Some(in_key().unwrap_or(start))
}

/// The UTF-16 offset of `key` written on the line of UTF-16 offset `from`
/// in `text`, at or after it.
fn key_offset(text: &str, from: usize, key: &str) -> Option<usize> {
    let byte = super::keyboard::utf16_to_byte_index(text, from);
    let line = &text[byte..text[byte..].find('\n').map_or(text.len(), |end| byte + end)];
    let at = byte + line.find(key)?;
    Some(text[..at].encode_utf16().count())
}

pub(super) use super::text_change::map_through_change;
