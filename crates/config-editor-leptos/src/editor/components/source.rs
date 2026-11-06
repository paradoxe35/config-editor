//! The textarea the user types in; it sits transparent over the code layer.

use leptos::prelude::*;

use super::super::state::EditorState;
use super::super::{input, keys};

#[component]
pub(crate) fn SourceInput(state: EditorState) -> impl IntoView {
    let read_only = state.read_only;
    // A tachys element holds at most 26 attributes; clipboard and
    // composition listeners are added in `input::attach_listeners`.
    view! {
        <textarea
            class="config-editor__input"
            style=move || {
                format!(
                    "--config-editor-content-height: calc(var(--config-editor-padding-top) + var(--config-editor-padding-bottom) + {} * var(--config-editor-line-height));",
                    state.source.get().split('\n').count(),
                )
            }
            aria-label=move || format!("{} source", state.label())
            aria-autocomplete=move || (!read_only.get()).then_some("list")
            aria-haspopup=move || (!read_only.get()).then_some("listbox")
            aria-expanded=move || (!state.suggestions.get().is_empty()).to_string()
            aria-invalid=move || (!state.diagnostics.get().is_empty()).to_string()
            node_ref=state.editor_ref
            spellcheck="false"
            autocapitalize="off"
            autocomplete="off"
            wrap="off"
            // Read-only cancels edits in `beforeinput` instead of setting
            // `readonly`, which in Chromium also stops the caret.
            aria-readonly=move || read_only.get().to_string()
            inputmode=move || read_only.get().then_some("none")
            prop:value=move || state.source.get()
            on:beforeinput=move |event| input::before_input(state, &event)
            on:input=move |event| input::input(state, &event)
            on:mousemove=move |event| input::mouse_move(state, &event)
            on:mouseleave=move |_| state.hide_hover()
            on:blur=move |_| input::blur(state)
            on:click=move |event| input::click(state, &event)
            on:keyup=move |event| input::key_up(state, &event)
            on:scroll=move |event| input::scroll(state, &event)
            on:keydown=move |event| keys::on_keydown(state, &event)
        >
            // The text is there as soon as the element is: `prop:value`
            // applies a moment later, and input or a host reading the
            // textarea in between must not see it empty.
            {state.source.get_untracked()}
        </textarea>
    }
}
