//! The way to a popup the pointer can enter (the hover card, the problem
//! tip with a link): while the pointer heads for it across other
//! text, that text's docs and problems wait, so the popup stays to be
//! reached instead of giving way to the tip or the card of what the
//! pointer crosses. Heading for it means each move stays inside the
//! shape spanned by the point before it and the popup (the "safe
//! triangle" of menus), on a way that starts at what the popup is for (its
//! anchor) or in the gap between the popup and its line; a pointer that
//! stops on the way still lets the popup go after its usual moment.
//!
//! Only a pointer on that way enters the popup: one that comes onto it
//! another way (a card below its line covers the next lines) goes through
//! to the text under it (see `popup_pointer`).

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::positioning::{POPUP_GAP, schedule_hover};
use super::problem_hit::{column_at_cell, covering};
use super::state::EditorState;

/// How far around the popup a move still heads for it, in pixels: room for
/// rounding, not so much that it reaches over the popup's own line.
const SLACK: f64 = 2.0;

/// A box in the window's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

pub(super) type Point = (f64, f64);

/// Whether the pointer moving from `from` to `to` heads for `target`: `to`
/// is inside `target` or the triangle from `from` to one of its sides
/// (together, the hull of `from` and `target`), grown by `SLACK`.
fn heads_for(from: Point, to: Point, target: Rect) -> bool {
    let grown = Rect {
        left: target.left - SLACK,
        top: target.top - SLACK,
        right: target.right + SLACK,
        bottom: target.bottom + SLACK,
    };
    let inside =
        |(x, y): Point| x >= grown.left && x <= grown.right && y >= grown.top && y <= grown.bottom;
    if inside(to) {
        return true;
    }
    if from == to || inside(from) {
        return false;
    }
    let corners = [
        (grown.left, grown.top),
        (grown.right, grown.top),
        (grown.right, grown.bottom),
        (grown.left, grown.bottom),
    ];
    (0..4).any(|side| in_triangle(to, from, corners[side], corners[(side + 1) % 4]))
}

/// Whether the pointer going from `from` to `to` (on `target`) crossed the
/// side of `target` that faces `from` above or below it: the side next to
/// the popup's line. From beside it (level with it) it came through
/// another side.
fn crosses_facing_side(from: Point, to: Point, target: Rect) -> bool {
    let side = if from.1 <= target.top {
        target.top
    } else if from.1 >= target.bottom {
        target.bottom
    } else {
        return false;
    };
    if (to.1 - from.1).abs() < f64::EPSILON {
        return (target.left..=target.right).contains(&from.0);
    }
    let at = (side - from.1) / (to.1 - from.1);
    let x = from.0 + (to.0 - from.0) * at.clamp(0.0, 1.0);
    x >= target.left - SLACK && x <= target.right + SLACK
}

/// Whether `point` is on `target` itself.
fn inside((x, y): Point, target: Rect) -> bool {
    x >= target.left && x <= target.right && y >= target.top && y <= target.bottom
}

/// Whether `point` is in the gap the popup `target` keeps from its line,
/// above or below it (not on it, nor beside it: on the lines it covers or
/// passes).
fn in_gap((x, y): Point, target: Rect) -> bool {
    let reach = POPUP_GAP + SLACK;
    let above = y >= target.top - reach && y < target.top;
    let below = y > target.bottom && y <= target.bottom + reach;
    x >= target.left - SLACK && x <= target.right + SLACK && (above || below)
}

/// Whether `point` is inside the triangle `a`, `b`, `c` (or on its edge).
fn in_triangle(point: Point, a: Point, b: Point, c: Point) -> bool {
    let cross =
        |p: Point, q: Point, r: Point| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
    let sides = [cross(a, b, point), cross(b, c, point), cross(c, a, point)];
    sides.iter().all(|side| *side >= -0.5) || sides.iter().all(|side| *side <= 0.5)
}

impl EditorState {
    /// The pointer moved to `(x, y)` in the window, over cell `cell` of
    /// zero-based `line`: whether it is on its way to the popup it can
    /// enter, across other text, so what it crosses waits (and the popup
    /// with it).
    pub(super) fn on_the_way_to_popup(&self, x: f64, y: f64, line: usize, cell: usize) -> bool {
        let from = self.hover().pointer.get_value();
        self.hover().pointer.set_value(Some((x, y)));
        let popup = self.reachable_popup();
        let on_way = self.hover().on_way.get_value();
        // Over the popup, which let the pointer through (it came another
        // way): the text under it answers.
        if popup.is_some_and(|popup| inside((x, y), popup)) {
            self.hover().on_way.set_value(false);
            return false;
        }
        let on_anchor = self.on_popup_anchor(line, cell);
        // In the gap between the line and the popup: as good as on it, so
        // resting there keeps it.
        if popup.is_some_and(|popup| in_gap((x, y), popup)) && !on_anchor {
            self.hover().on_way.set_value(true);
            self.popup_takes_pointer();
            self.hold_popup();
            return true;
        }
        if on_anchor {
            self.hover().on_way.set_value(true);
            self.popup_takes_pointer();
            return false;
        }
        let heading = on_way
            && match from {
                Some(from) if from == (x, y) => true,
                Some(from) => popup.is_some_and(|popup| heads_for(from, (x, y), popup)),
                None => false,
            };
        self.hover().on_way.set_value(heading);
        if !heading {
            return false;
        }
        self.wait_on_the_way();
        true
    }

