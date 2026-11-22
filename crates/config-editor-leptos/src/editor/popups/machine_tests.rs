//! The pointer's phase, as pure transitions: each rule of "the popup stays
//! while the pointer heads for it" (on the way, over the gap, on a quick
//! return, through a detour, into the popup) and of the timers each phase
//! owns.

use super::super::path::{Point, Rect};
use super::{At, Fire, Kind, Machine, Opener, Over, Phase, Settled, Sight, Then};

/// A card above its key's line, as the hover card opens when it fits.
const CARD: Rect = Rect {
    left: 60.0,
    top: 58.0,
    right: 400.0,
    bottom: 136.0,
};

/// The key the card is for, on the line under it (y 140..160).
const KEY: Point = (66.0, 150.0);

type Pointer = Machine<&'static str>;

/// The host's hover delay, the default.
const REST_MS: i32 = 300;

fn seen(on_anchor: bool) -> Sight {
    Sight {
        popup: Some(CARD),
        on_anchor,
    }
}

/// A card the pointer opened: it rested on `KEY` through the pause.
fn card_open() -> Pointer {
    let mut machine = Pointer::default();
    machine.on_text_again();
    assert_eq!(
        machine.over_text(
            KEY,
            4,
            1,
            Sight {
                popup: None,
                on_anchor: false
            }
        ),
        Over::Answers
    );
    machine.pend("port", KEY, 0.0, REST_MS);
    assert_eq!(
        restarted(&mut machine),
        Some(Some(REST_MS)),
        "the rest runs"
    );
    assert_eq!(machine.fired(), Some(Fire::Open("port")));
    machine.opened(Kind::Card, Opener::Pointer);
    restarted(&mut machine);
    machine
}

/// The delay of the timer the last phase entered runs, once per entry
/// (none: no new phase).
fn restarted(machine: &mut Pointer) -> Option<Option<i32>> {
    machine.restart().map(|(_, ms)| ms)
}

/// The zero-based line at `point`: 20 pixels a line (the key's, y 140..160,
/// is line 7).
fn line_of((_, y): Point) -> usize {
    (y / 20.0) as usize
}

/// Moves over the text from `from` to `to` in moves of about 8 pixels, at
/// whole pixels as mouse events come: what each move did.
fn walk(machine: &mut Pointer, from: Point, to: Point) -> Vec<Over> {
    let steps = ((to.0 - from.0).hypot(to.1 - from.1) / 8.0).ceil().max(1.0) as usize;
    (1..=steps)
        .map(|step| {
            let at = step as f64 / steps as f64;
            let point = (
                (from.0 + (to.0 - from.0) * at).floor(),
                (from.1 + (to.1 - from.1) * at).floor(),
            );
            let inside = point.0 >= CARD.left
                && point.0 < CARD.right
                && point.1 >= CARD.top
                && point.1 < CARD.bottom;
            if inside {
                // The page sends the card its own events from here.
                return if machine.enter_popup(Some(CARD)) {
                    Over::Waits
                } else {
                    machine.over_text(point, line_of(point), 1, seen(false))
                };
            }
            machine.over_text(point, line_of(point), 1, seen(false))
        })
        .collect()
}

/// Keep the card on the way to it: each move heading for it makes the
/// text it crosses wait and gives the card its moment again (to close and
/// let what the pointer stops on show), or holds it in the gap; the
/// pointer gets into it.
#[test]
fn the_card_stays_while_the_pointer_heads_for_it() {
    let mut machine = card_open();
    let mut from = KEY;
    for (step, to) in [(66.0, 146.0), (70.0, 141.0), (90.0, 130.0), (120.0, 120.0)]
        .into_iter()
        .enumerate()
    {
        let moves = walk(&mut machine, from, to);
        assert!(
            moves.iter().all(|over| *over == Over::Waits),
            "step {step}: {moves:?}"
        );
        from = to;
        let held = matches!(machine.phase(), Phase::Open { at: At::Text, .. });
        let waits = matches!(
            machine.phase(),
            Phase::Leaving {
                kind: Kind::Card,
                then: Then::CloseAndPointAgain,
                ..
            }
        );
        assert!(
            held || waits || machine.inside(),
            "step {step}: {:?}",
            machine.phase()
        );
        if waits {
            assert_eq!(
                restarted(&mut machine),
                Some(Some(350)),
                "step {step}: the moment again"
            );
        }
    }
    assert!(machine.inside(), "{:?}", machine.phase());
    // In it, nothing closes it.
    assert_eq!(machine.fired(), None);
}

