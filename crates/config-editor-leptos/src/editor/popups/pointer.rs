//! The pointer over the text and on the popups, as in VS Code: what it
//! rests on shows (a problem's tip at once, a key's card after a pause, a
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

use super::super::completion::visual_column;
use super::super::paths::split_line;
use super::super::positioning::{hover_card_anchor, pointer_text_position};
use super::super::problem_hit::{column_at_cell, covering};
use super::super::scroll_through::{scroll_past, wheel_pixels};
use super::super::source_area::SourceArea;
use super::super::state::EditorState;
use super::machine::{Over, Sight};
use super::path::popup_box;
use crate::editor::components::HoverCard;

impl EditorState {
    /// The pointer is at `(x, y)` in the window over the text (`free`: no
    /// button held): what is there shows.
    pub(crate) fn pointer_at(&self, element: &SourceArea, x: f64, y: f64, free: bool) {
        let (line, cell) = pointer_text_position(element, x, y);
        // Heading for the popup across other text: that text waits.
        if free && self.on_the_way_to_popup(x, y, line, cell) {
            return;
        }
        if free {
            self.point_at_problems(line, cell);
            self.step(|machine| machine.on_problems());
        }
        if !self.completion().suggestions.with_untracked(Vec::is_empty) {
            self.hide_hover();
            return;
        }
        // The text reads characters, the screen cells (a tab is wider).
        let column = self.column_at(line, cell);
        let on_protected = self.hover_protected(line, column).is_some();
        if !self.config().settings.hover_docs.get_untracked() {
            return;
        }
        let Some(info) = self.docs_under_pointer(line, column) else {
            self.leave_hover();
            return;
        };
        // The value's own message takes the place of its key's docs there.
        if on_protected && column >= info.start_column + info.key.chars().count() {
            self.leave_hover();
            return;
        }
        let waiting = self.with_machine(|machine| {
            machine.pending().is_some_and(|card| {
                card.line == line
                    && card.info.start_column == info.start_column
                    && card.info.key == info.key
            })
        });
        if waiting {
            return;
        }
        if self.card_shows(line, &info) {
            // Back on its key within the moment it was given to be
            // reached: it stays.
            self.keep_hover();
            return;
        }
        self.hide_hover();
        let anchor = hover_card_anchor(element, line, self.cell_at(line, info.start_column));
        let card = HoverCard {
            info,
            line,
            problems: None,
            anchor,
        };
        self.step(|machine| machine.pend(card));
    }

    /// The character column of rendered cell `cell` of zero-based `line`.
    fn column_at(&self, line: usize, cell: usize) -> usize {
        self.doc()
            .text
            .with_untracked(|text| column_at_cell(split_line(text, line).unwrap_or(""), cell) - 1)
    }

    /// The rendered cell of character column `column` of zero-based `line`.
    pub(crate) fn cell_at(&self, line: usize, column: usize) -> usize {
        self.doc().text.with_untracked(|text| {
            let line_text = split_line(text, line).unwrap_or("");
            let end = line_text
                .char_indices()
                .nth(column)
                .map_or(line_text.len(), |(at, _)| at);
            visual_column(&line_text[..end]) as usize
        })
    }

    /// The pointer moved to `(x, y)` in the window, over cell `cell` of
    /// zero-based `line`: whether it is on its way to the popup it can
    /// enter, across other text, so what it crosses waits (and the popup
    /// with it).
    fn on_the_way_to_popup(&self, x: f64, y: f64, line: usize, cell: usize) -> bool {
        let popup = self.reachable_popup();
        let sight = Sight {
            popup: popup.as_ref().and_then(popup_box),
            on_anchor: self.on_popup_anchor(line, cell),
        };
        self.step(|machine| machine.over_text((x, y), line, cell, sight)) == Some(Over::Waits)
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

    /// Whether cell `cell` of zero-based `line` is what the popup is for:
    /// the card's key, or the problems the tip the pointer opened shows.
    fn on_popup_anchor(&self, line: usize, cell: usize) -> bool {
        if self.popups().with_card(|card| card.is_some()) {
            return self
                .docs_under_pointer(line, self.column_at(line, cell))
                .is_some_and(|info| self.card_shows(line, &info));
        }
        let Some((hovered_line, hovered_column)) = self.popups().hovered_problem() else {
            return false;
        };
        self.doc().text.with_untracked(|text| {
            let here = split_line(text, line).unwrap_or("");
            let there = split_line(text, hovered_line - 1).unwrap_or("");
            let column = column_at_cell(here, cell);
            self.problems().diagnostics.with_untracked(|problems| {
                let now: Vec<_> = covering(problems, line + 1, column, here).collect();
                let shown: Vec<_> =
                    covering(problems, hovered_line, hovered_column, there).collect();
                !now.is_empty() && now == shown
            })
        })
    }

    /// Whether cell `cell` of zero-based `line` shows something of its own
    /// to the pointer: docs other than the card's, problems other than the
    /// tip's.
    pub(super) fn shows_own_popup(&self, line: usize, cell: usize) -> bool {
        if self.on_popup_anchor(line, cell) {
            return false;
        }
        let docs = self.config().settings.hover_docs.get_untracked()
            && self
                .docs_under_pointer(line, self.column_at(line, cell))
                .is_some();
        docs || self.problem_place_at(line, cell).is_some()
    }

    /// The pointer came onto `popup`: whether it enters it. A popup shown
    /// at the caret takes it from anywhere: so does F8's tip, which no
    /// pointer opened.
    fn enter(&self, popup: &HtmlElement) -> bool {
        let caret_tip = self.popups().open.with_untracked(Option::is_none)
            && self.problems().current.with_untracked(Option::is_some);
        if caret_tip {
            self.step(|machine| {
                machine.closed();
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
