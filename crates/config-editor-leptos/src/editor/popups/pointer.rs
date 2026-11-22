//! The pointer over the text and on the popups, as in VS Code: what it
//! rests on shows (a problem's tip, a key's card, once the pointer rests; a
//! protected value's lock tip), and a popup it can enter (the hover card,
//! the problem tip with a link or that scrolls) takes a pointer that comes
//! from what the popup is for (see `path`), so its text can be selected
//! and copied and its links followed. One that comes onto it another way
//! goes through to the text under it, where a click places the caret,
//! until where it goes on to says whether it heads for the popup or for
//! that text (see `under`). A wheel over the popup scrolls it while it can
//! scroll that way, else the text (the page at the text's ends), and the
//! popup closes, as when the wheel scrolls the text.
//!
//! The pointer's phase decides (see `machine`); this module tells it what
//! the pointer did and where the popup is, and carries out its answer.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, MouseEvent, WheelEvent};

use super::super::positioning::{hover_card_anchor, pointer_text_position};
use super::super::scroll_through::{scroll_past, wheel_pixels};
use super::super::source_area::SourceArea;
use super::super::state::EditorState;
use super::machine::{Over, Sight};
use super::path::popup_box;
use super::{PendingCard, PendingHover, now};
use crate::editor::completion::Part;

impl EditorState {
    /// The pointer is at `(x, y)` in the window over the text (`free`: no
    /// button held): what is there shows once the pointer rests on it.
    pub(crate) fn pointer_at(&self, element: &SourceArea, x: f64, y: f64, free: bool) {
        let (line, column) = pointer_text_position(element, x, y);
        // Heading for the popup across other text: that text waits.
        if free && self.on_the_way_to_popup(x, y, line, column) {
            return;
        }
        let tip = if free {
            self.problems_to_wait_for(line, column)
        } else {
            None
        };
        if free {
            self.step(|machine| machine.on_problems());
        }
        let here = self.card_here(element, line, column);
        let card = match &here {
            CardHere::Wait(card) => Some(card.clone()),
            _ => None,
        };
        let waits = card.is_some() || tip.is_some();
        match here {
            // Back on its key within the moment it was given to be
            // reached: it stays.
            CardHere::Shows => self.keep_hover(),
            // Something else shows next: the card goes now.
            _ if waits => {
                if self.popups().with_card(|card| card.is_some()) {
                    self.hide_hover();
                }
            }
            CardHere::Hide => self.hide_hover(),
            _ => self.leave_hover(),
        }
        if waits {
            let rest_ms = self.config().settings.hover_delay.get_untracked();
            self.step(|machine| {
                machine.pend(PendingHover { card, tip }, (x, y), now(), rest_ms);
            });
        }
    }

    /// The pointer is over character `column` of zero-based `line`: the
    /// problems there, one-based, when they wait for the pointer's rest.
    /// Shown already (the tip for them, the card of their line), that
    /// popup follows the pointer at once; with none there, the tip leaves.
    fn problems_to_wait_for(&self, line: usize, column: usize) -> Option<(usize, usize)> {
        let Some(place) = self.problem_place_at(line, column) else {
            self.leave_problem();
            return None;
        };
        if self.problems_shown_at(place) {
            self.hover_problem(place);
            return None;
        }
        Some(place)
    }

    /// What the card does for the pointer over character `column` of
    /// zero-based `line`.
    fn card_here(&self, element: &SourceArea, line: usize, column: usize) -> CardHere {
        if !self.completion().suggestions.with_untracked(Vec::is_empty) {
            return CardHere::Hide;
        }
        let on_protected = self.hover_protected(line, column).is_some();
        if !self.config().settings.hover_docs.get_untracked() {
            return CardHere::Leave;
        }
        // Only the field is found on a move; its docs are read after the
        // rest.
        let Some(field) = self.field_under_pointer(line, column) else {
            return CardHere::Leave;
        };
        // The value's own message takes the place of its key's docs there.
        if on_protected
            && (field.part == Part::Value
                || column >= field.start_column + field.key.chars().count())
        {
            return CardHere::Leave;
        }
        if self.card_shows(line, &field) {
            return CardHere::Shows;
        }
        let anchor = hover_card_anchor(element, line, field.start_column);
        CardHere::Wait(PendingCard {
            line,
            field,
            anchor,
        })
    }

