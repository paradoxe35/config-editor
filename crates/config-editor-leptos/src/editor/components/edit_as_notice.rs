//! The notice under the toolbar while editing as another language: the
//! host file's comments are not shown here, comments typed here are not
//! saved. A polite status, dismissible.

use leptos::prelude::*;

use super::super::state::EditorState;

#[component]
pub(crate) fn EditAsNotice(state: EditorState) -> impl IntoView {
    let shown = Memo::new(move |_| state.edit_as_notice());
    let message = move || {
        let reasons = shown.get();
        let edit = state.sync().edit_language.get();
        state.strings(|strings| {
            reasons
                .messages(strings, state.config().host_language, edit)
                .join(" ")
        })
    };
    let dismiss = move |_| state.dismiss_edit_as_notice();
    view! {
        <div
            class="config-editor__edit-as-notice"
            role="status"
            aria-live="polite"
            prop:hidden=move || !shown.get().any()
        >
            <span class="config-editor__edit-as-notice-message">{message}</span>
            <button
                type="button"
                class="config-editor__edit-as-notice-dismiss"
                aria-label=state.text(|strings| &strings.edit_as_dismiss)
                title=state.text(|strings| &strings.edit_as_dismiss)
                on:click=dismiss
            >
                "×"
            </button>
        </div>
    }
}
