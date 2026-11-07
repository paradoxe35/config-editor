//! The notice over the text when the host's baseline changed under unsaved
//! edits: Reload or Keep mine.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::host_editing::texts;
use super::super::state::EditorState;

#[component]
pub(crate) fn BaselineNotice(state: EditorState) -> impl IntoView {
    let editing = state.editing();
    let open = move || editing.conflict.with(Option::is_some);
    let reload = move |_| state.reload_baseline();
    let keep_mine = move |_| state.keep_mine();
    let keydown = move |event: KeyboardEvent| notice_keys(state, &event);
    view! {
        <div
            class="config-editor__notice"
            role="alert"
            node_ref=editing.notice_ref
            prop:hidden=move || !open()
            on:keydown=keydown
        >
            <span class="config-editor__notice-message">
                <strong>{texts::BASELINE_CHANGED}</strong>
                " "
                <span class="config-editor__notice-detail">{texts::BASELINE_CHANGED_DETAIL}</span>
            </span>
            <span class="config-editor__notice-actions">
                <button
                    type="button"
                    class="config-editor__notice-button config-editor__notice-reload"
                    title=texts::RELOAD_TITLE
                    on:click=reload
                >
                    {texts::RELOAD}
                </button>
                <button
                    type="button"
                    class="config-editor__notice-button config-editor__notice-keep"
                    title=texts::KEEP_MINE_TITLE
                    node_ref=editing.keep_mine_ref
                    on:click=keep_mine
                >
                    {texts::KEEP_MINE}
                </button>
            </span>
        </div>
    }
}

/// Escape keeps the text (the choice that loses nothing) and returns to it.
fn notice_keys(state: EditorState, event: &KeyboardEvent) {
    if event.key() == "Escape" {
        event.prevent_default();
        event.stop_propagation();
        state.keep_mine();
    }
}
