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
//! The rule for which pointer the popup takes is one: the popup lets
//! the pointer through only when it comes onto it from the lines it
//! covers, beside it, having lost its way there. Anywhere off those lines
//! (the popup's own line, its gap, the lines past its top or bottom, the
//! gutter and the page) a way starts again, whatever detour came before,
//! so a pointer that crosses the popup's top or bottom edge always enters
//! it, and one that crosses its side does when each move since it left
//! those other lines headed for it (the gutter counts as the text there).
//! Over the lines it covers, outside its box, the text answers as ever; a
//! pointer that came onto them from beside the popup and moves on under it
//! keeps the popup while where it goes on to tells whether it heads for
//! the popup or for a line under it (see `popup_under`).

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

/// Whether the pointer going from `from` to `to`, on a way that left the
/// popup's anchor (or its gap) at `start`, still heads for `target`: from
/// the point before it, or from where the way started (moves come at
/// whole pixels: a slow slanting move has steps straight up or across,
/// which the triangle from the point before it may leave by a hair), into
/// the triangle and closer (along its side, past the triangle's edge by
/// less than a pixel, is not heading for it).
fn heading_for(from: Point, start: Point, to: Point, target: Rect) -> bool {
    let toward = |from: Point| heads_for(from, to, target) && gets_closer(Some(from), to, target);
    toward(from) || toward(start)
}

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

/// Whether `point` is on `target` itself: its right and bottom edges are
/// past it, as the page hits boxes (a pointer at a card's bottom pixel row
/// is on the line under it).
fn inside((x, y): Point, target: Rect) -> bool {
    x >= target.left && x < target.right && y >= target.top && y < target.bottom
}

/// Whether `point` is on the lines `target` covers (its rows, beside it
/// or under it): the only place where the way to it can be lost.
fn on_covered_lines((_, y): Point, target: Rect) -> bool {
    y >= target.top && y < target.bottom
}

/// The way to a popup after a move (off its anchor and its gap).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Way {
    /// The move heads for the popup: the text it crosses waits.
    heading: bool,
    /// The pointer is on its way there: the popup takes it from here.
    on: bool,
    /// Where the way started.
    start: Option<Point>,
}

/// The way to `target` after the pointer moved from `from` (none: it was
/// not seen) to `to`, on its way there before (`on_way`, since `start`).
/// Off the lines the popup covers, a way starts at `to`, whatever detour
/// came before; one already heading for it goes on from where it started.
fn way_after(
    on_way: bool,
    from: Option<Point>,
    start: Option<Point>,
    to: Point,
    target: Rect,
) -> Way {
    let heading = match from {
        Some(from) if from == to => on_way,
        Some(from) => on_way && heading_for(from, start.unwrap_or(from), to, target),
        None => false,
    };
    let starts = !on_covered_lines(to, target);
    Way {
        heading,
        on: heading || starts,
        start: if starts && !heading { Some(to) } else { start },
    }
}

/// How far `point` is from `target` (zero on it).
fn distance((x, y): Point, target: Rect) -> f64 {
    let dx = (target.left - x).max(x - target.right).max(0.0);
    let dy = (target.top - y).max(y - target.bottom).max(0.0);
    dx.hypot(dy)
}

/// Whether the pointer moving from `from` (none: not seen) to `to` gets
/// closer to `target`, as VS Code's sticky hover counts it: the popup
/// stays while it does, whatever way the pointer takes (a detour round it
/// to its far side, along the gutter), and what the pointer crosses
/// waits. Moving along the popup's side, or away, does not.
fn gets_closer(from: Option<Point>, to: Point, target: Rect) -> bool {
    from.is_some_and(|from| distance(to, target) < distance(from, target))
}

/// Whether the pointer moving from `from` (none: not seen) to `to` goes
/// farther from `target`.
fn gets_farther(from: Option<Point>, to: Point, target: Rect) -> bool {
    from.is_none_or(|from| distance(to, target) > distance(from, target))
}

/// Where the pointer over the text is, for the popup it can enter.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Spot {
    /// Under the popup, which lets it through: the text there answers.
    Under,
    /// In the gap between the popup and its line, or on the popup's edge
    /// (which it does not let through: the pointer is not on it, events
    /// count whole pixels): as good as on it.
    Gap,
    /// Elsewhere.
    Away,
}

/// Where `point` is for `target`, which lets the pointer through
/// (`passing`) or not, the pointer last on its anchor at `anchor_x`.
fn spot(point: Point, target: Rect, passing: bool, anchor_x: Option<f64>) -> Spot {
    if inside(point, target) {
        return if passing { Spot::Under } else { Spot::Gap };
    }
    if in_gap(point, target, anchor_x) {
        Spot::Gap
    } else {
        Spot::Away
    }
}

