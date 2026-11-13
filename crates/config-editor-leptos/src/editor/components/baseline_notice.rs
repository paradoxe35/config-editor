//! The notice over the text when the host's baseline changed under unsaved
//! edits: Reload or Keep mine. It leaves the focus where it is (see
//! `host_editing::conflict`); `F6` reaches it.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

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
                <strong>{state.text(|strings| &strings.baseline_changed)}</strong>
                " "
                <span class="config-editor__notice-detail">{state.text(|strings| &strings.baseline_changed_detail)}</span>
                {keys_hint(state)}
            </span>
            <span class="config-editor__notice-actions">
                <button
                    type="button"
                    class="config-editor__notice-button config-editor__notice-reload"
                    title=state.shortcut_text(|strings| &strings.reload_title)
                    on:click=reload
                >
                    {state.text(|strings| &strings.reload)}
                </button>
                <button
                    type="button"
                    class="config-editor__notice-button config-editor__notice-keep"
                    title=state.text(|strings| &strings.keep_mine_title)
                    node_ref=editing.keep_mine_ref
                    on:click=keep_mine
                >
                    {state.text(|strings| &strings.keep_mine)}
                </button>
            </span>
        </div>
    }
}

/// How to reach the choices from the keyboard (F6); none on a touch
/// screen, where they are tapped (the footer's hint says "tap" there too).
fn keys_hint(state: EditorState) -> Option<impl IntoView> {
    (!super::super::touch::touch_screen()).then(|| {
        view! {
            " "
            <span class="config-editor__notice-keys">{state.text(|strings| &strings.baseline_changed_keys)}</span>
        }
    })
}

/// Escape keeps the text (the choice that loses nothing) and returns to it.
fn notice_keys(state: EditorState, event: &KeyboardEvent) {
    if event.key() == "Escape" {
        event.prevent_default();
        event.stop_propagation();
        state.keep_mine();
    }
}
