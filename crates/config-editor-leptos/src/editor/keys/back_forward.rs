//! `Alt+←` / `Alt+→` (macOS: `Ctrl+-` / `Ctrl+Shift+-`, as VS Code there,
//! since `Option+←` moves by word): go back / forward through the places
//! jumps left (see `back_forward`). With nowhere to go the key is left to
//! the browser (its own page back and forward).

use web_sys::KeyboardEvent;

use super::super::back_forward::Direction;
use super::super::state::EditorState;

pub(super) fn go_back_forward(state: EditorState, event: &KeyboardEvent) -> bool {
    let Some(direction) = direction_of(event, super::super::platform::apple_platform()) else {
        return false;
    };
    if !state.go(direction) {
        return false;
    }
    event.prevent_default();
    true
}

/// The way `event` asks to go, on an Apple platform or not.
fn direction_of(event: &KeyboardEvent, apple: bool) -> Option<Direction> {
    let (ctrl, alt, shift, meta) = (
        event.ctrl_key(),
        event.alt_key(),
        event.shift_key(),
        event.meta_key(),
    );
    if apple {
        if !ctrl || alt || meta || event.code() != "Minus" {
            return None;
        }
        return Some(if shift {
            Direction::Forward
        } else {
            Direction::Back
        });
    }
    if !alt || ctrl || shift || meta {
        return None;
    }
    match event.key().as_str() {
        "ArrowLeft" => Some(Direction::Back),
        "ArrowRight" => Some(Direction::Forward),
        _ => None,
    }
}
