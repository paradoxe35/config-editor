//! The short tooltip of protected text: why an edit was refused (at the
//! caret, for a moment), or what the hovered protected text is.

use crate::cssom::cssom;
use leptos::prelude::*;

use super::super::caret_tip::caret_tip_style;
use super::super::state::EditorState;

#[component]
pub(in crate::editor) fn ProtectTipView(state: EditorState) -> impl IntoView {
    move || {
        let tip = state.protect().tip.get()?;
        let style = caret_tip_style(state, tip.offset)?;
        Some(view! {
            <div
                class="config-editor__protect-tip"
                class:config-editor__protect-tip--refused=!tip.hover
                role=if tip.hover { "tooltip" } else { "status" }
                aria-live="polite"
                style=cssom(style)
            >
                {tip.message}
            </div>
        })
    }
}