/// Whether `point` is in the gap the popup `target` keeps from its line,
/// above or below it (not on it, nor beside it: on the lines it covers or
/// passes). Over a long anchor (a value wider than the popup) the gap
/// reaches along the line to `anchor_x`, where the pointer last was on
/// it: a pointer leaving the anchor's end for the popup crosses it there.
fn in_gap((x, y): Point, target: Rect, anchor_x: Option<f64>) -> bool {
    let reach = POPUP_GAP + SLACK;
    let above = y >= target.top - reach && y < target.top;
    let below = y >= target.bottom && y <= target.bottom + reach;
    let (left, right) = anchor_x.map_or((target.left, target.right), |anchor| {
        (target.left.min(anchor), target.right.max(anchor))
    });
    x >= left - SLACK && x <= right + SLACK && (above || below)
}

/// How far outside the triangle to the popup a move may land, in pixels:
/// mouse events count whole pixels, so a straight slanting move lands up
/// to a pixel off its line.
const PIXEL: f64 = 1.0;

/// Whether `point` is inside the triangle `a`, `b`, `c` (or within
/// `PIXEL` of it).
fn in_triangle(point: Point, a: Point, b: Point, c: Point) -> bool {
    // How far `r` is to the left of the edge from `p` to `q`.
    let side = |p: Point, q: Point, r: Point| {
        let length = (q.0 - p.0).hypot(q.1 - p.1);
        if length == 0.0 {
            return 0.0;
        }
        ((q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)) / length
    };
    let sides = [side(a, b, point), side(b, c, point), side(c, a, point)];
    sides.iter().all(|side| *side >= -PIXEL) || sides.iter().all(|side| *side <= PIXEL)
}

/// The box of `popup` in the window (none while it is not laid out).
fn popup_box(popup: &web_sys::HtmlElement) -> Option<Rect> {
    let rect = popup.get_bounding_client_rect();
    (rect.width() > 0.0).then(|| Rect {
        left: rect.left(),
        top: rect.top(),
        right: rect.right(),
        bottom: rect.bottom(),
    })
}

impl EditorState {
    /// The pointer moved to `(x, y)` in the window, over cell `cell` of
    /// zero-based `line`: whether it is on its way to the popup it can
    /// enter, across other text, so what it crosses waits (and the popup
    /// with it).
    pub(super) fn on_the_way_to_popup(&self, x: f64, y: f64, line: usize, cell: usize) -> bool {
        let from = self.hover().pointer.get_value();
        self.hover().pointer.set_value(Some((x, y)));
        let element = self.reachable_popup_element();
        let popup = element.as_ref().and_then(popup_box);
        let passing = element
            .as_ref()
            .is_some_and(super::popup_pointer::lets_through);
        let anchor_x = self.hover().anchor_x.get_value();
        let at = popup.map_or(Spot::Away, |popup| spot((x, y), popup, passing, anchor_x));
        let on_way = self.hover().on_way.get_value();
        // Over the popup, which let the pointer through (it came another
        // way): where it goes on to decides (see `popup_under`).
        if at == Spot::Under {
            self.hover().on_way.set_value(false);
            return element
                .as_ref()
                .is_some_and(|popup| self.under_popup(popup, line, cell));
        }
        self.leave_under();
        let on_anchor = self.on_popup_anchor(line, cell);
        // In the gap between the line and the popup: as good as on it, so
        // resting there keeps it.
        if at == Spot::Gap && !on_anchor {
            self.hover().on_way.set_value(true);
            self.hover().way_start.set_value(Some((x, y)));
            self.popup_takes_pointer();
            self.hold_popup();
            return true;
        }
        if on_anchor {
            self.hover().on_way.set_value(true);
            self.hover().anchor_x.set_value(Some(x));
            self.hover().way_start.set_value(Some((x, y)));
            self.popup_takes_pointer();
            return false;
        }
        let start = self.hover().way_start.get_value();
        let way = popup.map_or_else(
            // Not laid out yet: a pointer at rest stays on its way.
            || {
                let resting = on_way && from == Some((x, y));
                Way {
                    heading: resting,
                    on: resting,
                    start,
                }
            },
            |popup| way_after(on_way, from, start, (x, y), popup),
        );
        self.hover().on_way.set_value(way.on);
        self.hover().way_start.set_value(way.start);
        if popup.is_some_and(|popup| !on_covered_lines((x, y), popup)) {
            self.popup_takes_pointer();
        }
        if !way.heading && !popup.is_some_and(|popup| gets_closer(from, (x, y), popup)) {
            if popup.is_some_and(|popup| !gets_farther(from, (x, y), popup)) {
                self.hold_a_moment();
            }
            return false;
        }
        self.wait_on_the_way();
        true
    }

