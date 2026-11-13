//! Host-controlled editing: nothing is saved until the user or the host asks
//! (`Ctrl+S`, [`EditorHandle::save`](super::EditorHandle::save)), the host
//! hears when the text starts or stops differing from its baseline, and a
//! baseline that changes under unsaved edits asks the user what to do.
//!
//! - `save`: the save command and the host document it sends.
//! - `conflict`: a new baseline from the host, and the notice's choices.

mod conflict;
mod save;

#[cfg(test)]
mod edit_as_save_tests;
#[cfg(test)]
mod save_tests;
#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};

use leptos::prelude::*;

use std::sync::Arc;

use super::bridge::Bridge;
use super::diagnostics::SchemaBundle;
use super::languages::Language;
use super::line_endings::editor_text;
use super::state::EditorState;
use super::sync::SyncState;

/// What `on_save` receives: the document to store, in the host's
/// `language` (Edit-as edits already converted back).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SaveRequest {
    /// The document, in `language`.
    pub text: String,
    /// The host's language (the `language` prop), not the one on screen.
    pub language: Language,
}

/// What `on_baseline_conflict` reports about a baseline that changed while
/// the user had unsaved edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BaselineConflict {
    /// The notice opened: the user is asked to reload or keep their text.
    Opened,
    /// The user chose Reload: the text is the new baseline now (one undo
    /// step, sent through `on_change`).
    Reloaded,
    /// The user chose Keep mine (or pressed Escape): the text stays and is
    /// compared with the new baseline.
    KeptMine,
    /// The notice closed by itself: the text came to equal the new baseline,
    /// or the host set the baseline back.
    Settled,
}

/// A baseline the host set while the user had unsaved edits, waiting for
/// the user's choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingBaseline {
    /// In the host's language.
    pub(super) host: String,
    /// As shown in the language on screen.
    pub(super) shown: String,
}

/// What a new baseline from the host does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BaselineDecision {
    /// It is the baseline already in use.
    Unchanged,
    /// The diff and the dirty flag use it from now on.
    Take,
    /// It would silently mix with unsaved edits: ask the user.
    Conflict,
}

/// What a new baseline `new` does, with `in_use` the baseline the diff uses
/// now. It is taken silently when nothing is unsaved (`dirty` is false),
/// when it already is the text (`matches_text`: both sides made the same
/// change), or when it is a text the editor sent the host itself
/// (`sent_by_editor`: a save coming back while the user typed on). Anything
/// else is a conflict.
pub(super) fn decide_baseline(
    in_use: &str,
    new: &str,
    dirty: bool,
    matches_text: bool,
    sent_by_editor: bool,
) -> BaselineDecision {
    if new == in_use {
        BaselineDecision::Unchanged
    } else if !dirty || matches_text || sent_by_editor {
        BaselineDecision::Take
    } else {
        BaselineDecision::Conflict
    }
}

/// The host's `baseline` as shown on screen, `converted` to the language on
/// screen when that is another (`bridged`). `synced` is the last host text
/// and the edited text it saved as: a baseline equal to that host text (set
/// right after a save, or a formatting-only edit converted back to it) is
/// shown as that edited text, so saving while editing as another language
/// leaves nothing marked changed, formatting the conversion would redo
/// differently included.
pub(super) fn shown_baseline(
    bridged: bool,
    baseline: &str,
    synced: (&str, &str),
    converted: impl FnOnce() -> String,
) -> String {
    let (host_text, synced_edit) = synced;
    if bridged && baseline == host_text {
        return synced_edit.to_string();
    }
    converted()
}

/// Texts sent to the host recently, as fingerprints: a baseline equal to one
/// is the user's own work coming back, not a change from elsewhere.
#[derive(Clone, Debug, Default)]
pub(super) struct SentTexts(VecDeque<(usize, u64)>);

/// Sent texts remembered.
const SENT_TEXTS_KEPT: usize = 64;

impl SentTexts {
    fn fingerprint(text: &str) -> (usize, u64) {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        (text.len(), hasher.finish())
    }

    pub(super) fn note(&mut self, text: &str) {
        let print = Self::fingerprint(text);
        self.0.retain(|known| *known != print);
        self.0.push_back(print);
        if self.0.len() > SENT_TEXTS_KEPT {
            self.0.pop_front();
        }
    }

    pub(super) fn contains(&self, text: &str) -> bool {
        self.0.contains(&Self::fingerprint(text))
    }
}

