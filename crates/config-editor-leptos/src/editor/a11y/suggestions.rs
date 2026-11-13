//! The suggestion list, as a screen reader follows it: the textarea names
//! the list it controls (`aria-controls`) and, while the list is open, the
//! highlighted suggestion (`aria-activedescendant`), so moving through the
//! list with the arrows reads each suggestion while the focus stays in the
//! text. Set here rather than in the view: a tachys element holds at most
//! 26 attributes, and the textarea's are taken.

use leptos::prelude::*;

use super::super::state::EditorState;
use super::A11yState;

impl A11yState {
    /// The id of the suggestion list.
    pub(in crate::editor) fn suggestions_id(&self) -> String {
        format!("config-editor-{}-suggestions", self.id)
    }

    /// The id of suggestion `index` in the list.
    pub(in crate::editor) fn suggestion_id(&self, index: usize) -> String {
        format!("config-editor-{}-suggestion-{index}", self.id)
    }
}

/// Keeps the textarea's `aria-controls` and `aria-activedescendant` in
/// step with the list: both only while it is open, which is how the open
/// state shows (CodeMirror's pattern for a multi-line editor; see
/// `components::source` for why the textarea is no combobox).
pub(in crate::editor) fn connect_suggestions(state: EditorState) {
    Effect::new(move |_| {
        let Some(element) = state.view().textarea_ref.get() else {
            return;
        };
        let open = !state.completion().suggestions.with(Vec::is_empty);
        let active = state.completion().active.get();
        if open {
            let _ = element.set_attribute("aria-controls", &state.a11y().suggestions_id());
            let _ =
                element.set_attribute("aria-activedescendant", &state.a11y().suggestion_id(active));
        } else {
            let _ = element.remove_attribute("aria-controls");
            let _ = element.remove_attribute("aria-activedescendant");
        }
    });
}

/// Typing filtered a list the user asked for or moved in (`listed`) down to
/// nothing: it hides, and a screen reader is told why (the find widget says
/// "No results" likewise). A list that opened by itself closes silently.
pub(in crate::editor) fn say_no_suggestions_after(state: EditorState, listed: bool) {
    if listed && state.completion().requested.get_untracked() && !state.suggestions_showing() {
        let said = state.strings_untracked(|strings| strings.no_suggestions.clone());
        state.a11y().announce(said);
    }
}
