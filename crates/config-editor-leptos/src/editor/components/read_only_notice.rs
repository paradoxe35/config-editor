//! "Cannot edit in a read-only editor": said, and shown at the caret for a
//! moment, when typing, pasting, cutting or deleting in a read-only editor
//! (as VS Code does), so the refused key is not silent. With the host's
//! `read_only_reason` it says why, which the text area's accessible
//! description holds too.

use leptos::prelude::*;

use super::super::caret_tip::caret_tip_style;
use super::super::state::EditorState;
use crate::cssom::cssom;
use crate::reactive::later;
use crate::strings::fill;

/// How long the notice shows, in milliseconds.
const NOTICE_MS: i32 = 2500;

impl EditorState {
    /// An edit was refused because the editor is read-only: says so, and
    /// shows it at the caret for a moment.
    pub(in crate::editor) fn note_read_only(&self) {
        let a11y = self.a11y();
        let said = untrack(|| self.read_only_message());
        a11y.announce(said);
        a11y.read_only_notice
            .set(Some(self.doc().caret.get_untracked()));
        if later::defers() {
            let timer = later::after(a11y.read_only_notice, NOTICE_MS, |notice| notice.set(None));
            if let Some(old) = a11y
                .notice_timer
                .try_update_value(|kept| std::mem::replace(kept, timer))
            {
                old.cancel();
            }
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

    /// What the notice says: that the editor is read-only, and why when
    /// the host said. Tracked.
    pub(in crate::editor) fn read_only_message(&self) -> String {
        let reason = self.read_only_reason();
        self.strings(|strings| match reason {
            Some(reason) => fill(&strings.read_only_editor_reason, &[("reason", &reason)]),
            None => strings.read_only_editor.clone(),
        })
    }

    /// Puts the notice away (any key that is not more typing).
    pub(in crate::editor) fn hide_read_only_notice(&self) {
        let notice = self.a11y().read_only_notice;
        if notice.with_untracked(Option::is_some) {
            notice.set(None);
        }
    }
}

#[component]
pub(crate) fn ReadOnlyNotice(state: EditorState) -> impl IntoView {
    move || {
        let at = state.a11y().read_only_notice.get()?;
        let style = caret_tip_style(state, at)?;
        // The live region says it; this is for the eyes.
        Some(view! {
            <div class="config-editor__read-only-notice" aria-hidden="true" style=cssom(style)>
                {untrack(|| state.read_only_message())}
            </div>
        })
    }
}
