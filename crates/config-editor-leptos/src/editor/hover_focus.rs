//! The keyboard in the hover card, as in VS Code: a second `Ctrl+K Ctrl+I`
//! while the card shown at the caret is open moves the focus into it,
//! where the arrow keys, Page Up, Page Down, Home and End scroll it, and
//! Escape closes it and gives the text the focus back (see
//! `keys::on_root_keydown`). Tab still goes on to its links.

use leptos::prelude::*;
use web_sys::{HtmlElement, KeyboardEvent};

use super::state::EditorState;

/// How far an arrow key scrolls the card, in pixels: about a line of it.
const LINE_STEP: i32 = 18;

impl EditorState {
    /// Moves the focus into the card shown at the caret, and says how to
    /// scroll it and get back. Returns whether it did (no card there: the
    /// chord shows one).
    pub(super) fn focus_card(&self) -> bool {
        let hover = self.hover();
        if !hover.at_caret.get_value() || hover.card.with_untracked(Option::is_none) {
            return false;
        }
        let Some(card) = hover.card_ref.get_untracked() else {
            return false;
        };
        if crate::focus::in_place(&card).is_err() {
            return false;
        }
        let said = self.strings_untracked(|strings| strings.hover_focused.clone());
        self.a11y().announce(said);
        true
    }
}

/// Where a scroll key takes the card's `scroll_top`, from `top` in a card
/// showing `room` pixels of `height`; None for other keys.
fn scrolled_to(key: &str, top: i32, room: i32, height: i32) -> Option<i32> {
    let page = (room - LINE_STEP).max(LINE_STEP);
    let to = match key {
        "ArrowDown" => top + LINE_STEP,
        "ArrowUp" => top - LINE_STEP,
        "PageDown" => top + page,
        "PageUp" => top - page,
        "Home" => 0,
        "End" => height,
        _ => return None,
    };
    Some(to.clamp(0, (height - room).max(0)))
}

/// A key in the focused card (or on one of its links): the scroll keys
/// scroll it, and only it (not the page past its end).
pub(super) fn card_keys(card: &HtmlElement, event: &KeyboardEvent) {
    if event.ctrl_key() || event.alt_key() || event.meta_key() || event.shift_key() {
        return;
    }
    let Some(to) = scrolled_to(
        &event.key(),
        card.scroll_top(),
        card.client_height(),
        card.scroll_height(),
    ) else {
        return;
    };
    event.prevent_default();
    card.set_scroll_top(to);
}

#[cfg(test)]
mod tests {
    use super::{LINE_STEP, scrolled_to};

    /// QA 24 (M24-U1): nothing on the keyboard scrolled a long card. The
    /// scroll keys move it by a line, a page (less a line), or to either
    /// end, never past them; other keys are not taken.
    #[test]
    fn the_scroll_keys_move_the_card_within_its_ends() {
        let (room, height) = (200, 1000);
        let most = height - room;
        let at = |key, top| scrolled_to(key, top, room, height);
        assert_eq!(at("ArrowDown", 0), Some(LINE_STEP));
        assert_eq!(at("ArrowUp", 0), Some(0));
        assert_eq!(at("PageDown", 0), Some(room - LINE_STEP));
        assert_eq!(at("PageUp", 500), Some(500 - room + LINE_STEP));
        assert_eq!(at("End", 0), Some(most));
        assert_eq!(at("ArrowDown", most), Some(most));
        assert_eq!(at("Home", 500), Some(0));
        assert_eq!(at("Tab", 0), None);
        assert_eq!(at("Escape", 0), None);
        // A card that fits stays put.
        assert_eq!(scrolled_to("End", 0, 200, 150), Some(0));
    }
}