/// A pointer that stops on the way lets the card go after its moment, and
/// what it stopped on shows then.
#[test]
fn a_pointer_stopped_on_the_way_lets_the_card_go() {
    let mut machine = card_open();
    walk(&mut machine, KEY, (100.0, 146.0));
    assert_eq!(
        machine.fired(),
        Some(Fire::Close {
            kind: Kind::Card,
            then: Then::CloseAndPointAgain
        })
    );
    machine.closed(0.0);
    assert_eq!(machine.phase(), &Phase::Closed);
    assert!(machine.point_again(), "over the text: what is there shows");
}

/// Keep the card over the gap: resting in the gap between the line and
/// the card holds it, with no moment running.
#[test]
fn resting_in_the_gap_holds_the_card() {
    let mut machine = card_open();
    machine.leave(Kind::Card);
    assert_eq!(restarted(&mut machine), Some(Some(350)));
    let gap = (100.0, 138.0);
    assert_eq!(machine.over_text(gap, 4, 1, seen(false)), Over::Waits);
    assert!(matches!(machine.phase(), Phase::Open { at: At::Text, .. }));
    assert_eq!(
        restarted(&mut machine),
        Some(None),
        "the moment is called off"
    );
    assert_eq!(machine.over_text(gap, 4, 1, seen(false)), Over::Waits);
    assert_eq!(machine.fired(), None);
}

/// Keep the card on a quick return: back on its key within its moment,
/// it stays (the editor holds it), without closing to open again.
#[test]
fn a_quick_return_to_the_key_keeps_the_card() {
    let mut machine = card_open();
    machine.leave(Kind::Card);
    restarted(&mut machine);
    // A second leave does not put the moment off.
    machine.leave(Kind::Card);
    assert_eq!(restarted(&mut machine), None);
    assert_eq!(machine.over_text(KEY, 4, 1, seen(true)), Over::Answers);
    machine.hold();
    assert!(matches!(
        machine.phase(),
        Phase::Open {
            kind: Kind::Card,
            ..
        }
    ));
    assert_eq!(restarted(&mut machine), Some(None));
}

/// The pointer stopped where it is: a card that let it through decides
/// it went to the card (no text of its own there), then the next move
/// enters it.
fn rest_on_card(machine: &mut Pointer) {
    if let Some(Fire::Settle { line, .. }) = machine.fired() {
        assert_eq!(machine.settle(line, false), Settled::Popup);
        machine.enter_popup(Some(CARD));
    }
}

/// Keep a card after any detour: off the lines the card covers (into the
/// gutter, past its far side) the way starts again, so the pointer coming
/// back into it from there is taken, at once or (across its side) where
/// it stops on it; the card never goes meanwhile.
#[test]
fn a_detour_off_the_covered_lines_still_enters_the_card() {
    for detour in [(2.0, 150.0), (190.0, 150.0), (420.0, 40.0), (20.0, 174.0)] {
        let mut machine = card_open();
        walk(&mut machine, KEY, detour);
        walk(&mut machine, detour, (100.0, 110.0));
        assert!(
            !matches!(machine.phase(), Phase::Closed),
            "via {detour:?}: the card stays"
        );
        rest_on_card(&mut machine);
        assert!(machine.inside(), "via {detour:?}: {:?}", machine.phase());
    }
}

/// Keep a card that still shows through any detour: a pointer getting
/// closer to it keeps it, whichever way it goes, off the text too.
#[test]
fn getting_closer_off_the_text_keeps_the_card() {
    let mut machine = card_open();
    machine.left_text((40.0, 150.0), false, Some(CARD));
    machine.off_text((52.0, 128.0), Some(CARD));
    assert!(matches!(
        machine.phase(),
        Phase::Leaving {
            then: Then::CloseAndPointAgain,
            ..
        }
    ));
    assert!(!machine.on_text());
    // Back onto the text and into the card.
    machine.on_text_again();
    walk(&mut machine, (58.0, 120.0), (100.0, 110.0));
    rest_on_card(&mut machine);
    assert!(machine.inside(), "{:?}", machine.phase());
}