/// The host-editing part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct HostEditing {
    /// The baseline the diff and the dirty flag use, in the host's
    /// language. It follows the `baseline` prop, except while a conflict
    /// waits for the user.
    pub(super) baseline: RwSignal<String>,
    /// `baseline` as shown, in the language on screen (the diff reads it
    /// too).
    #[cfg_attr(not(feature = "diff"), allow(dead_code))]
    pub(super) shown_baseline: Memo<String>,
    /// A new baseline waiting for the user's choice (the notice is open).
    pub(super) conflict: RwSignal<Option<PendingBaseline>>,
    /// The text on screen differs from the baseline as shown.
    pub(super) dirty: Memo<bool>,
    /// What `on_change` and `on_save` sent the host lately.
    pub(super) sent: StoredValue<SentTexts>,
    pub(super) on_save: Option<Callback<SaveRequest>>,
    /// `Ctrl+S` saves (with `on_save`).
    pub(super) save_shortcut: Signal<bool>,
    pub(super) on_conflict: Option<Callback<BaselineConflict>>,
    /// The notice's Keep mine button, where `F6` moves the focus.
    pub(super) keep_mine_ref: NodeRef<leptos::html::Button>,
    pub(super) notice_ref: NodeRef<leptos::html::Div>,
}

/// The host's side of saving (from the component's props).
pub(super) struct Init {
    /// The host's diff baseline at mount, in the host language.
    pub(super) baseline: String,
    /// What reaches the host, remembered (see [`noting_sent`]).
    pub(super) sent: StoredValue<SentTexts>,
    pub(super) on_save: Option<Callback<SaveRequest>>,
    pub(super) save_shortcut: Signal<bool>,
    pub(super) on_conflict: Option<Callback<BaselineConflict>>,
}

impl HostEditing {
    /// Host editing for `text`, the document on screen, kept in step with
    /// the host's text through `sync`.
    pub(super) fn new(
        init: Init,
        text: RwSignal<String>,
        host_language: Language,
        schema: StoredValue<Option<Arc<SchemaBundle>>>,
        sync: &SyncState,
    ) -> Self {
        let Init {
            baseline,
            sent,
            on_save,
            save_shortcut,
            on_conflict,
        } = init;
        let baseline = RwSignal::new(editor_text(baseline));
        let SyncState {
            edit_language,
            shown_cache,
            host_text,
            synced_edit,
            synced,
            ..
        } = *sync;
        // The baseline converted for the language on screen: once per
        // baseline and language, not on every conversion of an edit.
        let converted = Memo::new(move |_| {
            let host = baseline.get();
            let edit = edit_language.get();
            // Usually the baseline is the host text a switch just converted.
            let cached = shown_cache.with_value(|cache| {
                cache
                    .as_ref()
                    .filter(|(language, from, _)| *language == edit && *from == host)
                    .map(|(_, _, shown)| shown.clone())
            });
            cached.unwrap_or_else(
                || match Bridge::new(host_language, edit, schema.get_value()) {
                    Some(bridge) => bridge
                        .to_edit(&host)
                        .map_or_else(|_| host.clone(), |conversion| conversion.text),
                    None => host,
                },
            )
        });
        // A baseline the host's text is (nothing unsaved, a formatting-only
        // edit converted back to it) is shown as the edited text it saved
        // as, followed through every conversion.
        let shown_baseline = Memo::new(move |_| {
            synced.track();
            let host = baseline.get();
            let bridged = edit_language.get() != host_language;
            host_text.with_value(|host_text| {
                synced_edit.with_value(|synced_edit| {
                    shown_baseline(bridged, &host, (host_text, synced_edit), || converted.get())
                })
            })
        });
        let dirty =
            Memo::new(move |_| text.with(|text| shown_baseline.with(|baseline| text != baseline)));
        Self {
            baseline,
            shown_baseline,
            conflict: RwSignal::new(None),
            dirty,
            sent,
            on_save,
            save_shortcut,
            on_conflict,
            keep_mine_ref: NodeRef::new(),
            notice_ref: NodeRef::new(),
        }
    }
}

/// `on_change` wrapped to remember what it sends in `sent` (see
/// [`SentTexts`]).
pub(super) fn noting_sent(
    sent: StoredValue<SentTexts>,
    on_change: Callback<String>,
) -> Callback<String> {
    Callback::new(move |text: String| {
        sent.update_value(|sent| sent.note(&text));
        on_change.run(text);
    })
}

/// The host's side (from the component's props).
pub(super) struct Host {
    pub(super) baseline: Signal<String>,
    pub(super) on_dirty_change: Option<Callback<bool>>,
}

pub(super) fn connect(state: EditorState, host: Host) {
    let Host {
        baseline,
        on_dirty_change,
    } = host;
    Effect::new(move |_| {
        let new = editor_text(baseline.get());
        untrack(|| state.offer_baseline(new));
    });
    conflict::settle_when_text_matches(state);
    conflict::reach_notice_from_outside(state);
    if let Some(callback) = on_dirty_change {
        Effect::new(move |_| callback.run(state.editing().dirty.get()));
    }
}
