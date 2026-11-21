//! Keeping the editor and its host in step: the controlled `value`, the
//! debounced copy validation reads, and Edit-as (converting edits back to
//! the host's language, and switching the language on screen).

use super::line_endings::{LineEndings, editor_text};
use leptos::prelude::*;

use std::sync::Arc;

use super::bridge::links::{Clipboard, Links};
use super::bridge::{self, Bridge};
use super::codec;
use super::diagnostics::{DIAGNOSTICS_DEBOUNCE_MS, Placeholders, save_message, syntax_message};
use super::edit::{Edit, Origin};
use super::edit_as::{EditAsProblem, EditAsText, SaveError};
use super::history::{EditHistory, ViewState};
use super::language::ConfigLanguage;
use super::languages::Language;
use super::paths::TextPosition;
use super::schema::SchemaBundle;
use super::state::{EditorState, textarea_selection};
use crate::editor::text::{TextRange, TextSize, byte_of, utf16_of};
use crate::reactive::later;

mod echoes;
mod gate;
mod session;

pub use echoes::{Echo, Echoes, SETTLE_MS, clock as echo_clock};
pub(crate) use gate::{Held, Made, Route};
pub(crate) use session::{HostSession, HostTextReader};

impl SyncState {
    /// Sends `text`, the document in the host's language, to `on_change`,
    /// recorded as sent: its echo is told apart when it comes back.
    pub(super) fn tell_host(&self, text: String) {
        self.echoes.update_value(|echoes| {
            echoes.sent(&text);
        });
        if let Some(on_change) = self.host_on_change {
            on_change.run(text);
        }
    }
}

/// The host's side of the connection (from the component's props).
pub(super) struct Host {
    pub(super) value: Option<Signal<String>>,
    /// The revision `on_change` sent `value` with, when the host passes it
    /// back.
    pub(super) value_revision: MaybeProp<u64>,
    pub(super) edit_as: MaybeProp<Language>,
    pub(super) on_edit_as_change: Option<Callback<Language>>,
    /// A switch the document could not make when mounted.
    pub(super) initial_pending: Option<Language>,
    pub(super) on_sync_change: Option<Callback<bool>>,
    /// The host hands every `on_change` text back as `value` (see
    /// `echoes`).
    pub(super) echoes: bool,
}