/// A pointer coming onto the card from beside it, on the lines it covers,
/// is let through to the text: while it moves along the line it came in
/// on the text waits; stopped on text with its own docs, that shows; off
/// that line, the card takes it.
#[test]
fn a_pointer_from_beside_the_card_is_let_through_until_it_stops() {
    let mut machine = card_open();
    walk(&mut machine, KEY, (420.0, 150.0));
    walk(&mut machine, (420.0, 150.0), (420.0, 100.0));
    assert!(!machine.enter_popup(Some(CARD)), "lost its way beside it");
    assert!(machine.passing());
    assert_eq!(
        machine.over_text((390.0, 100.0), 2, 30, seen(false)),
        Over::Waits
    );
    assert!(matches!(
        machine.phase(),
        Phase::Open {
            at: At::Under { entered: 2, .. },
            ..
        }
    ));
    assert_eq!(
        restarted(&mut machine),
        Some(Some(120)),
        "where it stops decides"
    );
    assert_eq!(
        machine.fired(),
        Some(Fire::Settle {
            line: 2,
            column: 30
        })
    );
    assert_eq!(machine.settle(2, true), Settled::Text);
    assert!(matches!(machine.phase(), Phase::Open { at: At::Text, .. }));

    // Off the line it came in along: the card takes it.
    let mut machine = card_open();
    walk(&mut machine, KEY, (420.0, 150.0));
    walk(&mut machine, (420.0, 150.0), (420.0, 100.0));
    machine.enter_popup(Some(CARD));
    machine.over_text((390.0, 100.0), 2, 30, seen(false));
    assert_eq!(
        machine.over_text((380.0, 80.0), 1, 30, seen(false)),
        Over::Waits
    );
    assert!(!machine.passing());
    assert!(machine.enter_popup(Some(CARD)));
}

/// While a card waits out its pause the tip the pointer is on keeps
/// showing; the card's pause ending opens the card in its place, and a
/// pause given up hands the tip back its own phase and timer.
#[test]
fn a_pending_card_keeps_the_tip_behind_it() {
    let mut machine = Pointer::default();
    machine.over_text(
        KEY,
        4,
        1,
        Sight {
            popup: None,
            on_anchor: false,
        },
    );
    machine.opened(Kind::Tip, Opener::Pointer);
    restarted(&mut machine);
    machine.pend("port", KEY, 0.0, REST_MS);
    assert_eq!(
        restarted(&mut machine),
        Some(Some(100)),
        "short: the tip shows"
    );
    // The pointer leaves the problems: the tip would leave, but the
    // pause runs the one timer.
    machine.leave(Kind::Tip);
    assert_eq!(restarted(&mut machine), None);
    machine.cancel_pending();
    assert!(matches!(
        machine.phase(),
        Phase::Leaving {
            kind: Kind::Tip,
            ..
        }
    ));
    assert_eq!(restarted(&mut machine), Some(Some(200)), "the tip's moment");

    machine.hold();
    machine.pend("port", KEY, 0.0, REST_MS);
    assert_eq!(machine.fired(), Some(Fire::Open("port")));
    assert!(matches!(
        machine.phase(),
        Phase::Open {
            kind: Kind::Tip,
            ..
        }
    ));
    machine.opened(Kind::Card, Opener::Pointer);
    assert!(matches!(
        machine.phase(),
        Phase::Open {
            kind: Kind::Card,
            ..
        }
    ));
}

/// A popup the caret opened never leaves for the pointer, and takes the
/// pointer from anywhere, beside it too.
#[test]
fn a_popup_the_caret_opened_stays_and_takes_the_pointer() {
    let mut machine = Pointer::default();
    machine.over_text(
        (420.0, 100.0),
        2,
        40,
        Sight {
            popup: None,
            on_anchor: false,
        },
    );
    machine.opened(Kind::Card, Opener::Caret);
    restarted(&mut machine);
    machine.leave(Kind::Card);
    machine.over_text((600.0, 300.0), 9, 40, seen(false));
    assert!(matches!(
        machine.phase(),
        Phase::Open {
            by: Opener::Caret,
            ..
        }
    ));
    assert_eq!(machine.fired(), None);
    assert!(machine.enter_popup(Some(CARD)));
}

/// The pointer in the popup keeps it; leaving it starts its moment.
#[test]
fn leaving_the_popup_starts_its_moment() {
    let mut machine = card_open();
    walk(&mut machine, KEY, (200.0, 100.0));
    assert!(machine.inside());
    machine.leave(Kind::Card);
    assert!(machine.inside(), "in it, the pointer has not left it");
    assert!(machine.left_popup());
    assert!(!machine.left_popup());
    machine.leave(Kind::Card);
    assert_eq!(restarted(&mut machine), Some(Some(350)));
    assert_eq!(
        machine.fired(),
        Some(Fire::Close {
            kind: Kind::Card,
            then: Then::Close
        })
    );
}

/// Entering any phase hands back the timer of the one before, to cancel.
#[test]
fn each_new_phase_hands_back_the_timer_before_it() {
    use crate::reactive::later::Timeout;
    let mut machine = card_open();
    machine.leave(Kind::Card);
    let first = machine.restart().expect("entered");
    assert_eq!(first.1, Some(350));
    machine.set_timer(Timeout::default());
    machine.closed(0.0);
    let (_, next) = machine.restart().expect("entered");
    assert_eq!(next, None, "closed: no timer");
    assert_eq!(machine.restart(), None, "once per entry");
}

