//! A short notice at the caret, said and shown for a moment, so a key that
//! did nothing is not silent (as VS Code does): "Cannot edit in a
//! read-only editor" when typing, pasting, cutting or deleting in a
//! read-only editor (with the host's `read_only_reason`, which the text
//! area's accessible description holds too, it says why), "No
//! suggestions" for `Ctrl+Space`, "No definition for 'name'" for `F12`.

use leptos::prelude::*;

use super::super::popups::caret_tip_style;
use super::super::state::EditorState;
use crate::cssom::cssom;
use crate::editor::text::utf16_of;
use crate::reactive::later;
use crate::strings::fill;

/// How long the notice shows, in milliseconds.
const NOTICE_MS: i32 = 2500;

/// The notice shown at the caret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) struct CaretNotice {
    pub(in crate::editor) message: String,
    /// Where it points, in UTF-16 units of the text.
    pub(in crate::editor) offset: usize,
    /// An edit was refused (the read-only editor's notice), not merely
    /// nothing found.
    pub(in crate::editor) refused: bool,
}

impl EditorState {
    /// An edit was refused because the editor is read-only: says so, and
    /// shows it at the caret for a moment.
    pub(in crate::editor) fn note_read_only(&self) {
        let message = untrack(|| self.read_only_message());
        self.show_caret_notice(message, true);
    }

    /// A key found nothing to do (no suggestions, no definition): says
    /// `message`, and shows it at the caret for a moment.
    pub(in crate::editor) fn note_nothing_found(&self, message: String) {
        self.show_caret_notice(message, false);
    }

    fn show_caret_notice(&self, message: String, refused: bool) {
        let a11y = self.a11y();
        a11y.announce(message.clone());
        let caret = self.doc().caret.get_untracked();
        let offset = self.doc().text.with_untracked(|text| utf16_of(text, caret));
        a11y.caret_notice.set(Some(CaretNotice {
            message,
            offset,
            refused,
        }));
        if later::defers() {
            let timer = later::after(a11y.caret_notice, NOTICE_MS, |notice| notice.set(None));
            later::replace_in(a11y.notice_timer, timer);
        }
    }

    /// Why the editor is read-only, while it is and the host said why.
    /// Tracked.
    pub(in crate::editor) fn read_only_reason(&self) -> Option<String> {
        let config = self.config();
        if !config.read_only.get() {
            return None;
        }
        config
            .read_only_reason
            .get()
            .filter(|reason| !reason.trim().is_empty())
    }

    /// What the read-only notice says: that the editor is read-only, and
    /// why when the host said. Tracked.
    pub(in crate::editor) fn read_only_message(&self) -> String {
        let reason = self.read_only_reason();
        self.strings(|strings| match reason {
            Some(reason) => fill(&strings.read_only_editor_reason, &[("reason", &reason)]),
            None => strings.read_only_editor.clone(),
        })
    }

    /// Puts the notice away (any key that is not more typing).
    pub(in crate::editor) fn hide_caret_notice(&self) {
        let notice = self.a11y().caret_notice;
        if notice.with_untracked(Option::is_some) {
            notice.set(None);
        }
    }
}

#[component]
pub(crate) fn CaretNoticeView(state: EditorState) -> impl IntoView {
    move || {
        let notice = state.a11y().caret_notice.get()?;
        let style = caret_tip_style(state, notice.offset)?;
        let class = if notice.refused {
            "config-editor__read-only-notice"
        } else {
            "config-editor__caret-notice"
        };
        // The live region says it; this is for the eyes.
        Some(view! {
            <div class=class aria-hidden="true" style=cssom(style)>
                {notice.message}
            </div>
        })
    }
}