    /// The pointer moved to `(x, y)` in the window, over character
    /// `column` of zero-based `line`: whether it is on its way to the popup it can
    /// enter, across other text, so what it crosses waits (and the popup
    /// with it).
    fn on_the_way_to_popup(&self, x: f64, y: f64, line: usize, column: usize) -> bool {
        let popup = self.reachable_popup();
        let sight = Sight {
            popup: popup.as_ref().and_then(popup_box),
            on_anchor: self.on_popup_anchor(line, column),
        };
        self.step(|machine| machine.over_text((x, y), line, column, sight)) == Some(Over::Waits)
    }

    /// The pointer stopped on the way and the popup went: what it rests
    /// on shows, as if it had just moved there (off the text, nothing).
    pub(crate) fn point_again(&self) {
        let again = self.step(|machine| machine.point_again()) == Some(true);
        let pointer = self.with_machine(|machine| machine.pointer());
        if let (true, Some((x, y)), Some(element)) = (again, pointer, self.textarea()) {
            self.pointer_at(&element, x, y, true);
        }
    }

    /// The pointer moved over the text.
    pub(crate) fn pointer_on_text(&self) {
        self.step(|machine| machine.on_text_again());
    }

    /// The pointer left the text for `to`, at `(x, y)` in the window: the
    /// next move over it goes on from there (the gutter, the page). Off
    /// the lines the popup covers, a way to it starts there, as over the
    /// text. Onto the popup, the way so far holds (the popup decides
    /// whether it takes the pointer). The card and the problem tip close
    /// after their moment.
    pub(crate) fn pointer_left_text(&self, to: Option<&web_sys::EventTarget>, x: f64, y: f64) {
        let popup = self.reachable_popup();
        let onto_popup = popup.as_ref().is_some_and(|popup| {
            to.and_then(|to| to.dyn_ref::<web_sys::Node>())
                .is_some_and(|to| popup.contains(Some(to)))
        });
        let popup = popup.as_ref().and_then(popup_box);
        self.step(|machine| machine.left_text((x, y), onto_popup, popup));
        self.leave_hover();
        self.leave_problem();
    }

    /// The pointer moved to `(x, y)` in the window off the text (the
    /// gutter, the page): as over the text, a way to the popup starts there
    /// off the lines it covers, and is lost beside it on them.
    pub(crate) fn pointer_off_text(&self, x: f64, y: f64) {
        let popup = self.reachable_popup().as_ref().and_then(popup_box);
        self.step(|machine| machine.off_text((x, y), popup));
    }

    /// The popup the pointer can enter: the hover card, else the problem
    /// tip with a link (or one that scrolls).
    pub(crate) fn reachable_popup(&self) -> Option<HtmlElement> {
        let popups = self.popups();
        let card = popups.with_card(|card| card.is_some());
        if card {
            return popups.card_ref.get_untracked().map(Into::into);
        }
        let tip = popups.open.with_untracked(Option::is_none) || popups.hovered_problem().is_some();
        if tip && popups.tip_reachable.get_untracked() {
            return popups.tip_ref.get_untracked().map(Into::into);
        }
        None
    }

    /// Whether character `column` of zero-based `line` is what the popup
    /// is for: the card's key, or the problems the tip the pointer opened
    /// shows.
    fn on_popup_anchor(&self, line: usize, column: usize) -> bool {
        if self.popups().with_card(|card| card.is_some()) {
            return self
                .field_under_pointer(line, column)
                .is_some_and(|field| self.card_shows(line, &field));
        }
        self.popups()
            .hovered_problem()
            .is_some_and(|shown| self.same_problems((line + 1, column + 1), shown))
    }

    /// Whether character `column` of zero-based `line` shows something of
    /// its own to the pointer: docs other than the card's, problems other
    /// than the tip's.
    pub(super) fn shows_own_popup(&self, line: usize, column: usize) -> bool {
        if self.on_popup_anchor(line, column) {
            return false;
        }
        let docs = self.config().settings.hover_docs.get_untracked()
            && self.docs_under_pointer(line, column).is_some();
        docs || self.problem_place_at(line, column).is_some()
    }