/// The host-and-Edit-as part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct SyncState {
    /// The language on screen; `edit_as` switches change it.
    pub(super) edit_language: RwSignal<Language>,
    /// The host's `on_change`: told each text sent (see
    /// [`SyncState::tell_host`]).
    pub(super) host_on_change: Option<Callback<String>>,
    /// The host's text and the text on screen that stands for it.
    pub(super) session: StoredValue<HostSession>,
    /// The session's host text, read.
    pub(super) host_text: HostTextReader,
    /// The texts sent and handed back (see `echoes`): with the `echoes`
    /// option, a `value` that is an echo of an edit typed past is skipped.
    pub(super) echoes: StoredValue<Echoes>,
    /// Counts the pauses after an echo started (see `settle_later`): only
    /// the newest one's check runs.
    pub(super) settles: StoredValue<u64>,
    /// Notified when the session changes: the baseline as shown follows it
    /// (a formatting-only edit converted to the host's own text is the
    /// baseline as shown, so nothing is dirty).
    pub(super) synced: Trigger,
    /// Told the language on screen after each switch (and undo or redo back
    /// across one); set when the editor connects.
    pub(super) on_edit_as_change: StoredValue<Option<Callback<Language>>>,
    /// Whether `on_edit_as_change` was told a language yet (the mount's
    /// report is left out after a switch made before it).
    pub(super) language_told: StoredValue<bool>,
    /// An `edit_as` switch waiting until the document can convert.
    pub(super) pending_switch: RwSignal<Option<Language>>,
    /// Edit-as problems of the text shown (values it cannot show, a
    /// switch that waits).
    pub(super) load_problems: RwSignal<Vec<EditAsProblem>>,
    /// Edit-as problems of the last save (values the host cannot keep, a
    /// save refused).
    pub(super) conversion_problems: RwSignal<Vec<EditAsProblem>>,
    /// Each item of the text on screen's link to the host item it came
    /// from, kept by the edits (see `bridge::links`): they tell a save
    /// which item of the host each item is.
    pub(super) links: StoredValue<Links>,
    /// Text copied or cut with the links of its items: a paste of it
    /// takes them.
    pub(super) clipboard: StoredValue<Clipboard>,
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
        let mut opening_shown = false;
        if let Some(bridge) = Bridge::new(host_language, edit_language, schema.get_value()) {
            match bridge.to_edit(&host_text) {
                Ok(conversion) => {
                    load_problems = bridge.view_problems(&conversion);
                    shown = conversion.text;
                }
                Err(_) => pending = Some(edit_language),
            }
            if let Some(opening) = opening.map(editor_text) {
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
                opening_shown = synced.as_deref() != Some(opening.as_str());
                shown = opening;
            }
        }
        let synced = synced.unwrap_or_else(|| shown.clone());
        let edit_language = RwSignal::new(if pending.is_some() {
            host_language
        } else {
            edit_language
        });
        let session = StoredValue::new(HostSession::new(&host_text, &synced));
        let sync = Self {
            edit_language,
            host_on_change,
            links: StoredValue::new(if opening_shown {
                Links::default()
            } else {
                let shown_in = edit_language.get_untracked();
                let reading = Bridge::new(host_language, shown_in, schema.get_value())
                    .map(|bridge| bridge.reading());
                Links::fresh(&host_text, &synced, reading)
            }),
            clipboard: StoredValue::new(Clipboard::default()),
            host_text: HostTextReader(session),
            session,
            echoes: StoredValue::new(Echoes::new(false)),
            settles: StoredValue::new(0),
            synced: Trigger::new(),
            on_edit_as_change: StoredValue::new(None),
            language_told: StoredValue::new(false),
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
        if self.bridge().is_some() {
            return;
        }
        let text = self.doc().text.get_untracked();
        let previous = self.sync().host_text.get_value();
        if self.admit(&previous, &text, Made::Typed).is_ok() {
            self.sync().typed_in_host_language(&text);
            self.sync().tell_host(text);
        }
    }

    /// `host` as shown in the language on screen, or the error it cannot
    /// convert with (the switch then waits: see [`take_host_value_at`]).
    fn shown(&self, host: &str) -> Result<String, codec::ConvertError> {
        let Some(bridge) = self.bridge() else {
            return Ok(host.to_string());
        };
        let conversion = bridge.to_edit(host)?;
        self.sync()
            .load_problems
            .set(bridge.view_problems(&conversion));
        Ok(conversion.text)
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
                    // The host's own text again: the session's text on
                    // screen; another is the host's once it reaches it.
                    if host == edit.text {
                        self.sync().typed_in_host_language(&edit.text);
                    }
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

    /// The links of the text on screen's items follow `edit` (see
    /// `bridge::links`): an edit of the user's as the splices it made, an
    /// undo or redo back to the links its step kept, a text the host or a
    /// switch put on screen as the items of the host's text (or, unsaved
    /// edits converted, as the items at the same pointers). Returns the
    /// view's state before it (its links, and the host text it was saved
    /// as), for the undo step.
    pub(super) fn note_view_edit(&self, edit: &Edit, origin: &Origin, left: Language) -> ViewState {
        // A text in the host's own language is no view of it: it stands
        // for no saved host text in a view (pasted into one, it is read in
        // the view's language, and waits when it does not read there).
        let in_view = left != self.config().host_language;
        let before = ViewState::of(
            &edit.before,
            self.sync().links.get_value(),
            self.sync().saved_as(&edit.before).filter(|_| in_view),
        );
        if edit.before == edit.text {
            return before;
        }
        let language = self.sync().edit_language.get_untracked();
        let saved = self.sync().is_synced(&edit.text);
        let next = if language == self.config().host_language {
            // The host's own text: no view, no links.
            Links::default()
        } else if matches!(edit.origin, Origin::Undo | Origin::Redo) {
            // The step the undo or redo is made of (it is moved after).
            let view = self
                .doc()
                .history
                .with_untracked(|history| history.next_view(edit.origin == Origin::Redo).cloned())
                .unwrap_or_default();
            // Back to a text saved before: its save is that host text.
            if let Some(host) = view.saved_as {
                self.sync().restored(&edit.text, host);
            }
            view.links
        } else if matches!(origin, Origin::Command | Origin::Input { .. }) {
            let mut links = before.links.clone();
            self.sync().clipboard.update_value(|clipboard| {
                let pieces = bridge::links::pieces(edit, clipboard);
                links.edited(language.behavior(), &edit.before, &pieces, clipboard);
            });
            links
        } else if saved {
            // A switch, or the host's text: its items, each the host's.
            let reading = self.bridge_to(language).map(|bridge| bridge.reading());
            Links::fresh(&self.sync().host_text.get_value(), &edit.text, reading)
        } else if *origin == Origin::Switch && left != self.config().host_language {
            // Unsaved edits converted: each item at the same pointer.
            let mut links = before.links.clone();
            links.converted(
                left.behavior(),
                &edit.before,
                language.behavior(),
                &edit.text,
            )
        } else {
            Links::default()
        };
        self.sync().links.set_value(next);
        before
    }

    /// What the links of the text on screen, `text` in `language`, tell
    /// `bridge`'s save over the host's last text `previous` (see
    /// `links::Links::told`); a host whose items hold what no view shows
    /// (XML markup) is told even after typing only.
    fn told(
        &self,
        bridge: &Bridge,
        language: Language,
        text: &str,
        previous: &str,
    ) -> (Option<bridge::links::Told>, Vec<bridge::links::Untouched>) {
        let identity = bridge.host.behavior().items_hold_unshown(previous);
        let mut links = self.sync().links.get_value();
        // Both read the spans of the view's items: once.
        let (told, untouched) = super::paths::sharing_spans(language.behavior(), text, || {
            (
                links.told(language.behavior(), text, previous, identity),
                links.untouched(language.behavior(), text),
            )
        });
        self.sync().links.set_value(links);
        (told, untouched)
    }

    /// `text` (bytes `start..end` of the text on screen) copied or cut:
    /// the clipboard's record of it keeps its items' links.
    /// In the host's own language the text on screen is the host's: each
    /// item copied is the host's item at its own pointer, so a paste into
    /// a view after a switch keeps its source.
    pub(super) fn note_copied(&self, text: &str, (start, end): (TextSize, TextSize)) {
        let language = self.behavior();
        let own = self.bridge().is_none();
        let mut links = if own {
            self.doc()
                .text
                .with_untracked(|current| Links::fresh(current, current, None))
        } else {
            self.sync().links.get_value()
        };
        // A selection from its end copies no link.
        let copied = if start <= end {
            self.doc().text.with_untracked(|current| {
                links.copied((language, current), TextRange::new(start, end))
            })
        } else {
            Vec::new()
        };
        if !own {
            self.sync().links.set_value(links);
        }
        self.sync()
            .clipboard
            .update_value(|clipboard| clipboard.copied(text, copied));
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
        self.sync().language_told.set_value(true);
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
    if host.echoes {
        let host_text = state.sync().host_text.get_value();
        state.sync().echoes.update_value(|echoes| {
            *echoes = Echoes::new(true);
            echoes.given(&host_text);
        });
    }
    if let Some(value) = host.value {
        follow_value(state, value, host.value_revision);
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
/// new text, not on the old one the effect would replace later. Its
/// `revision`, read as the value changes, is the one the host set with it.
fn follow_value(state: EditorState, value: Signal<String>, revision: MaybeProp<u64>) {
    leptos::reactive::effect::ImmediateEffect::new_scoped(move || {
        let value = value.get();
        let revision = revision.get_untracked();
        let text = state.sync().line_endings.received(value);
        take_host_value_at(state, text, revision);
    });
}

/// Once the pause after an echo that left the host behind is over, a host
/// still on an older text hears the document again (see
/// `Echoes::settle`); a later echo of the kind starts the pause again.
fn settle_later(state: EditorState) {
    state.sync().settles.update_value(|settles| *settles += 1);
    let current = state.sync().settles.get_value();
    later::after(state, SETTLE_MS, move |state| {
        if state.sync().settles.get_value() != current {
            return;
        }
        let host = state.sync().host_text.get_value();
        let again = state
            .sync()
            .echoes
            .try_update_value(|echoes| echoes.settle(&host))
            .unwrap_or(false);
        if again && let Some(on_change) = state.sync().host_on_change {
            on_change.run(host);
        }
    });
}

/// [`take_host_value_at`] for a text the host gave with no revision.
#[cfg(test)]
#[allow(
    dead_code,
    reason = "its callers are tests of single languages: some builds have none"
)]
pub(super) fn take_host_value(state: EditorState, external: String) {
    take_host_value_at(state, external, None);
}

/// Shows `external` (normalized), a new text from the host, given with the
/// `revision` `on_change` sent it with when the host passes it back (the
/// `value_revision` prop: its echo is matched exactly, see
/// `Echoes::received_at`). One the language on screen cannot hold (TOML
/// has no `null`) is shown in the host's language, read as it, and the
/// switch waits for the document to convert, as one asked for on such a
/// document does: the host hears the language on screen, and the edits
/// reach `on_change` meanwhile.
fn take_host_value_at(state: EditorState, external: String, revision: Option<u64>) {
    let host = state
        .sync()
        .session
        .with_value(|session| session.host.clone());
    let flushed = echo_clock();
    match state
        .sync()
        .echoes
        .try_update_value(|echoes| match revision {
            Some(revision) => echoes.received_at(&external, revision, &host, flushed),
            None => echoes.received(&external, &host, flushed),
        })
        .unwrap_or(Echo::Apply)
    {
        Echo::Apply => {}
        Echo::Skip => return,
        Echo::Settle => return settle_later(state),
        // Left behind the document by a late echo: the host hears it
        // again (recorded as sent already).
        Echo::Resend => {
            if let Some(on_change) = state.sync().host_on_change {
                on_change.run(host.to_string());
            }
            return settle_later(state);
        }
    }
    // The host's text already (its own echo): reconverting would reformat
    // the text under the caret.
    if *host == *external {
        return;
    }
    let (edit, waiting) = match state.shown(&external) {
        Ok(edit) => (edit, None),
        Err(_) => (
            external.clone(),
            Some(state.sync().edit_language.get_untracked()),
        ),
    };
    // A new session, though the view's text may read the same.
    state.sync().received(&external, &edit);
    let reading = match waiting {
        Some(_) => None,
        None => state.bridge().map(|bridge| bridge.reading()),
    };
    state
        .sync()
        .links
        .set_value(Links::fresh(&external, &edit, reading));
    state.note_in_sync(true);
    let current = state.doc().text.get_untracked();
    let mut change = Edit::replacing(current, edit, Origin::HostValue);
    if let Some(waiting) = waiting {
        change.language = Some(state.config().host_language);
        state.commit(change);
        // Fails the same way, and waits with the reason.
        switch_to(state, waiting);
    } else if change.before != change.text {
        state.commit(change);
        state.hide_hover();
    }
}

/// Debounced so half-typed text does not flash squiggles; everything else
/// reads the live source.
fn debounce_validation(state: EditorState) {
    // The step waiting: a new text cancels it.
    let waiting = StoredValue::new(later::Timeout::default());
    Effect::new(move |_| {
        let text = state.doc().text.get();
        let timer = later::after(state, DIAGNOSTICS_DEBOUNCE_MS, move |state| {
            // The pause in typing: the placeholders' fields first (a read
            // of the whole text), then the problems in a task of their
            // own, so a key pressed meanwhile is not held up by both; the
            // problems wait for the next pause then.
            state.recheck_tokens();
            let timer = later::after(state, 0, move |state| {
                state.problems().diagnosed_text.set(text);
            });
            later::replace_in(waiting, timer);
        });
        later::replace_in(waiting, timer);
    });
}

/// While editing as another language, each pause in typing converts the
/// text back and sends it to the host when it converts: in a task of its
/// own, after the one checking the text and placing its problems, so
/// neither waits for the other (together, a long document with thousands
/// of host problems held the page for over half a second).
fn convert_edits(state: EditorState) {
    Effect::new(move |_| {
        let text = state.problems().diagnosed_text.get();
        let language = state.sync().edit_language.get();
        later::after(state, 0, move |state| {
            // A text the document no longer is (the first run, after the
            // host replaced the text in the same task, or typed on since)
            // is not an edit: converting it would send the old data under
            // the host's new text. The pause after the new text converts
            // that one; `flush_pending_edits` converts it at once.
            let current = state.doc().text.with_untracked(|current| *current == text)
                && state.sync().edit_language.get_untracked() == language;
            if current {
                convert_now(state, text, language);
            }
        });
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
            note_conversion_problems(state, Vec::new());
            state.note_in_sync(true);
            return;
        };
        if state.sync().is_synced(&text) {
            note_conversion_problems(state, Vec::new());
            state.note_in_sync(true);
            return;
        }
        // The placeholders follow their fields first, as the typing pause
        // has them follow: a save or flush before the pause converts the
        // text the pause would (a placeholder's item shifted is still real).
        if state.doc().text.with_untracked(|current| *current == text) {
            state.recheck_tokens();
        }
        let previous = state.sync().host_text.get_value();
        let origin = state.sync().origin();
        // Back to a text saved before (an undo step, across a switch too,
        // says so), or the host's own (a reloaded baseline): its host text
        // exactly, not converted again (a list of one converted again may
        // not read back as the lone element the host holds).
        let known = state
            .sync()
            .saved_as(&text)
            .or_else(|| {
                state
                    .doc()
                    .history
                    .with_untracked(|history| history.saved_as(&text))
            })
            .map(|host| host.to_string())
            // The same text in another view's language is another document
            // (text cut in one view, pasted back after a switch).
            .filter(|host| bridge.shows_as(host, &text));
        let hosts_own = known.is_some();
        // The links are the text on screen's.
        let on_screen = state.doc().text.with_untracked(|current| *current == text)
            && state.sync().edit_language.get_untracked() == language;
        let mut loose = None;
        let result = match known {
            Some(host) => Ok(host),
            None => {
                // Which host item each item is, as the edits kept it; a
                // host whose items hold what no view shows (XML markup)
                // reads it even when only typed.
                let (told, untouched) = if on_screen {
                    state.told(&bridge, language, &text, &previous)
                } else {
                    Default::default()
                };
                let saved =
                    bridge.to_host_linked(&text, &previous, &origin, told.as_ref(), &untouched);
                loose = told.map(|told| told.loose);
                saved
            }
        };
        // However the edit got there, the gate decides whether the host
        // may have it: a save never changes a read-only value, not even
        // back to a text the host itself had.
        let made = if hosts_own {
            Made::Restored
        } else {
            Made::Converted
        };
        let result = result
            .map_err(Held::Refused)
            .and_then(|saved| state.admit(&previous, &saved, made).map(|()| saved));
        // A text the guard holds shows the guard's problem, not one of its
        // conversion.
        let problems = match &result {
            // A parse error is the syntax problem's to show (`problems`
            // lists none for it): only another reason is worded here.
            Err(Held::Refused(error @ SaveError::Convert(codec::ConvertError::Parse(_)))) => {
                bridge.problems(&text, &Err(error.clone()))
            }
            Err(Held::Refused(error)) => bridge.problems(
                &text,
                &Err(without_secret_quotes(state, error.clone(), &text, language)),
            ),
            _ => Vec::new(),
        };
        note_conversion_problems(state, problems);
        state.note_in_sync(result.is_ok());
        if let Ok(saved) = result {
            if on_screen {
                let mut links = state.sync().links.get_value();
                let links = links.saved(
                    language.behavior(),
                    (&saved, &text),
                    Some(bridge.reading()),
                    loose,
                );
                state.sync().links.set_value(links);
            }
            let changed = saved != previous;
            state.sync().saved(&saved, &text);
            if changed {
                state.sync().tell_host(saved);
            }
        }
    }
}

/// The last conversion's problems are `problems`: told only when they
/// change, as validation reads them (each pause would check the text
/// again for the same, empty, list).
fn note_conversion_problems(state: EditorState, problems: Vec<EditAsProblem>) {
    let problems_of = state.sync().conversion_problems;
    if problems_of.with_untracked(|known| *known != problems) {
        problems_of.set(problems);
    }
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
pub(super) fn save_pending(state: EditorState) {
    let (Some(text), Some(session), Some(edit), Some(schema)) = (
        state.doc().text.try_get_untracked(),
        state.sync().session.try_get_value(),
        state.sync().edit_language.try_get_untracked(),
        state.config().schema.try_get_value(),
    ) else {
        return;
    };
    if *session.view == *text {
        return;
    }
    let Some(bridge) = bridge::Bridge::new(state.config().host_language, edit, schema) else {
        return;
    };
    let (host, origin) = (session.host.to_string(), session.origin.to_string());
    // Told which item is which by the links, as a pause's save is.
    let (told, untouched) = state
        .sync()
        .links
        .try_get_value()
        .map(|_| state.told(&bridge, edit, &text, &host))
        .unwrap_or_default();
    if let Ok(saved) = bridge.to_host_linked(&text, &host, &origin, told.as_ref(), &untouched)
        && saved != host
        && state.admit(&host, &saved, Made::Converted).is_ok()
    {
        state.sync().tell_host(saved);
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
    if on_edit_as_change.is_some() {
        // The language on screen once mounted (the host's own, also while a
        // switch waits), as a documents view tells it, unless a switch made
        // since told it (one asked for before mounting, in the effect
        // above). Reported after mounting, so a host's callback can use the
        // editor it is creating.
        Effect::new(move |previous: Option<()>| {
            if previous.is_none() && !state.sync().language_told.get_value() {
                state.report_language(state.sync().edit_language.get_untracked());
            }
        });
    }
    Effect::new(move |_| {
        state.problems().diagnosed_text.track();
        retry_pending_switch(state);
    });
}

/// Tries a switch still waiting for the document to convert, after the
/// typing pause. Not on a step back through undo: the switch would end
/// redo, and the steps the user may redo would be lost. The next edit
/// (which ends redo anyway) makes it. Not while a drag of the text goes
/// on: the switch would rewrite the text it carries; it is tried when the
/// drag ends (see `drag_move`).
pub(super) fn retry_pending_switch(state: EditorState) {
    if state.doc().history.with_untracked(EditHistory::can_redo) || state.dragging() {
        return;
    }
    if let Some(next) = state.sync().pending_switch.get_untracked() {
        switch_to(state, next);
    }
}

impl EditorState {
    /// Does the typing pause's work for the text on screen now, so the
    /// problems are those of this text (F8 walks them at once): checks it,
    /// tries a switch still waiting, and converts an edit-as edit, whose
    /// problems (where it does not convert back) are placed on this text,
    /// not on the one before the last keys.
    pub(super) fn check_text_now(&self) {
        let state = *self;
        let text = state.doc().text.get_untracked();
        if state
            .problems()
            .diagnosed_text
            .with_untracked(|diagnosed| *diagnosed == text)
        {
            return;
        }
        state.problems().diagnosed_text.set(text);
        retry_pending_switch(state);
        // A switch made shows another text.
        let text = state.doc().text.get_untracked();
        if state
            .problems()
            .diagnosed_text
            .with_untracked(|diagnosed| *diagnosed != text)
        {
            state.problems().diagnosed_text.set(text.clone());
        }
        let language = state.sync().edit_language.get_untracked();
        convert_now(state, text, language);
    }
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
    // A change held back in the host's language (a locked field reads
    // otherwise) is no text of the host's: a view of it would save over a
    // host text it does not show. The switch waits, the hold's reason
    // shown, until an edit ends the hold.
    if current == host && state.changes_held() {
        state.sync().pending_switch.set(Some(next));
        state.sync().load_problems.set(Vec::new());
        return;
    }
    let text = state.doc().text.get_untracked();
    // An edit of a view still waiting out the typing pause is saved first,
    // with the edits that tell its items apart (converted straight into the
    // next view, they would be lost, and the save would pair the items by
    // place).
    if !state.sync().is_synced(&text) && state.bridge_to(current).is_some() {
        convert_now(state, text.clone(), current);
    }
    let host_text = state.sync().host_text.get_value();
    let host_text_before = host_text.clone();
    let unsaved = !state.sync().is_synced(&text);
    let mut view_warnings = Vec::new();
    let mut guarded = false;
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
            Some(bridge) => {
                let (told, untouched) = state.told(&bridge, current, &text, &host_text);
                bridge
                    .to_host_linked(
                        &text,
                        &host_text,
                        &state.sync().origin(),
                        told.as_ref(),
                        &untouched,
                    )
                    .and_then(|saved| {
                        // The guard holding the text shows it in the host's
                        // language, the host not told; a save the host
                        // refuses waits.
                        match state.admit(&host_text, &saved, Made::Converted) {
                            Ok(()) => Ok(saved),
                            Err(Held::Refused(error)) => Err(error),
                            Err(_) => {
                                guarded = true;
                                Ok(saved)
                            }
                        }
                    })
            }
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
                // switch restore it exactly (its step records the text left).
                state.sync().switched(&converted);
                // The new view is the host's text shown: in sync, whatever
                // the last pause made of a text since undone.
                note_conversion_problems(state, Vec::new());
                state.note_in_sync(true);
            } else if next == state.config().host_language && !guarded {
                // Back to the host language with an edit still waiting out
                // the typing pause: the converted text is the host's now.
                state.sync().saved(&converted, &converted);
                state.note_in_sync(true);
                if converted != host_text_before {
                    state.sync().tell_host(converted.clone());
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
            let caret = field_caret(
                utf16_of(&text, selection.0),
                (current, &text),
                (next, &converted),
            )
            .map(|caret| byte_of(&converted, caret));
            // Another language's lines: each fold is carried by its field; the
            // switch is an undo step back to this text (see
            // `edit::Origin::Switch`).
            state.commit(Edit::switching(text, converted, next, selection, caret));
            state.sync().load_problems.set(view_warnings);
        }
        Err(error) => {
            // A parse error of the text the switch read: of the view's
            // when it has edits the host has not had, else of the host's.
            let error = if unsaved {
                without_secret_quotes(state, error, &text, current)
            } else {
                let host = state.config().host_language;
                without_secret_quotes(state, error, &host_text_before, host)
            };
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

/// `error`, the reason `source` (in `language`) cannot be saved or
/// switched, without what it quotes of a secret: a parse error worded as
/// its syntax problem is (see `diagnostics::syntax_message`), any other
/// reason with each secret's text it quotes hidden (see
/// `diagnostics::save_message`). Every save or switch problem shown is
/// worded through here.
fn without_secret_quotes(
    state: EditorState,
    error: SaveError,
    source: &str,
    language: Language,
) -> SaveError {
    match error {
        SaveError::Convert(codec::ConvertError::Parse(mut parse)) => {
            let protect = state.protect();
            let (accepted, patterns) = (protect.accepted_tokens(), protect.placeholder_patterns());
            parse.message = state.config().schema.with_value(|schema| {
                syntax_message(
                    source,
                    &parse,
                    schema.as_deref(),
                    language.behavior(),
                    Placeholders {
                        accepted: &accepted,
                        patterns: &patterns,
                    },
                )
            });
            SaveError::Convert(codec::ConvertError::Parse(parse))
        }
        SaveError::Convert(codec::ConvertError::Unsupported(message)) => {
            let protect = state.protect();
            let (accepted, patterns) = (protect.accepted_tokens(), protect.placeholder_patterns());
            let message = state.config().schema.with_value(|schema| {
                save_message(
                    &message,
                    source,
                    schema.as_deref(),
                    language.behavior(),
                    Placeholders {
                        accepted: &accepted,
                        patterns: &patterns,
                    },
                )
            });
            SaveError::Convert(codec::ConvertError::Unsupported(message))
        }
        other => other,
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
        let key = path.texts().pop()?;
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

#[cfg(all(test, feature = "protect", feature = "json"))]
mod gate_tests;

#[cfg(all(test, feature = "yaml"))]
mod value_tests;