    /// Whether the pointer came onto the popup on its way there: its last
    /// place (over the text, or where it left it) was off the lines the
    /// popup covers, or on them heading for it from there (each move
    /// inside the triangle to the popup), whatever side it crossed (a card
    /// above a long value is reached slanting across its side from the
    /// value's end). On the lines beside it, the way was lost there. Only
    /// then does the popup let the pointer through.
    pub(super) fn entered_on_the_way(&self) -> bool {
        self.hover().pointer.get_value().is_some() && self.hover().on_way.get_value()
    }

    /// Whether the pointer comes onto `popup` across its side, from the
    /// lines it covers beside it (its last place over the text or the
    /// gutter, at whole pixels: one on the popup's partly covered edge
    /// pixel was still on the text): heading for it or not, it may be going
    /// to one of those lines (see `popup_under`).
    pub(super) fn comes_from_beside(&self, popup: &web_sys::HtmlElement) -> bool {
        let (Some((x, y)), Some(popup)) = (self.hover().pointer.get_value(), popup_box(popup))
        else {
            return false;
        };
        on_covered_lines((x, y), popup) && (x < popup.left + PIXEL || x > popup.right - PIXEL)
    }

    /// The pointer stopped on the way and the popup went: what it rests
    /// on shows, as if it had just moved there (off the text, nothing).
    pub(super) fn point_again(&self) {
        let (Some((x, y)), Some(element)) = (self.hover().pointer.get_value(), self.textarea())
        else {
            return;
        };
        self.hover().on_way.set_value(false);
        if self.hover().on_text.get_value() {
            super::input::pointer_at(*self, &element, x, y, true);
        }
    }

    /// The pointer left the text for `to`, at `(x, y)` in the window: the
    /// next move over it goes on from there (the gutter, the page). Off
    /// the lines the popup covers, a way to it starts there, as over the
    /// text. Onto the popup, the way so far holds (the popup decides
    /// whether it takes the pointer).
    pub(super) fn leave_pointer(&self, to: Option<&web_sys::EventTarget>, x: f64, y: f64) {
        let element = self.reachable_popup_element();
        let onto_popup = element.as_ref().is_some_and(|popup| {
            to.and_then(|to| to.dyn_ref::<web_sys::Node>())
                .is_some_and(|to| popup.contains(Some(to)))
        });
        if !onto_popup {
            self.pointer_off_text(x, y);
        }
    }

    /// The pointer moved to `(x, y)` in the window off the text (the
    /// gutter, the page): as over the text, a way to the popup starts there
    /// off the lines it covers, and is lost beside it on them.
    pub(super) fn pointer_off_text(&self, x: f64, y: f64) {
        self.leave_under();
        let from = self.hover().pointer.get_value();
        self.hover().pointer.set_value(Some((x, y)));
        self.hover().anchor_x.set_value(None);
        let Some(popup) = self.reachable_popup_element().as_ref().and_then(popup_box) else {
            self.hover().on_way.set_value(false);
            return;
        };
        let (on_way, start) = (
            self.hover().on_way.get_value(),
            self.hover().way_start.get_value(),
        );
        let way = way_after(on_way, from, start, (x, y), popup);
        self.hover().on_way.set_value(way.on);
        self.hover().way_start.set_value(way.start);
        if !on_covered_lines((x, y), popup) {
            self.popup_takes_pointer();
        }
        if way.heading || gets_closer(from, (x, y), popup) {
            self.wait_on_the_way();
        } else if !gets_farther(from, (x, y), popup) {
            self.hold_a_moment();
        }
    }

