//! Opening and closing the hover card and the problem tip, and what their
//! timers do when they fire.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::super::components::HoverCard;
use super::super::problem_hit::{column_at_cell, covering};
use super::super::state::EditorState;
use super::machine::{Fire, Kind, Opener, Settled, Then};
use super::{PendingCard, Popup};

impl EditorState {
    /// Opens `card`, shown at the caret (`Ctrl+K Ctrl+I`, a tap): the
    /// pointer leaving keeps it. It lists the problems at its `problems`
    /// place, or those of F8's on its line.
    pub(crate) fn show_card_at_caret(&self, card: HoverCard) {
        self.hide_hover();
        self.open_card(card, Opener::Caret);
    }

    /// The pointer's pause over `pending` is over: its card shows its
    /// field's docs. A field the schema says nothing of stays pending: the
    /// pointer moving on it waits for nothing more.
    fn open_pending_card(&self, pending: PendingCard) {
        let PendingCard {
            line,
            field,
            anchor,
        } = pending;
        let Some(info) = self
            .doc()
            .text
            .with_untracked(|text| self.field_docs(text, &field))
        else {
            self.step(|machine| {
                machine.pend_quietly(PendingCard {
                    line,
                    field,
                    anchor,
                });
            });
            return;
        };
        let card = HoverCard {
            info,
            line,
            problems: None,
            anchor,
        };
        self.open_card(card, Opener::Pointer);
        self.popups().note_card_field(line, field);
    }

    fn open_card(&self, mut card: HoverCard, by: Opener) {
        card.problems = self.card_problems_for(card.line, card.problems);
        self.popups().card_height.set(0.0);
        self.popups().replace(Some(Popup::Card {
            card: Box::new(card),
            by,
        }));
        self.step(|machine| machine.opened(Kind::Card, by));
    }

    /// Hides the card after a moment, unless the pointer gets there (to
    /// select its text, follow a link, scroll it); a card waiting out its
    /// pause goes now. One shown at the caret stays.
    pub(crate) fn leave_hover(&self) {
        if self.popups().with_card(|card| card.is_some()) {
            self.step(|machine| machine.leave(Kind::Card));
        } else {
            self.step(|machine| machine.cancel_pending());
        }
    }

    /// The pointer is back on the card's key: a hiding it was waiting for
    /// is called off (the card stays, as in VS Code).
    pub(crate) fn keep_hover(&self) {
        self.step(|machine| machine.hold());
    }

    /// Hides the card now, and one waiting out its pause.
    pub(crate) fn hide_hover(&self) {
        self.step(|machine| machine.cancel_pending());
        if self.popups().with_card(|card| card.is_none()) {
            return;
        }
        // The card or a link in it had the focus (Tab, in `Ctrl+M` mode; a
        // second `Ctrl+K Ctrl+I`): the text takes it back instead of the
        // page.
        let focused = self.card_has_focus();
        self.note_card_focus(false);
        self.popups().replace(None);
        if focused && let Some(element) = self.textarea() {
            let _ = element.focus_in_place();
        }
    }

    /// Records whether the focus is in the card.
    pub(crate) fn note_card_focus(&self, focused: bool) {
        if self.popups().focused.get_untracked() != focused {
            self.popups().focused.set(focused);
        }
    }