/// The card opens once the pointer rests, not a fixed time after it came
/// onto the key: a hand drifting a few pixels is at rest (the wait goes
/// on, and what it drifts onto shows), a farther move starts the wait
/// again from there.
#[test]
fn the_card_opens_once_the_pointer_rests() {
    let mut machine = Pointer::default();
    machine.pend("port", KEY, 0.0, REST_MS);
    assert_eq!(restarted(&mut machine), Some(Some(REST_MS)));
    machine.pend("port", (KEY.0 + 3.0, KEY.1 - 2.0), 50.0, REST_MS);
    assert_eq!(restarted(&mut machine), None, "drifting: still at rest");
    machine.pend("host", (KEY.0 + 2.0, KEY.1 + 3.0), 80.0, REST_MS);
    assert_eq!(restarted(&mut machine), None, "drifting onto the next key");
    machine.pend("host", (KEY.0 + 8.0, KEY.1), 120.0, REST_MS);
    assert_eq!(
        restarted(&mut machine),
        Some(Some(REST_MS)),
        "moving: the wait starts again"
    );
    // Drifting is measured from where the pointer came to rest again.
    machine.pend("host", (KEY.0 + 11.0, KEY.1), 150.0, REST_MS);
    assert_eq!(restarted(&mut machine), None);
    assert_eq!(machine.fired(), Some(Fire::Open("host")));
    assert_eq!(machine.phase(), &Phase::Closed);
}

/// The host's hover delay sets the rest, a zero one too.
#[test]
fn the_rest_is_the_host_hover_delay() {
    for rest in [0, 120, 1500] {
        let mut machine = Pointer::default();
        machine.pend("port", KEY, 0.0, rest);
        assert_eq!(restarted(&mut machine), Some(Some(rest)));
    }
}

/// A rest over with nothing to show waits for nothing more on the same
/// key, wherever the pointer moves on it; another key waits again.
#[test]
fn a_rest_with_nothing_to_show_waits_no_more_on_its_key() {
    let mut machine = Pointer::default();
    machine.pend("port", KEY, 0.0, REST_MS);
    restarted(&mut machine);
    assert_eq!(machine.fired(), Some(Fire::Open("port")));
    restarted(&mut machine);
    machine.pend_quietly("port");
    assert_eq!(restarted(&mut machine), None);
    machine.pend("port", (KEY.0 + 30.0, KEY.1), 400.0, REST_MS);
    assert_eq!(restarted(&mut machine), None, "the same key");
    machine.pend("host", (KEY.0 + 60.0, KEY.1), 450.0, REST_MS);
    assert_eq!(restarted(&mut machine), Some(Some(REST_MS)));
}

/// Moving from popup to popup is quick: while one shows, or within a
/// moment after it closed, the next shows after a short rest (never
/// longer than the host's delay); later, after the full rest again.
#[test]
fn the_next_popup_after_one_shows_after_a_short_rest() {
    // While the card shows.
    let mut machine = card_open();
    machine.pend("host", (KEY.0, KEY.1 + 20.0), 1000.0, REST_MS);
    assert_eq!(restarted(&mut machine), Some(Some(100)));

    // Just after it closed: short; long after, the full rest.
    for (rest_at, ms) in [(1200.0, 100), (1300.0, 100), (1301.0, REST_MS)] {
        let mut machine = card_open();
        machine.closed(1000.0);
        restarted(&mut machine);
        machine.pend("host", (KEY.0, KEY.1 + 20.0), rest_at, REST_MS);
        assert_eq!(restarted(&mut machine), Some(Some(ms)), "at {rest_at}");
    }

    // A host delay shorter than the short rest stays shorter.
    let mut machine = card_open();
    machine.pend("host", (KEY.0, KEY.1 + 20.0), 1000.0, 40);
    assert_eq!(restarted(&mut machine), Some(Some(40)));

    // Nothing shown before: nothing closed, the full rest.
    let mut machine = Pointer::default();
    machine.closed(1000.0);
    machine.pend("host", KEY, 1100.0, REST_MS);
    assert_eq!(restarted(&mut machine), Some(Some(REST_MS)));
}

/// The card stays its moment after the pointer leaves its key whatever
/// the host's hover delay, so a quick (or zero) delay still leaves it
/// reachable.
#[test]
fn the_card_stays_its_moment_whatever_the_hover_delay() {
    for rest in [0, 50, 2000] {
        let mut machine = Pointer::default();
        machine.pend("port", KEY, 0.0, rest);
        restarted(&mut machine);
        assert_eq!(machine.fired(), Some(Fire::Open("port")));
        machine.opened(Kind::Card, Opener::Pointer);
        restarted(&mut machine);
        machine.leave(Kind::Card);
        assert_eq!(restarted(&mut machine), Some(Some(350)), "delay {rest}");
    }
}
