//! Keyboard handling for the textarea, one concern per module. Each handler
//! returns `true` when it took the key, and [`on_keydown`] stops there; the
//! order below is the order keys are matched in.

mod chord;
pub(super) mod complete;
pub(super) mod enter;
mod navigation;
mod page_keys;
mod suggestions;
mod tab;
mod typing;

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::state::EditorState;

pub(super) fn on_keydown(state: EditorState, event: &KeyboardEvent) {
    // Keys act on the document as the browser last left it.
    super::input::flush_input(state);
    // Before anything closes: an Escape only arms Tab to leave when it had
    // nothing to close.
    // Before the card hides: Alt+Enter opens its link.
    if super::a11y::focus_keys(state, event) || super::a11y::open_docs_key(state, event) {
        return;
    }
    // A modifier alone keeps the card: Alt for Alt+Enter, which opens its
    // link, Ctrl for a chord; so does `Ctrl+K Ctrl+I` again, to focus it.
    let keeps_card = chord::keeps_card(state, event);
    if !matches!(event.key().as_str(), "Shift" | "Control" | "Alt" | "Meta") && !keeps_card {
        state.hide_hover();
    }
    state.protect().hide_tip(false);
    // Typing refused at a read-only field ends with any key that is not
    // more of it (see `note_refused`).
    if matches!(
        event.key().as_str(),
        "ArrowLeft"
            | "ArrowRight"
            | "ArrowUp"
            | "ArrowDown"
            | "Home"
            | "End"
            | "PageUp"
            | "PageDown"
            | "Backspace"
            | "Delete"
            | "Escape"
            | "Tab"
    ) {
        state.forget_refusal();
    }
    page_keys::keep_page_still(state, event);
    if !matches!(
        event.key().as_str(),
        "F8" | "Shift" | "Control" | "Alt" | "Meta"
    ) && !keeps_card
    {
        state.hide_shown_problem();
    }
    if chord::chord(state, event) || super::folding::shortcuts(state, event) {
        return;
    }
    if state.step_over_token(event)
        || navigation::smart_home(state, event)
        || navigation::keep_placeholder(state, event)
        || super::changes::go_to_change(state, event)
    {
        return;
    }
    if state.config().read_only.get_untracked() {
        // Navigation works as usual: Escape collapses a selection (the root
        // listener closes popups); suggestions never open.
        if event.key() == "Escape" {
            event.prevent_default();
            navigation::escape(state, event);
        } else if event.ctrl_key() && event.code() == "Space" {
            event.prevent_default();
        }
        return;
    }
    super::folding::unfold_before_edit(state, event);
    let handled = navigation::undo_redo(state, event)
        || suggestions::request(state, event)
        || navigation::next_problem(state, event)
        || typing::toggle_comments(state, event)
        || typing::join_with_next_line(state, event)
        || (state.config().settings.auto_closing.get_untracked()
            && typing::pairs_and_tags(state, event))
        || typing::indentation(state, event);
    if handled {
        return;
    }
    let Some(options_open) = suggestions::step_past_unchanged(state, event) else {
        return;
    };
    if options_open && suggestions::list_keys(state, event) {
        return;
    }
    match event.key().as_str() {
        "Escape" => navigation::escape(state, event),
        "Tab" => tab::tab(state, event),
        "Enter" => enter::enter(state, event),
        _ => {}
    }
}

/// Keys on the editor root, from the text or a widget: find and replace
/// shortcuts, saving, `F6` to the baseline notice and back; Escape closes
/// every popup.
pub(super) fn on_root_keydown(state: EditorState, event: &KeyboardEvent) {
    if super::search::shortcuts(state, event)
        || save_key(state, event)
        || state.notice_shortcut(event)
    {
        return;
    }
    if event.key() == "Escape" {
        event.prevent_default();
        // From a link in the problem tip (the hover card's hands the focus
        // back itself): the text takes the focus back, not the page.
        let from_tip = from_problem_tip(event);
        state.close_suggestions();
        state.completion().explicit.set(false);
        super::changes::close_preview(state);
        state.hide_hover();
        state.hide_shown_problem();
        if from_tip && let Some(element) = state.textarea() {
            let _ = element.focus_in_place();
        }
    }
}

/// Whether `event` comes from inside the problem tip.
fn from_problem_tip(event: &KeyboardEvent) -> bool {
    use wasm_bindgen::JsCast;
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|target| target.closest(".config-editor__problem-tip").ok().flatten())
        .is_some()
}

/// `Ctrl+S` / `Cmd+S` saves through `on_save`, instead of the browser's
/// save dialog; without `on_save` the browser keeps the key.
fn save_key(state: EditorState, event: &KeyboardEvent) -> bool {
    let command = (event.ctrl_key() || event.meta_key()) && !event.alt_key() && !event.shift_key();
    let enabled =
        state.editing().on_save.is_some() && state.editing().save_shortcut.get_untracked();
    if !enabled || !command || !event.key().eq_ignore_ascii_case("s") {
        return false;
    }
    event.prevent_default();
    state.save();
    true
}

/// Ctrl, Cmd, Alt or Shift held.
fn modified(event: &KeyboardEvent) -> bool {
    event.ctrl_key() || event.meta_key() || event.alt_key() || event.shift_key()
}

/// No Ctrl, Cmd or Alt held.
pub(super) fn unmodified(event: &KeyboardEvent) -> bool {
    !event.ctrl_key() && !event.meta_key() && !event.alt_key()
}

/// The single character a key types, if it types one.
pub(super) fn typed_character(event: &KeyboardEvent) -> Option<char> {
    let key = event.key();
    let mut characters = key.chars();
    let character = characters.next()?;
    characters.next().is_none().then_some(character)
}