    /// The pointer moved along the popup (no farther from it): the popup
    /// stays its moment from here, while what the pointer is on answers.
    fn hold_a_moment(&self) {
        self.hold_popup();
        self.leave_hover();
        if self.problems().hovered.with_untracked(Option::is_some) {
            self.leave_problem();
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

    /// Whether cell `cell` of zero-based `line` is what the popup is for:
    /// the card's key, or the problems the tip shows.
    pub(super) fn on_popup_anchor(&self, line: usize, cell: usize) -> bool {
        if self.hover().card.with_untracked(Option::is_some) {
            return self
                .docs_under_pointer(line, cell)
                .is_some_and(|info| self.card_shows(line, &info));
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
    pub(super) fn hold_popup(&self) {
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
    use super::{Point, Rect, Spot, heads_for, in_gap, inside, spot, way_after};

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

    /// Whether a pointer going through `points` from the popup's anchor,
    /// in moves of at most `step` pixels reported at whole pixels as mouse
    /// events are, is on its way when it gets onto `card` (the card takes
    /// it), following the rules of `on_the_way_to_popup` (and of
    /// `leave_pointer`: off the text, in the gutter, the same). `None`: it
    /// never got onto it.
    fn walk(points: &[Point], step: f64, card: Rect, anchor: Rect) -> Option<bool> {
        let anchor_x = Some(points[0].0);
        let mut from = points[0];
        let mut on_way = true;
        let mut start = Some(from);
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let steps = ((b.0 - a.0).hypot(b.1 - a.1) / step).round().max(1.0);
            for i in 1..=steps as usize {
                let at = i as f64 / steps;
                let to = (
                    (a.0 + (b.0 - a.0) * at).floor(),
                    (a.1 + (b.1 - a.1) * at).floor(),
                );
                if inside(to, card) {
                    return Some(on_way);
                }
                if inside(to, anchor) || spot(to, card, false, anchor_x) == Spot::Gap {
                    on_way = true;
                    start = Some(to);
                } else {
                    let way = way_after(on_way, Some(from), start, to, card);
                    (on_way, start) = (way.on, way.start);
                }
                from = to;
            }
        }
        None
    }

    /// `walk` straight from `start` to `end` in `steps` moves.
    fn reaches(start: Point, end: Point, steps: usize, card: Rect, anchor: Rect) -> Option<bool> {
        let length = (end.0 - start.0).hypot(end.1 - start.1);
        walk(&[start, end], length / steps as f64, card, anchor)
    }

    /// QA 27 (M27-U1): a card above its line closed on the way up to it
    /// (the pointer at its bottom pixel row counted as on it, which let it
    /// through), and one above a long value was not taken from the value's
    /// end (it came across the card's side, not its bottom). Every way in
    /// small steps from anywhere on the anchor to anywhere on the card, at
    /// 15 to 40 steps, gets onto it on its way, above or below the line.
    #[test]
    fn every_small_step_way_from_the_anchor_reaches_the_card() {
        // A long value at y 140..160 from x 40 to 700, cards 4 px from it.
        let anchor = Rect {
            left: 40.0,
            top: 140.0,
            right: 700.0,
            bottom: 160.0,
        };
        let above = Rect {
            left: 40.5,
            top: 58.6,
            right: 380.5,
            bottom: 136.0,
        };
        let below = Rect {
            top: 164.0,
            bottom: 241.4,
            ..above
        };
        for card in [above, below] {
            for start_x in [45.0, 88.0, 300.0, 520.0, 690.0] {
                for start_y in [141.0, 150.5, 159.0] {
                    for end in [(128.0, 119.0), (370.0, 70.0), (45.0, 130.0), (200.0, 100.0)] {
                        let end = if card == below {
                            (end.0, 300.0 - end.1)
                        } else {
                            end
                        };
                        for steps in [15, 22, 30, 40] {
                            assert_eq!(
                                reaches((start_x, start_y), end, steps, card, anchor),
                                Some(true),
                                "{card:?} from {start_x},{start_y} to {end:?} in {steps}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// QA 28 (F1): a card above its line let the pointer through (and
    /// closed under it) when it got there after straying into the
    /// indentation or the gutter. A detour off the lines the card covers
    /// (left, right, onto the line past the key, past the card's far
    /// edge), at 4 to 16 px a move, keeps the way, above or below the
    /// line.
    #[test]
    fn a_detour_off_the_covered_lines_keeps_the_way() {
        let anchor = Rect {
            left: 60.0,
            top: 140.0,
            right: 100.0,
            bottom: 160.0,
        };
        let above = Rect {
            left: 60.0,
            top: 58.6,
            right: 400.0,
            bottom: 136.0,
        };
        let below = Rect {
            top: 164.0,
            bottom: 241.4,
            ..above
        };
        let key = (66.0, 150.0);
        for card in [above, below] {
            let (away, into, far) = if card == above {
                (174.0, 110.0, 40.0)
            } else {
                (126.0, 190.0, 260.0)
            };
            let detours = [
                (48.0, 146.0),
                (20.0, 150.0),
                (2.0, 150.0),
                (130.0, 146.0),
                (190.0, 150.0),
                (66.0, away),
                (20.0, away),
                (420.0, far),
            ];
            for detour in detours {
                for step in [4.0, 8.0, 16.0] {
                    let way = [key, detour, (100.0, into)];
                    assert_eq!(
                        walk(&way, step, card, anchor),
                        Some(true),
                        "{card:?} via {detour:?} at {step}"
                    );
                }
            }
        }
    }

    /// QA 26 (M26-U1): a card below its line took the pointer that came
    /// onto the lines it covers from beside it, so those lines could not
    /// be hovered, clicked or scrolled. A way from beside the card, or from
    /// its line past its side and then down, has lost its way when it
    /// gets onto it (the card lets it through).
    #[test]
    fn a_way_from_beside_the_card_is_not_on_the_way() {
        let anchor = Rect {
            left: 40.0,
            top: 40.0,
            right: 80.0,
            bottom: 56.0,
        };
        let card = CARD;
        // Out along the line, then beside it and in from the side.
        assert_eq!(reaches((60.0, 45.0), (420.0, 45.0), 20, card, anchor), None);
        let beside = (420.0, 100.0);
        assert!(!heads_for((60.0, 45.0), beside, card));
        for step in [4.0, 8.0, 16.0] {
            // Along the line past the card's side, down beside it, and
            // in under it.
            let way = [(60.0, 45.0), (420.0, 45.0), beside, (100.0, 100.0)];
            assert_eq!(walk(&way, step, card, anchor), Some(false), "{step}");
            // From below the card up beside it, then in.
            let way = [
                (60.0, 45.0),
                (420.0, 45.0),
                (420.0, 260.0),
                beside,
                (100.0, 100.0),
            ];
            assert_eq!(walk(&way, step, card, anchor), Some(false), "{step}");
        }
        // Along its line past its side.
        assert!(!heads_for((60.0, 45.0), (600.0, 45.0), card));
        assert!(!heads_for((600.0, 45.0), (600.0, 100.0), card));
        // An integer pointer on the card's bottom row is past it; on its
        // top row, on it.
        let above = Rect {
            top: 58.6,
            bottom: 136.0,
            ..CARD
        };
        for anchor_x in [None, Some(60.0)] {
            assert_eq!(spot((100.0, 136.0), above, false, anchor_x), Spot::Gap);
            assert_eq!(spot((100.0, 136.0), above, true, anchor_x), Spot::Gap);
            assert_eq!(spot((100.0, 135.0), above, true, anchor_x), Spot::Under);
            assert_eq!(spot((100.0, 135.0), above, false, anchor_x), Spot::Gap);
            assert_eq!(spot((100.0, 145.0), above, false, anchor_x), Spot::Away);
        }
        // Past the card's side over a long anchor: the gap reaches to
        // where the pointer left it, no further.
        assert_eq!(spot((500.0, 138.0), above, false, None), Spot::Away);
        assert_eq!(spot((500.0, 138.0), above, false, Some(520.0)), Spot::Gap);
        assert_eq!(spot((560.0, 138.0), above, false, Some(520.0)), Spot::Away);
    }

    /// QA 29 (L29-U2): a pointer going straight up or down along the
    /// card's side, in the gutter or the indentation, stayed within a pixel
    /// of the triangle to it at each step, so it counted as heading for it
    /// and a line the card covers could not be reached from there. Along
    /// its side on the lines it covers, it gets no closer: the way is
    /// lost, whatever the step, however near the side.
    #[test]
    fn a_way_along_the_cards_side_is_lost() {
        let anchor = Rect {
            left: 60.0,
            top: 140.0,
            right: 100.0,
            bottom: 160.0,
        };
        let above = Rect {
            left: 60.0,
            top: 58.6,
            right: 400.0,
            bottom: 136.0,
        };
        let below = Rect {
            top: 164.0,
            bottom: 241.4,
            ..above
        };
        for card in [above, below] {
            let line = if card == above { 125.0 } else { 175.0 };
            for x in [54.0, 48.0, 30.0] {
                for step in [2.0, 4.0, 8.0] {
                    let way = [(66.0, 150.0), (x, 150.0), (x, line), (100.0, line)];
                    assert_eq!(
                        walk(&way, step, card, anchor),
                        Some(false),
                        "{card:?} at {x} by {step}"
                    );
                }
            }
        }
    }

    /// QA 24 (L24-U1): the pointer resting in the 4 px gap between the
    /// line and the card let it close. The gap above and below it counts
    /// as on the way; the line itself, and beside the card, do not. QA 26
    /// (M26-U1): nor does the card itself, or the lines just beside it
    /// (which it lets the pointer through to when it came another way).
    #[test]
    fn the_gap_between_the_line_and_the_card_is_on_the_way() {
        for y in [56.5, 58.0, 59.5, 203.0] {
            assert!(in_gap((100.0, y), CARD, None), "{y}");
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
            assert!(!in_gap((x, y), CARD, Some(60.0)), "{x}, {y}");
        }
    }
}
