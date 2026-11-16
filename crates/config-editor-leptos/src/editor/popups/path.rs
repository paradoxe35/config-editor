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
//! the popup or for a line under it (see `under`).
//!
//! Pure geometry: the pointer's phase (see `machine`) asks it.

use super::super::positioning::POPUP_GAP;

/// How far around the popup a move still heads for it, in pixels: room for
/// rounding, not so much that it reaches over the popup's own line.
const SLACK: f64 = 2.0;

/// A box in the window's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) right: f64,
    pub(crate) bottom: f64,
}

pub(crate) type Point = (f64, f64);

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
pub(super) fn inside((x, y): Point, target: Rect) -> bool {
    x >= target.left && x < target.right && y >= target.top && y < target.bottom
}

/// Whether `point` is on the lines `target` covers (its rows, beside it
/// or under it): the only place where the way to it can be lost.
pub(super) fn on_covered_lines((_, y): Point, target: Rect) -> bool {
    y >= target.top && y < target.bottom
}

/// The way to a popup after a move (off its anchor and its gap).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Way {
    /// The move heads for the popup: the text it crosses waits.
    pub(super) heading: bool,
    /// The pointer is on its way there: the popup takes it from here.
    pub(super) on: bool,
    /// Where the way started.
    pub(super) start: Option<Point>,
}

/// The way to `target` after the pointer moved from `from` (none: it was
/// not seen) to `to`, on its way there before (`on_way`, since `start`).
/// Off the lines the popup covers, a way starts at `to`, whatever detour
/// came before; one already heading for it goes on from where it started.
pub(super) fn way_after(
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
pub(super) fn gets_closer(from: Option<Point>, to: Point, target: Rect) -> bool {
    from.is_some_and(|from| distance(to, target) < distance(from, target))
}

/// Whether the pointer moving from `from` (none: not seen) to `to` goes
/// farther from `target`.
pub(super) fn gets_farther(from: Option<Point>, to: Point, target: Rect) -> bool {
    from.is_none_or(|from| distance(to, target) > distance(from, target))
}

/// Where the pointer over the text is, for the popup it can enter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Spot {
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
pub(super) fn spot(point: Point, target: Rect, passing: bool, anchor_x: Option<f64>) -> Spot {
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

/// Whether `point` is on the lines `target` covers, beside it: at whole
/// pixels, so one on the popup's partly covered edge pixel was still on
/// the text.
pub(super) fn beside(point: Point, target: Rect) -> bool {
    on_covered_lines(point, target)
        && (point.0 < target.left + PIXEL || point.0 > target.right - PIXEL)
}

/// The box of `popup` in the window (none while it is not laid out).
pub(super) fn popup_box(popup: &web_sys::HtmlElement) -> Option<Rect> {
    let rect = popup.get_bounding_client_rect();
    (rect.width() > 0.0).then(|| Rect {
        left: rect.left(),
        top: rect.top(),
        right: rect.right(),
        bottom: rect.bottom(),
    })
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

    /// The pointer heading for the card across the next line's key keeps
    /// the card (that key's card or problem tip waits): every small step
    /// towards a point inside it heads for it; a step away, or along the
    /// line, does not.
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
    /// it), following the rules of `Machine::over_text` (and of
    /// `off_text`: off the text, in the gutter, the same). `None`: it
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

    /// Every way in small steps from anywhere on the anchor to anywhere on
    /// the card, at 15 to 40 steps, gets onto it on its way, above or below
    /// the line: across its bottom pixel row (which is past it, not on it),
    /// and across its side from the end of a value longer than the card.
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

    /// A detour off the lines the card covers (into the indentation or the
    /// gutter, onto the line past the key, past the card's far edge), at 4
    /// to 16 px a move, keeps the way, above or below the line: the card
    /// takes the pointer that gets there after it.
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

    /// A way from beside the card, or from its line past its side and then
    /// down, has lost its way when it gets onto it: the card lets it through,
    /// so the lines it covers can still be hovered, clicked and scrolled.
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

    /// A pointer going straight up or down along the card's side (in the
    /// gutter or the indentation) stays within a pixel of the triangle to
    /// it, but on the lines it covers it gets no closer: the way is lost,
    /// whatever the step, however near the side, so those lines can be
    /// reached from there.
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

    /// The 4 px gap above and below the card counts as on the way, so a
    /// pointer resting there keeps it; the line itself, beside the card,
    /// the card itself and the lines just beside it (which it lets the
    /// pointer through to when it came another way) do not.
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
