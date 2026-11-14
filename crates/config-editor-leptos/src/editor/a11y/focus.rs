//! Leaving the editor with the keyboard. Tab indents while editing (a
//! focus trap otherwise), so there are two ways out, as in VS Code:
//!
//! - `Ctrl+M` switches Tab to moving the focus, and back. Tab still
//!   accepts a suggestion while the list is open.
//! - Escape, then Tab: an Escape that had nothing to close (no
//!   suggestions, find widget, hover card, preview or problem message)
//!   lets the next Tab or Shift+Tab leave, once.
//!
//! Leaving is the browser's own Tab: the key is not cancelled, so the
//! focus goes to the next (or previous) focusable element of the page.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::state::EditorState;

/// What the editor looks like to the key.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Context {
    /// `Ctrl+M` mode is on.
    pub(super) tab_moves_focus: bool,
    /// `Ctrl+M` works here (the keys table's `When` for it holds): not in
    /// a read-only editor, whose Tab always leaves.
    pub(super) toggles: bool,
    /// The previous key was an Escape with nothing to close.
    pub(super) escaped: bool,
    /// The suggestion list shows options.
    pub(super) list_open: bool,
    /// Something Escape would close is open (the list included).
    pub(super) popup_open: bool,
}

/// What a key does here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    /// `Ctrl+M`: switch the Tab mode.
    Toggle,
    /// Let the browser move the focus.
    Leave,
    /// Not ours: the editor's keys handle it.
    Pass,
}

/// The modifier keys of a key press.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Modifiers {
    pub(super) ctrl: bool,
    pub(super) alt: bool,
    pub(super) meta: bool,
    pub(super) shift: bool,
}

/// What `key` does, and whether a Tab after it leaves (the Escape state).
pub(super) fn decide(key: &str, modifiers: Modifiers, context: Context) -> (Action, bool) {
    let Modifiers {
        ctrl,
        alt,
        meta,
        shift,
    } = modifiers;
    match key {
        "m" | "M" if ctrl && !alt && !meta && !shift && context.toggles => (Action::Toggle, false),
        // Shift for Shift+Tab, and the other modifiers alone, keep it.
        "Shift" | "Control" | "Alt" | "Meta" => (Action::Pass, context.escaped),
        "Escape" => (Action::Pass, !context.popup_open),
        "Tab" if !ctrl && !alt && !meta => {
            let leave = (context.tab_moves_focus && !context.list_open)
                || (context.escaped && !context.popup_open);
            (if leave { Action::Leave } else { Action::Pass }, false)
        }
        _ => (Action::Pass, false),
    }
}

/// Whether anything Escape closes is open.
fn popup_open(state: EditorState) -> bool {
    state.suggestions_showing()
        || super::super::search::is_open(state)
        || state.hover().card.with_untracked(Option::is_some)
        || super::super::changes::preview_open(state)
        || state.problems().current.with_untracked(Option::is_some)
        || state.problems().hovered.with_untracked(Option::is_some)
        || state.protect().tip_open()
}

/// The focus keys, first among the textarea's: `true` when the key was
/// taken (the Tab mode switched, or Tab leaving the editor).
pub(in crate::editor) fn focus_keys(state: EditorState, event: &KeyboardEvent) -> bool {
    let a11y = state.a11y();
    // The keys table is asked only for the key itself.
    let toggle_key = event.ctrl_key() && matches!(event.key().as_str(), "m" | "M");
    let context = Context {
        tab_moves_focus: a11y.tab_moves_focus.get_untracked(),
        toggles: toggle_key && super::shortcuts::works_now(state, "Ctrl+M"),
        escaped: a11y.escaped.get_value(),
        list_open: !state.completion().suggestions.with_untracked(Vec::is_empty),
        popup_open: popup_open(state),
    };
    let modifiers = Modifiers {
        ctrl: event.ctrl_key(),
        alt: event.alt_key(),
        meta: event.meta_key(),
        shift: event.shift_key(),
    };
    let (action, escaped) = decide(&event.key(), modifiers, context);
    a11y.escaped.set_value(escaped);
    match action {
        Action::Toggle => {
            event.prevent_default();
            toggle_tab_focus(state);
            true
        }
        Action::Leave => {
            state.close_suggestions();
            // The change preview comes right after the text (see
            // `changes`); otherwise the browser moves the focus on.
            if !event.shift_key() && super::super::changes::tab_into_preview(state) {
                event.prevent_default();
            }
            true
        }
        Action::Pass => false,
    }
}

/// Switches whether Tab moves the focus, and says so.
pub(in crate::editor) fn toggle_tab_focus(state: EditorState) {
    let on = !state.a11y().tab_moves_focus.get_untracked();
    state.a11y().tab_moves_focus.set(on);
    let said = state.strings_untracked(|strings| {
        if on {
            strings.tab_focus_on.clone()
        } else {
            strings.tab_focus_off.clone()
        }
    });
    state.a11y().announce(said);
}
