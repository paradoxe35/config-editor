//! The short tooltip of protected text: why an edit was refused (at the
//! caret, for a moment), or what the hovered protected text is.

use crate::cssom::cssom;
use leptos::prelude::*;

use super::super::caret_tip::caret_tip_style;
use super::super::state::EditorState;

#[component]
pub(in crate::editor) fn ProtectTipView(state: EditorState) -> impl IntoView {
    // Why an edit was refused is said through the editor's live region,
    // which is there from the start (a region made together with its
    // text, and again for each refused key, may never be read), as the
    // read-only editor's refusal is.
    Effect::new(move |_| {
        if let Some(tip) = state.protect().tip.get().filter(|tip| !tip.hover) {
            state.a11y().announce(tip.message);
        }
    });
    move || {
        let tip = state.protect().tip.get()?;
        let style = caret_tip_style(state, tip.offset)?;
        Some(view! {
            <div
                class="config-editor__protect-tip"
                class:config-editor__protect-tip--refused=!tip.hover
                role=tip.hover.then_some("tooltip")
                // Said already (see above).
                aria-hidden=(!tip.hover).then_some("true")
                style=cssom(style)
            >
                {tip.message}
            </div>
        })
    }
}