    /// The pointer came onto `popup`: whether it enters it. A popup shown
    /// at the caret takes it from anywhere: so does F8's tip, which no
    /// pointer opened.
    fn enter(&self, popup: &HtmlElement) -> bool {
        let caret_tip = self.popups().open.with_untracked(Option::is_none)
            && self.problems().current.with_untracked(Option::is_some);
        if caret_tip {
            self.step(|machine| {
                machine.closed(now());
                machine.opened(super::machine::Kind::Tip, super::machine::Opener::Caret);
            });
        }
        self.step(|machine| machine.enter_popup(popup_box(popup))) == Some(true)
    }

    /// The pointer left the popup: whether it had entered it.
    pub(crate) fn left_popup(&self) -> bool {
        self.step(|machine| machine.left_popup()) == Some(true)
    }

    /// Whether the pointer is in a popup, or over the text: Escape then
    /// closes what shows for it.
    pub(super) fn pointer_over_popups(&self) -> bool {
        self.with_machine(|machine| machine.inside() || machine.on_text())
    }

    /// The popup shows something else: it takes the pointer again.
    pub(crate) fn popup_takes_pointer(&self) {
        self.step(|machine| machine.take_pointer_again());
    }
}

/// What the card does for the pointer where it is.
enum CardHere {
    /// It shows this key already: it stays.
    Shows,
    /// Its key's card waits for the pointer's rest.
    Wait(PendingCard),
    /// Nothing here has a card: the card leaves after its moment.
    Leave,
    /// It hides (the suggestion list is open).
    Hide,
}

/// The pointer came onto `popup`: whether it enters it (on its way from
/// what the popup is for, or to a popup shown at the caret). Otherwise the
/// popup lets it through, and stays while where it goes on to decides (see
/// `under`).
pub(crate) fn enter_popup(state: EditorState, popup: &HtmlElement, event: &MouseEvent) -> bool {
    if state.enter(popup) {
        return true;
    }
    // Through the popup, the pointer is over the text.
    if let Some(element) = state.textarea() {
        let (x, y) = (f64::from(event.client_x()), f64::from(event.client_y()));
        state.pointer_at(&element, x, y, event.buttons() == 0);
    }
    false
}

/// The pointer left a popup it had entered for the text (`event`, its
/// `mouseleave`): whether it went there. Then what is under it answers at
/// once, as if it had moved there (the page sends the text no move when
/// it comes back to the pixel it last had there): back on what the popup
/// is for, the popup stays, without closing to open again.
pub(crate) fn back_on_text(state: EditorState, event: &MouseEvent) -> bool {
    let Some(element) = state.textarea() else {
        return false;
    };
    let onto_text = event.related_target().is_some_and(|to| {
        let text: &web_sys::EventTarget = element.element().unchecked_ref();
        &to == text
    });
    if onto_text {
        let (x, y) = (f64::from(event.client_x()), f64::from(event.client_y()));
        state.pointer_at(&element, x, y, event.buttons() == 0);
    }
    onto_text
}

/// A wheel over `popup`: it scrolls the popup while it can scroll that
/// way; else the text, or the page at the text's ends, and the popups
/// close.
pub(crate) fn popup_wheel(state: EditorState, popup: &HtmlElement, event: &WheelEvent) {
    if wheel_past(state, popup, event) {
        state.hide_hover();
        state.hide_problem_tip();
    }
}

/// A wheel over `popup` that stays open (the suggestion list): the same,
/// without closing it.
pub(crate) fn list_wheel(state: EditorState, popup: &HtmlElement, event: &WheelEvent) {
    wheel_past(state, popup, event);
}

/// A wheel over `popup`: whether it went on to the text (the popup cannot
/// scroll that way).
fn wheel_past(state: EditorState, popup: &HtmlElement, event: &WheelEvent) -> bool {
    let (dx, dy) = wheel_pixels(state, event);
    if scrolls_itself(popup, dy) {
        return false;
    }
    event.prevent_default();
    scroll_past(state, dx, dy);
    true
}

/// Whether `popup` scrolls by `dy` pixels (down when positive) itself.
fn scrolls_itself(popup: &HtmlElement, dy: f64) -> bool {
    let top = popup.scroll_top();
    let most = popup.scroll_height() - popup.client_height();
    (dy > 0.0 && top < most) || (dy < 0.0 && top > 0)
}