    /// Whether the focus is in the hover card (on it or one of its links).
    pub(crate) fn card_has_focus(&self) -> bool {
        let active = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element());
        self.popups()
            .card_ref
            .get_untracked()
            .zip(active)
            .is_some_and(|(card, active)| card.contains(Some(&active)))
    }

    /// Whether the focus moving to `to` keeps the card: into it (Tab from
    /// the text to its links, in `Ctrl+M` mode, as to the problem tip's),
    /// or from it back to the text (Shift+Tab). The text's blur and the
    /// card's focus-out hide it otherwise.
    pub(crate) fn keep_card_for(&self, to: Option<&web_sys::EventTarget>) -> bool {
        let Some(to) = to.and_then(|to| to.dyn_ref::<web_sys::Node>()) else {
            return false;
        };
        let into_card = self
            .popups()
            .card_ref
            .get_untracked()
            .is_some_and(|card| card.contains(Some(to)));
        let into_text = self
            .view()
            .textarea_ref
            .get_untracked()
            .is_some_and(|text| text.is_same_node(Some(to)));
        into_card || into_text
    }

    /// Shows the problems covering one-based `place` (under the pointer, or
    /// a focused underline's start): in the card when it documents their
    /// line, else in the problem tip, in place of any other popup.
    pub(crate) fn hover_problem(&self, place: (usize, usize)) {
        let popups = self.popups();
        enum Shown {
            Same,
            Card,
            Other,
        }
        let shown = popups.open.with_untracked(|open| match open {
            Some(Popup::Tip { place: shown }) if *shown == place => Shown::Same,
            Some(Popup::Card { card, .. }) if card.line + 1 == place.0 => Shown::Card,
            _ => Shown::Other,
        });
        match shown {
            Shown::Same => {
                self.step(|machine| machine.hold());
            }
            Shown::Card => popups.open.update(|open| {
                if let Some(Popup::Card { card, .. }) = open
                    && card.problems != Some(place)
                {
                    card.problems = Some(place);
                }
            }),
            Shown::Other => {
                if self.popups().with_card(|card| card.is_some()) {
                    self.note_card_focus(false);
                }
                popups.replace(Some(Popup::Tip { place }));
                self.step(|machine| machine.opened(Kind::Tip, Opener::Pointer));
            }
        }
    }

    /// The pointer is over rendered cell `cell` of zero-based `line`: the
    /// tip shows the problems covering the character there, or closes (a
    /// moment later) where none does.
    pub(crate) fn point_at_problems(&self, line: usize, cell: usize) {
        match self.problem_place_at(line, cell) {
            Some(place) => self.hover_problem(place),
            None => self.leave_problem(),
        }
    }

    /// The place (one-based line, column) of rendered cell `cell` of
    /// zero-based `line` when problems cover the character there.
    pub(crate) fn problem_place_at(&self, line: usize, cell: usize) -> Option<(usize, usize)> {
        self.doc().text.with_untracked(|text| {
            let line_text = text.split('\n').nth(line).unwrap_or("");
            let place = (line + 1, column_at_cell(line_text, cell));
            self.problems().diagnostics.with_untracked(|problems| {
                covering(problems, place.0, place.1, line_text)
                    .next()
                    .is_some()
                    .then_some(place)
            })
        })
    }

    /// The pointer left an underline: its tip closes a moment later, unless
    /// the pointer reached the tip (to scroll it or click a docs link).
    pub(crate) fn leave_problem(&self) {
        if self.popups().hovered_problem().is_some() {
            self.step(|machine| machine.leave(Kind::Tip));
        }
    }

    /// Hides the tip of the underline under the pointer.
    pub(crate) fn hide_problem_tip(&self) {
        if self.popups().hovered_problem().is_some() {
            self.popups().replace(None);
        }
    }

    /// Hides the message F8 showed, and the one under the pointer.
    pub(crate) fn hide_shown_problem(&self) {
        if self.problems().current.get_untracked().is_some() {
            // The tip had the focus: the text takes it back, not the page.
            let focused = self.popups().focused.get_untracked() && !self.popups().is_open();
            self.problems().current.set(None);
            if !self.popups().is_open() {
                // The pointer had the caret's tip to reach.
                self.step(|machine| machine.closed());
                self.popups().focused.set(false);
            }
            if focused && let Some(element) = self.textarea() {
                let _ = element.focus_in_place();
            }
        }
        self.hide_problem_tip();
    }

    /// The focus moved from inside the editor (the text, the tip, a
    /// control) `to` another element (or out of the window, `None`): out
    /// of the editor, the tip closes, as the hover card does.
    pub(crate) fn tip_follows_focus(&self, to: Option<&web_sys::EventTarget>) {
        let inside = self.view().root_ref.get_untracked().is_some_and(|root| {
            to.and_then(|to| to.dyn_ref::<web_sys::Node>())
                .is_some_and(|to| root.contains(Some(to)))
        });
        if !inside {
            if !self.popups().with_card(|card| card.is_some()) {
                self.popups().focused.set(false);
            }
            self.hide_shown_problem();
        }
    }

    /// A timer of the pointer's phase fired.
    pub(super) fn popup_timer_fired(self) {
        let Some(fire) = self.step(|machine| machine.fired()).flatten() else {
            return;
        };
        match fire {
            Fire::Open(pending) => self.open_pending_card(pending),
            Fire::Close {
                kind: Kind::Card,
                then,
            } => {
                let point_again = then == Then::CloseAndPointAgain;
                if point_again && self.card_has_focus() {
                    self.step(|machine| machine.kept());
                    return;
                }
                self.hide_hover();
                if point_again {
                    self.point_again();
                }
            }
            Fire::Close {
                kind: Kind::Tip, ..
            } => self.hide_problem_tip(),
            Fire::Settle { line, cell } => {
                if self.reachable_popup().is_none() {
                    return;
                }
                let shows_own = self.shows_own_popup(line, cell);
                if self.step(|machine| machine.settle(line, shows_own)) == Some(Settled::Text) {
                    self.hide_hover();
                    self.hide_problem_tip();
                    self.point_again();
                }
            }
        }
    }
}
