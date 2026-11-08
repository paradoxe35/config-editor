//! The textarea the user types in; it sits transparent over the code layer.

use leptos::prelude::*;

use super::super::state::EditorState;
use super::super::{input, keys};

#[component]
pub(crate) fn SourceInput(state: EditorState) -> impl IntoView {
    let read_only = state.config().read_only;
    // A tachys element holds at most 26 attributes; clipboard and
    // composition listeners are added in `input::attach_listeners`.
    view! {
        <textarea
            class="config-editor__input"
            style:--config-editor-content-height=move || {
                format!(
                    "calc(var(--config-editor-padding-top) + var(--config-editor-padding-bottom) + {} * var(--config-editor-line-height))",
                    state.view().projection.with(|projection| projection.rows()),
                )
            }
            // A text box of several lines that suggests, as CodeMirror's
            // and Monaco's are. Not a combobox: screen readers take one for
            // a single-line field (reading and moving by line break), and
            // `aria-expanded` is not one of a text box's states. The open
            // list shows by `aria-controls` and `aria-activedescendant`,
            // set only while it is open (see `a11y::suggestions`).
            role="textbox"
            aria-multiline="true"
            aria-label=state.language_text(|strings| &strings.source_label)
            aria-autocomplete=move || (!read_only.get()).then_some("list")
            aria-haspopup=move || (!read_only.get()).then_some("listbox")
            aria-invalid=move || (!state.problems().diagnostics.with(Vec::is_empty)).to_string()
            // What is at the caret: its problems, a read-only reason, a
            // secret's hint (see `a11y`).
            aria-describedby=state.a11y().caret_note_id()
            node_ref=state.view().textarea_ref
            spellcheck="false"
            autocapitalize="off"
            autocomplete="off"
            wrap="off"
            // Read-only cancels edits in `beforeinput` instead of setting
            // `readonly`, which in Chromium also stops the caret.
            aria-readonly=move || read_only.get().to_string()
            inputmode=move || read_only.get().then_some("none")
            prop:value=move || {
                let projection = state.view().projection.get();
                state.doc().text.with(|doc| projection.view(doc).to_string())
            }
            on:beforeinput=move |event| input::before_input(state, &event)
            on:input=move |event| input::input(state, &event)
            on:mousemove=move |event| input::mouse_move(state, &event)
            on:mouseleave=move |_| input::mouse_leave(state)
            on:blur=move |event| input::blur(state, &event)
            on:click=move |event| input::click(state, &event)
            on:keyup=move |event| input::key_up(state, &event)
            on:scroll=move |event| input::scroll(state, &event)
            on:keydown=move |event| keys::on_keydown(state, &event)
        >
            // The text is there as soon as the element is: `prop:value`
            // applies a moment later, and input or a host reading the
            // textarea in between must not see it empty.
            {state.doc().text.get_untracked()}
        </textarea>
    }
}
