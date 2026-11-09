//! `Ctrl+K` chords, as in VS Code (`Cmd+K` on macOS): `Ctrl+K Ctrl+I`
//! shows the hover card (schema docs) for the key at the caret with the
//! problems there (or their tip, without docs), and again focuses that
//! card to scroll it (see `hover_focus`); with
//! folding on `Ctrl+K Ctrl+0` folds everything and `Ctrl+K Ctrl+J` unfolds
//! everything (see `folding`).

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::state::EditorState;

/// Whether the key is `Ctrl+K`, or the `Ctrl+I` finishing the chord, while
/// the card shown at the caret is open: the card stays for the chord to
/// focus it (see `hover_focus`), with the problems it lists.
pub(super) fn keeps_card(state: EditorState, event: &KeyboardEvent) -> bool {
    let command = (event.ctrl_key() || event.meta_key()) && !event.alt_key() && !event.shift_key();
    let key = event.key();
    let wanted = if state.typing().chord.get_value() {
        "i"
    } else {
        "k"
    };
    let card =
        state.hover().at_caret.get_value() && state.hover().card.with_untracked(Option::is_some);
    // The problem tip F8 or the chord showed at the caret too.
    let tip = state.problems().current.with_untracked(Option::is_some);
    command && key.eq_ignore_ascii_case(wanted) && (card || tip)
}

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
            // Again while its card shows: into the card, to scroll it.
            if !state.focus_card() && !state.focus_problem_tip() {
                super::super::a11y::show_at_caret(state);
            }
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
