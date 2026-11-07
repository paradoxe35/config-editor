//! Text only screen readers get (see `a11y`): what is at the caret, which
//! the textarea's `aria-describedby` points at, and a polite live region
//! for short announcements.

use leptos::prelude::*;

use super::super::state::EditorState;

#[component]
pub(crate) fn Announcer(state: EditorState) -> impl IntoView {
    let a11y = state.a11y;
    view! {
        <div class="config-editor__sr-only" id=a11y.caret_note_id()>
            {move || a11y.caret_note.get()}
        </div>
        <div
            class="config-editor__sr-only config-editor__announcement"
            role="status"
            aria-live="polite"
            aria-atomic="true"
        >
            {move || a11y.announcement.get()}
        </div>
    }
}
