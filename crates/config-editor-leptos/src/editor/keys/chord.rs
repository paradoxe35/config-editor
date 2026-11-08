//! `Ctrl+K` chords, as in VS Code (`Cmd+K` on macOS): `Ctrl+K Ctrl+I`
//! shows the hover card (schema docs) for the key at the caret, and with
//! folding on `Ctrl+K Ctrl+0` folds everything and `Ctrl+K Ctrl+J` unfolds
//! everything (see `folding`).

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::state::EditorState;

/// The chord keys: `Ctrl+K` arms a chord (with something to finish it: hover
/// docs or folding on), the next key finishes it. Returns whether the key
/// was taken.
pub(super) fn chord(state: EditorState, event: &KeyboardEvent) -> bool {
    let key = event.key();
    let command = (event.ctrl_key() || event.meta_key()) && !event.alt_key();
    let chord = state.typing().chord;
    if chord.get_value() && !matches!(key.as_str(), "Control" | "Meta" | "Shift" | "Alt") {
        chord.set_value(false);
        if command && !event.shift_key() && key.eq_ignore_ascii_case("i") {
            event.prevent_default();
            state.show_hover_at_caret();
            return true;
        }
        if super::super::folding::chord_key(state, event) {
            return true;
        }
        // Any other key ends the chord and does nothing else, as in VS
        // Code ("not a command"): `Ctrl+K X` types no `x`.
        event.prevent_default();
        return true;
    }
    let settings = &state.config().settings;
    let finishable = settings.hover_docs.get_untracked() || settings.folding.get_untracked();
    if command && !event.shift_key() && key.eq_ignore_ascii_case("k") && finishable {
        event.prevent_default();
        chord.set_value(true);
        return true;
    }
    false
}
