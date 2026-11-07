//! The go-to-line field (`Ctrl+G`) over the top of the text.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::super::state::{EditorState, line_count};
use super::super::go_to_line::{self, hint};

#[component]
pub(crate) fn GoToLineWidget(state: EditorState) -> impl IntoView {
    let go_to = state.go_to_line;
    let lines = move || state.doc.text.with(|text| line_count(text));
    let hint_id = format!("config-editor-goto-hint-{}", go_to.id);
    view! {
        <div
            class="config-editor__goto"
            role="dialog"
            aria-label="Go to line"
            node_ref=go_to.widget_ref
            prop:hidden=move || !go_to.open.get()
        >
            <input
                type="text"
                class="config-editor__find-input config-editor__goto-input"
                aria-label="Go to line"
                aria-describedby=hint_id.clone()
                placeholder="line or line:column"
                spellcheck="false"
                autocomplete="off"
                node_ref=go_to.input_ref
                prop:value=move || go_to.text.get()
                on:input=move |event| go_to.text.set(event_target_value(&event))
                on:keydown=move |event| field_keys(state, &event)
                on:blur=move |_| go_to_line::cancel_on_blur(state)
            />
            <span id=hint_id class="config-editor__goto-hint" aria-live="polite">
                {move || go_to.text.with(|text| hint(text, lines()))}
            </span>
        </div>
    }
}

/// Enter goes to the typed position, Escape cancels.
fn field_keys(state: EditorState, event: &KeyboardEvent) {
    match event.key().as_str() {
        "Enter" => {
            event.prevent_default();
            go_to_line::go(state);
        }
        "Escape" => {
            event.prevent_default();
            event.stop_propagation();
            go_to_line::cancel(state);
        }
        _ => {}
    }
}