    /// Whether the pointer came onto the popup at `entry` (in the window)
    /// on its way there: its last move over the text was on the popup's
    /// anchor, in its gap, or heading for it from there, and from that
    /// point it crossed the popup's side that faces its line (not another
    /// side, from the lines beside or past it: moves come in batches when
    /// the page is busy). Only then does the popup take it.
    pub(super) fn entered_on_the_way(&self, entry: Point) -> bool {
        let (Some(from), Some(popup)) = (self.hover().pointer.get_value(), self.reachable_popup())
        else {
            return false;
        };
        self.hover().on_way.get_value() && crosses_facing_side(from, entry, popup)
    }

    /// The pointer stopped on the way and the popup went: what it rests
    /// on shows, as if it had just moved there.
    fn point_again(&self) {
        let (Some((x, y)), Some(element)) = (self.hover().pointer.get_value(), self.textarea())
        else {
            return;
        };
        self.hover().on_way.set_value(false);
        super::input::pointer_at(*self, &element, x, y, true);
    }

    /// The pointer left the text for `to`: the next move over it starts a
    /// new way. Onto the popup, the way so far holds (the popup decides
    /// whether it takes the pointer).
    pub(super) fn forget_pointer(&self, to: Option<&web_sys::EventTarget>) {
        let onto_popup = self.reachable_popup_element().is_some_and(|popup| {
            to.and_then(|to| to.dyn_ref::<web_sys::Node>())
                .is_some_and(|to| popup.contains(Some(to)))
        });
        if !onto_popup {
            self.hover().pointer.set_value(None);
            self.hover().on_way.set_value(false);
        }
    }

    /// The popup the pointer can enter: the hover card, else the problem
    /// tip with a link (or one that scrolls).
    pub(super) fn reachable_popup_element(&self) -> Option<web_sys::HtmlElement> {
        let shown = self.hover().card.with_untracked(Option::is_some)
            || self.problems().hovered.with_untracked(Option::is_some);
        if !shown {
            return None;
        }
        let root = self.view().root_ref.get_untracked()?;
        let popup = root
            .query_selector(".config-editor__hover")
            .ok()
            .flatten()
            .or_else(|| {
                root.query_selector(".config-editor__problem-tip--interactive:not([hidden])")
                    .ok()
                    .flatten()
            })?;
        popup.dyn_into().ok()
    }

    /// The box of the popup the pointer can enter.
    fn reachable_popup(&self) -> Option<Rect> {
        let popup = self.reachable_popup_element()?;
        let rect = popup.get_bounding_client_rect();
        (rect.width() > 0.0).then(|| Rect {
            left: rect.left(),
            top: rect.top(),
            right: rect.right(),
            bottom: rect.bottom(),
        })
    }

    /// Whether cell `cell` of zero-based `line` is what the popup is for:
    /// the card's key, or the problems the tip shows.
    fn on_popup_anchor(&self, line: usize, cell: usize) -> bool {
        let card = self.hover().card.with_untracked(|card| {
            card.as_ref()
                .map(|card| (card.line, card.info.start_column, card.info.key.clone()))
        });
        if let Some(card) = card {
            return self
                .docs_under_pointer(line, cell)
                .is_some_and(|info| (line, info.start_column, info.key) == card);
        }
        self.doc().text.with_untracked(|text| {
            let Some((hovered_line, hovered_column)) = self.problems().hovered.get_untracked()
            else {
                return false;
            };
            let lines: Vec<&str> = text.split('\n').collect();
            let here = lines.get(line).copied().unwrap_or("");
            let there = lines.get(hovered_line - 1).copied().unwrap_or("");
            let column = column_at_cell(here, cell);
            self.problems().diagnostics.with_untracked(|problems| {
                let now: Vec<_> = covering(problems, line + 1, column, here).collect();
                let shown: Vec<_> =
                    covering(problems, hovered_line, hovered_column, there).collect();
                !now.is_empty() && now == shown
            })
        })
    }

    /// In the popup's gap: it (and the problems the card lists) stays, the
    /// moment it was given to be reached called off.
    fn hold_popup(&self) {
        let problems = self.problems();
        problems.tip_token.update_value(|token| *token += 1);
        problems.tip_leaving.set_value(false);
        if self.hover().card.with_untracked(Option::is_some) && !self.hover().at_caret.get_value() {
            self.hover().token.update_value(|token| *token += 1);
        }
    }

