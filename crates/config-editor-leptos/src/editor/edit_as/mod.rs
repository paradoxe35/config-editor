//! Editing as another language, beyond the conversion itself (`bridge`,
//! `sync`):
//!
//! - `problems`: what Edit-as reports (values a language cannot keep or
//!   show, saves refused), worded from the `strings` setting when shown.
//! - `notice`: the notice saying what of the host's file the edited
//!   language does not show, and what typed there is not saved.

mod notice;
mod problems;
// The matrix pairs languages, so it needs every built-in one.
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod switch_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod tests;

pub(super) use notice::NoticeReasons;
pub(super) use problems::{EditAsProblem, EditAsText, SaveError};

use leptos::prelude::*;

use super::languages::Language;
use super::state::EditorState;

/// The notice's part of the editor state.
#[derive(Clone, Copy)]
pub(super) struct EditAsState {
    /// Why the notice shows, before dismissals: checked on the text after
    /// each typing pause, as problems are.
    pub(super) reasons: Memo<NoticeReasons>,
    /// The reasons dismissed, and the language they were dismissed in:
    /// another language, another notice.
    pub(super) dismissed: RwSignal<(Language, NoticeReasons)>,
}

impl EditAsState {
    pub(super) fn new(
        host_language: Language,
        edit_language: RwSignal<Language>,
        diagnosed_text: RwSignal<String>,
        host_text: StoredValue<String>,
    ) -> Self {
        let reasons = Memo::new(move |_| {
            let edit = edit_language.get();
            diagnosed_text.with(|edited| {
                host_text.with_value(|host| NoticeReasons::of(host_language, edit, host, edited))
            })
        });
        let dismissed = RwSignal::new((host_language, NoticeReasons::default()));
        Self { reasons, dismissed }
    }
}

impl EditorState {
    /// The notice's reasons not dismissed.
    pub(super) fn edit_as_notice(&self) -> NoticeReasons {
        let edit = self.sync.edit_language.get();
        let reasons = self.edit_as.reasons.get();
        self.edit_as.dismissed.with(|(language, dismissed)| {
            if *language == edit {
                reasons.without(*dismissed)
            } else {
                reasons
            }
        })
    }

    /// Hides the notice for what it says now; a reason it did not give yet
    /// shows it again.
    pub(super) fn dismiss_edit_as_notice(&self) {
        let edit = self.sync.edit_language.get_untracked();
        let shown = self.edit_as.reasons.get_untracked();
        self.edit_as.dismissed.update(|(language, dismissed)| {
            if *language != edit {
                *language = edit;
                *dismissed = NoticeReasons::default();
            }
            dismissed.host_comments |= shown.host_comments;
            dismissed.typed_comments |= shown.typed_comments;
        });
    }
}
