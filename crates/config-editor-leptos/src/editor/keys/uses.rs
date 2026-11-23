//! `Shift+F12` (Find uses at the caret), and the keys of the list of uses
//! while it is open: the focus stays in the text, as with the suggestions,
//! and the arrows move through the list.

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::state::EditorState;

/// `Shift+F12`: the uses of the name at the caret. Where the schema names
/// keys, finding none says so; where it names none, the key is left to the
/// browser.
pub(super) fn find_uses(state: EditorState, event: &KeyboardEvent) -> bool {
    let shift_only =
        event.shift_key() && !event.ctrl_key() && !event.meta_key() && !event.alt_key();
    if event.key() != "F12" || !shift_only {
        return false;
    }
    if !state.find_uses() {
        if !state.names_keys() {
            return false;
        }
        state.say_no_uses();
    }
    event.prevent_default();
    true
}

/// The keys of the open list: arrows, `Home`/`End` and the page keys move
/// through it, `Enter` goes to the use on, `Escape` and `Tab` close it.
/// Any other key closes it and acts as usual (a modifier alone keeps it).
pub(super) fn list_keys(state: EditorState, event: &KeyboardEvent) -> bool {
    let uses = state.uses();
    if !uses.is_open() {
        return false;
    }
    let count = uses.list.with_untracked(|list| {
        list.as_ref()
            .map_or(0, super::super::key_names::UsesList::count)
    });
    let active = uses.active.get_untracked();
    let plain = !event.ctrl_key() && !event.meta_key() && !event.alt_key();
    let step = |to: usize| {
        if count > 0 && to != active {
            uses.active.set(to);
        }
    };
    match event.key().as_str() {
        "ArrowDown" if plain => step(if active + 1 >= count { 0 } else { active + 1 }),
        "ArrowUp" if plain => step(if active == 0 {
            count.saturating_sub(1)
        } else {
            active - 1
        }),
        "PageDown" if plain => step((active + PAGE).min(count.saturating_sub(1))),
        "PageUp" if plain => step(active.saturating_sub(PAGE)),
        "Home" if plain => step(0),
        "End" if plain => step(count.saturating_sub(1)),
        "Enter" if plain => {
            if !state.go_to_use(active) {
                state.close_uses();
            }
        }
        "Escape" | "Tab" => {
            state.close_uses();
            // Tab only closes: it neither indents nor leaves.
        }
        "Shift" | "Control" | "Alt" | "Meta" => return true,
        _ => {
            state.close_uses();
            return false;
        }
    }
    event.prevent_default();
    true
}

/// Rows a page key moves over.
const PAGE: usize = 8;