    /// On the way to the popup: it (and the problems the card lists) stays
    /// a moment more, then goes unless the pointer got there.
    fn wait_on_the_way(&self) {
        let problems = self.problems();
        let card = self.hover().card.with_untracked(Option::is_some);
        if card && !self.hover().at_caret.get_value() {
            // The card and the problems it lists wait together.
            problems.tip_token.update_value(|token| *token += 1);
            problems.tip_leaving.set_value(false);
            self.hover().token.update_value(|token| *token += 1);
            let token = self.hover().token.get_value();
            schedule_hover(*self, move |state| {
                if state.hover().token.get_value() != token
                    || state.hover().card_entered.get_value()
                    || state.card_has_focus()
                {
                    return;
                }
                state.hide_hover();
                if !state.problems().tip_entered.get_value() {
                    state.hide_problem_tip();
                }
                state.point_again();
            });
        } else if problems.hovered.with_untracked(Option::is_some) {
            problems.tip_leaving.set_value(false);
            self.leave_problem();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Rect, crosses_facing_side, heads_for, in_gap};

    /// A card below a key's line, as the hover card opens.
    const CARD: Rect = Rect {
        left: 40.0,
        top: 60.0,
        right: 380.0,
        bottom: 200.0,
    };

    /// QA 23 (M23-U1): the pointer heading for the card across the next
    /// line's key swapped the card for that key's (or the problem tip), so
    /// the card's docs could not be reached. Every small step towards a
    /// point inside it heads for it; a step away, or along the line, does
    /// not.
    #[test]
    fn small_steps_towards_the_card_head_for_it() {
        let key = (60.0, 45.0);
        for target in [(80.0, 190.0), (370.0, 70.0), (45.0, 150.0), (200.0, 100.0)] {
            let mut from = key;
            for step in 1..=20 {
                let at = step as f64 / 20.0;
                let to = (
                    key.0 + (target.0 - key.0) * at,
                    key.1 + (target.1 - key.1) * at,
                );
                assert!(
                    heads_for(from, to, CARD),
                    "{target:?} step {step}: {from:?} -> {to:?}"
                );
                from = to;
            }
        }
        // Away, back up, or along the line past the card's side.
        assert!(!heads_for(key, (60.0, 30.0), CARD));
        assert!(!heads_for(key, (20.0, 45.0), CARD));
        assert!(!heads_for((390.0, 45.0), (420.0, 45.0), CARD));
        // Not moving decides nothing.
        assert!(!heads_for(key, key, CARD));
    }

    /// QA 26 (M26-U1): a card below its line took the pointer that came
    /// onto the lines it covers from beside it, so those lines could not
    /// be hovered, clicked or scrolled. Only a way across its side facing
    /// the line counts (moves may come in batches, so a long step too);
    /// from beside it, it does not.
    #[test]
    fn only_the_side_facing_the_line_lets_the_pointer_in() {
        // Down from the key, straight or slanted, in one step or several.
        for (from, to) in [
            ((60.0, 45.0), (60.0, 70.0)),
            ((60.0, 45.0), (370.0, 190.0)),
            ((60.0, 58.0), (45.0, 61.0)),
            ((200.0, 59.0), (200.0, 60.5)),
        ] {
            assert!(crosses_facing_side(from, to, CARD), "{from:?} -> {to:?}");
        }
        // From beside the card, or from its line past its side.
        for (from, to) in [
            ((420.0, 100.0), (370.0, 100.0)),
            ((600.0, 45.0), (370.0, 70.0)),
            ((20.0, 150.0), (45.0, 150.0)),
            ((-200.0, 40.0), (45.0, 120.0)),
        ] {
            assert!(!crosses_facing_side(from, to, CARD), "{from:?} -> {to:?}");
        }
        // Below the card (from the lines past it), up across its far side
        // only when it is above its line.
        let above = Rect {
            top: 0.0,
            bottom: 40.0,
            ..CARD
        };
        assert!(crosses_facing_side((60.0, 50.0), (60.0, 30.0), above));
    }

    /// QA 24 (L24-U1): the pointer resting in the 4 px gap between the
    /// line and the card let it close. The gap above and below it counts
    /// as on the way; the line itself, and beside the card, do not. QA 26
    /// (M26-U1): nor does the card itself, or the lines just beside it
    /// (which it lets the pointer through to when it came another way).
    #[test]
    fn the_gap_between_the_line_and_the_card_is_on_the_way() {
        for y in [56.5, 58.0, 59.5, 203.0] {
            assert!(in_gap((100.0, y), CARD), "{y}");
        }
        for (x, y) in [
            (100.0, 50.0),
            (100.0, 53.0),
            (20.0, 58.0),
            (400.0, 58.0),
            (100.0, 60.0),
            (100.0, 120.0),
            (39.0, 120.0),
            (381.0, 150.0),
        ] {
            assert!(!in_gap((x, y), CARD), "{x}, {y}");
        }
    }
}
