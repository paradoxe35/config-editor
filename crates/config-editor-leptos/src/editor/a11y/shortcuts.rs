//! The keys that work in the text, named on the textarea
//! (`aria-keyshortcuts`) so a screen reader can tell them to its user.
//! Only single chords are listed (the attribute cannot spell the
//! two-step `Ctrl+K Ctrl+I`), and only those the editor takes in this
//! build and with these settings: a key it leaves to the browser (find with
//! `search` off, save without `on_save`) is not named. Set here rather than
//! in the view: a tachys element holds at most 26 attributes, and the
//! textarea's are taken.

use leptos::prelude::*;

use super::super::state::EditorState;

/// The chord with `Cmd` in place of `Ctrl` as well, where the editor takes
/// both (`Ctrl+Y`, `Ctrl+Space` and `Ctrl+M` are `Ctrl` only; see the
/// README's keys).
fn control_or_meta(key: &str) -> [String; 2] {
    [format!("Control+{key}"), format!("Meta+{key}")]
}

/// The chords in effect now, space-separated as `aria-keyshortcuts` is.
fn chords(state: EditorState) -> String {
    let settings = state.config().settings;
    let mut chords: Vec<String> = Vec::new();
    let editable = !state.config().read_only.get();
    if cfg!(feature = "search") && settings.search.get() {
        chords.extend(control_or_meta("F"));
        if editable {
            chords.extend(control_or_meta("H"));
        }
    }
    if cfg!(feature = "search") && settings.go_to_line.get() {
        chords.extend(control_or_meta("G"));
    }
    if editable && settings.completions.get() {
        chords.push("Control+Space".to_string());
    }
    if state.editing().on_save.is_some() && settings.save_shortcut.get() {
        chords.extend(control_or_meta("S"));
    }
    chords.push("Control+M".to_string());
    chords.extend(["F8", "Shift+F8"].map(String::from));
    if cfg!(feature = "diff") && settings.diff.get() {
        chords.extend(["Alt+F5", "Shift+Alt+F5"].map(String::from));
    }
    chords.join(" ")
}

/// Keeps the textarea's `aria-keyshortcuts` in step with the settings.
pub(in crate::editor) fn connect_shortcuts(state: EditorState) {
    Effect::new(move |_| {
        let Some(element) = state.view().textarea_ref.get() else {
            return;
        };
        let _ = element.set_attribute("aria-keyshortcuts", &chords(state));
    });
}
