//! Moving between changes, as in VS Code: `Alt+F5` / `Shift+Alt+F5` to the
//! next / previous one, `Alt+F3` / `Shift+Alt+F3` also opening its preview.

use web_sys::KeyboardEvent;

use super::super::state::EditorState;

/// The change keys; they move the caret only, so they work read-only too.
pub(in crate::editor) fn go_to_change(state: EditorState, event: &KeyboardEvent) -> bool {
    if !event.alt_key() || event.ctrl_key() || event.meta_key() || !state.features.diff {
        return false;
    }
    let preview = match event.code().as_str() {
        "F5" => false,
        "F3" => true,
        _ => return false,
    };
    event.prevent_default();
    state.go_to_change(!event.shift_key(), preview);
    true
}
