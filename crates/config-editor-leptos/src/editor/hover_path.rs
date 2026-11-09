//! The way to a popup the pointer can enter (the hover card, the problem
//! tip, each with a link): while the pointer heads for it across other
//! text, that text's docs and problems wait, so the popup stays to be
//! reached instead of giving way to the tip or the card of what the
//! pointer crosses. Heading for it means each move stays inside the
//! shape spanned by the point before it and the popup (the "safe
//! triangle" of menus); a pointer that stops on the way still lets the
//! popup go after its usual moment.

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

type Point = (f64, f64);

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

/// Whether `point` is in the gap the popup `target` keeps from its line,
/// above or below it (or on the popup).
fn in_gap((x, y): Point, target: Rect) -> bool {
    let reach = POPUP_GAP + SLACK;
    x >= target.left - SLACK
        && x <= target.right + SLACK
        && y >= target.top - reach
        && y <= target.bottom + reach
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
        // In the gap between the line and the popup: as good as on it, so
        // resting there keeps it.
        if popup.is_some_and(|popup| in_gap((x, y), popup)) && !self.on_popup_anchor(line, cell) {
            self.hover().heading.set_value(true);
            self.hold_popup();
            return true;
        }
        let heading = match from {
            Some(from) if from == (x, y) => self.hover().heading.get_value(),
            Some(from) => popup.is_some_and(|popup| heads_for(from, (x, y), popup)),
            None => false,
        };
        self.hover().heading.set_value(heading);
        if !heading || self.on_popup_anchor(line, cell) {
            return false;
        }
        self.wait_on_the_way();
        true
    }

    /// The pointer left the text (for a popup or away): the next move
    /// starts a new way.
    pub(super) fn forget_pointer(&self) {
        self.hover().pointer.set_value(None);
        self.hover().heading.set_value(false);
    }

    /// The box of the popup the pointer can enter: the hover card with a
    /// link, else the problem tip with one (or one that scrolls).
    fn reachable_popup(&self) -> Option<Rect> {
        let shown = self.hover().card.with_untracked(Option::is_some)
            || self.problems().hovered.with_untracked(Option::is_some);
        if !shown {
            return None;
        }
        let root = self.view().root_ref.get_untracked()?;
        let popup = root
            .query_selector(".config-editor__hover--interactive")
            .ok()
            .flatten()
            .or_else(|| {
                root.query_selector(".config-editor__problem-tip--interactive:not([hidden])")
                    .ok()
                    .flatten()
            })?;
        let popup: web_sys::HtmlElement = popup.dyn_into().ok()?;
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
            });
        } else if problems.hovered.with_untracked(Option::is_some) {
            problems.tip_leaving.set_value(false);
            self.leave_problem();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Rect, heads_for, in_gap};

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

    /// QA 24 (L24-U1): the pointer resting in the 4 px gap between the
    /// line and the card let it close. The gap above and below it counts
    /// as on it; the line itself, and beside the card, do not.
    #[test]
    fn the_gap_between_the_line_and_the_card_is_on_the_way() {
        for y in [56.5, 58.0, 59.5, 60.0, 120.0, 203.0] {
            assert!(in_gap((100.0, y), CARD), "{y}");
        }
        for (x, y) in [(100.0, 50.0), (100.0, 53.0), (20.0, 58.0), (400.0, 58.0)] {
            assert!(!in_gap((x, y), CARD), "{x}, {y}");
        }
    }
}
