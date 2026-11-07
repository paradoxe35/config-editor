//! Host-controlled editing: nothing is saved until the user or the host asks
//! (`Ctrl+S`, [`EditorHandle::save`](super::EditorHandle::save)), the host
//! hears when the text starts or stops differing from its baseline, and a
//! baseline that changes under unsaved edits asks the user what to do.
//!
//! - `save`: the save command and the host document it sends.
//! - `conflict`: a new baseline from the host, and the notice's choices.

mod conflict;
mod save;
pub(super) mod texts;

#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};

use leptos::prelude::*;

use super::bridge::Bridge;
use super::languages::Language;
use super::state::{EditorState, normalize_newlines};

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

/// The host's `baseline` as shown on screen. `synced` is the last host text
/// and the edited text it saved as: a baseline equal to that host text (set
/// right after a save) is shown as that edited text, so saving while
/// editing as another language leaves nothing marked changed, formatting
/// the conversion would redo differently included.
pub(super) fn shown_baseline(
    bridge: Option<&Bridge>,
    baseline: &str,
    synced: (&str, &str),
) -> String {
    let Some(bridge) = bridge else {
        return baseline.to_string();
    };
    let (host_text, synced_edit) = synced;
    if baseline == host_text {
        return synced_edit.to_string();
    }
    bridge
        .to_edit(baseline)
        .map_or_else(|_| baseline.to_string(), |conversion| conversion.text)
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
    /// The notice's Keep mine button, which takes the focus when it opens.
    pub(super) keep_mine_ref: NodeRef<leptos::html::Button>,
    pub(super) notice_ref: NodeRef<leptos::html::Div>,
}

impl HostEditing {
    pub(super) fn new(
        baseline: RwSignal<String>,
        dirty: Memo<bool>,
        sent: StoredValue<SentTexts>,
        on_save: Option<Callback<SaveRequest>>,
        save_shortcut: Signal<bool>,
        on_conflict: Option<Callback<BaselineConflict>>,
    ) -> Self {
        Self {
            baseline,
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
        let new = normalize_newlines(baseline.get());
        untrack(|| state.offer_baseline(new));
    });
    conflict::settle_when_text_matches(state);
    if let Some(callback) = on_dirty_change {
        Effect::new(move |_| callback.run(state.editing().dirty.get()));
    }
}
