//! The short tooltip of protected text: why an edit was refused (at the
//! caret, for a moment), or what the hovered protected text is.

use leptos::prelude::*;

use super::super::completion::visual_column;
use super::super::keyboard::utf16_to_byte_index;
use super::super::positioning::hover_card_style;
use super::super::state::EditorState;
use super::live::Tip;

#[component]
pub(in crate::editor) fn ProtectTipView(state: EditorState) -> impl IntoView {
    move || {
        let tip = state.protect().tip.get()?;
        let style = tip_style(state, &tip)?;
        Some(view! {
            <div
                class="config-editor__protect-tip"
                class:config-editor__protect-tip--refused=!tip.hover
                role=if tip.hover { "tooltip" } else { "status" }
                aria-live="polite"
                style=style
            >
                {tip.message}
            </div>
        })
    }
}

/// Where the tooltip goes: under (or above) the text it points at.
fn tip_style(state: EditorState, tip: &Tip) -> Option<String> {
    let element = state.textarea()?;
    state.view().layout_revision.track();
    let (line, column) = state.doc().text.with_untracked(|text| {
        let byte = utf16_to_byte_index(text, tip.offset);
        let line_start = text[..byte].rfind('\n').map_or(0, |at| at + 1);
        (
            text[..line_start].matches('\n').count(),
            visual_column(&text[line_start..byte]) as usize,
        )
    });
    Some(hover_card_style(&element, line, column))
}
