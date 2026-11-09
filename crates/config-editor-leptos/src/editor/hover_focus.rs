//! The keyboard in the hover card, as in VS Code: a second `Ctrl+K Ctrl+I`
//! while the card shown at the caret is open moves the focus into it,
//! where the arrow keys, Page Up, Page Down, Home and End scroll it, and
//! Escape closes it and gives the text the focus back (see
//! `keys::on_root_keydown`). Tab still goes on to its links.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
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
        // A dialog by the time the focus lands, so that is what is said.
        let _ = card.set_attribute("role", "dialog");
        self.note_card_focus(true);
        if crate::focus::in_place(&card).is_err() {
            self.note_card_focus(false);
            return false;
        }
        let said = self.strings_untracked(|strings| strings.hover_focused.clone());
        self.a11y().announce(said);
        true
    }

    /// Moves the focus into the problem tip F8 or `Ctrl+K Ctrl+I` showed
    /// at the caret (one without docs, so no card), as into the card.
    /// Returns whether it did.
    pub(super) fn focus_problem_tip(&self) -> bool {
        let problems = self.problems();
        if problems.current.with_untracked(Option::is_none) {
            return false;
        }
        let Some(tip) = problems.tip_ref.get_untracked() else {
            return false;
        };
        if tip.hidden() {
            return false;
        }
        let _ = tip.set_attribute("role", "dialog");
        problems.tip_focused.set(true);
        if crate::focus::in_place(&tip).is_err() {
            problems.tip_focused.set(false);
            return false;
        }
        let said = self.strings_untracked(|strings| strings.hover_focused.clone());
        self.a11y().announce(said);
        true
    }
}

/// `Enter` on the focused `popup` (the card, the problem tip) itself (a
/// link in it opens itself), or `Alt+Enter` anywhere in it: opens the
/// docs of the link focused, else of the problem in view (see
/// `docs_in_reach`); without a docs link in it, those `Alt+Enter` opens
/// from the text. Returns whether it did.
pub(super) fn opens_docs_from(
    state: EditorState,
    popup: &HtmlElement,
    event: &KeyboardEvent,
) -> bool {
    if event.key() != "Enter" || event.ctrl_key() || event.meta_key() || event.shift_key() {
        return false;
    }
    let target = event.target();
    let on_popup = target.as_ref() == Some(popup.as_ref());
    if !(event.alt_key() || on_popup) {
        return false;
    }
    let Some(url) = docs_in_reach(popup, target.as_ref()) else {
        return super::a11y::open_docs(state, event);
    };
    event.prevent_default();
    if let Some(window) = web_sys::window() {
        let _ = window.open_with_url_and_target_and_features(&url, "_blank", "noopener");
    }
    true
}

/// The docs links of a popup: each problem's, and the card's own.
const DOCS_LINKS: &str = ".config-editor__problem-tip-link, .config-editor__hover-link";

/// The docs a key on `popup` opens: those of the docs link focused
/// (`target`), else of the first problem (or the card's docs) in view in
/// its scroll; `None` when it has no docs link.
fn docs_in_reach(popup: &HtmlElement, target: Option<&web_sys::EventTarget>) -> Option<String> {
    let href = |link: &web_sys::Element| link.get_attribute("href");
    if let Some(link) = target.and_then(|target| target.dyn_ref::<web_sys::Element>())
        && link.matches(DOCS_LINKS).unwrap_or(false)
    {
        return href(link);
    }
    let links = popup.query_selector_all(DOCS_LINKS).ok()?;
    let frame = popup.get_bounding_client_rect();
    let top = frame.top() + f64::from(popup.client_top());
    let bottom = top + f64::from(popup.client_height());
    let links: Vec<web_sys::Element> = (0..links.length())
        .filter_map(|index| links.item(index)?.dyn_into().ok())
        .collect();
    let in_view = links.iter().find(|link| {
        // A problem is in view when any of its lines is.
        let shown = link
            .closest(".config-editor__problem-tip-entry")
            .ok()
            .flatten()
            .unwrap_or_else(|| (*link).clone())
            .get_bounding_client_rect();
        shown.bottom() > top && shown.top() < bottom
    });
    in_view.or(links.first()).and_then(href)
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
