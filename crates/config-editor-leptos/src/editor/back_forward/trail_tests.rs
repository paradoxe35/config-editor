use super::super::super::edit::Change;
use super::super::super::text::{TextRange, TextSize};
use super::{CAP, Place, Spot, Trail};

fn at(editor: u64, caret: u32) -> Spot {
    Spot {
        editor,
        place: Place {
            anchor: TextSize::from(caret),
            caret: TextSize::from(caret),
            top: i32::try_from(caret).unwrap() * 10,
            left: 0,
        },
    }
}

fn any(_: &Spot) -> bool {
    true
}

#[test]
fn back_returns_the_place_a_jump_left_and_forward_goes_again() {
    let mut trail = Trail::default();
    trail.leave(at(1, 5));
    assert!(trail.can_back(at(1, 90), any));
    assert!(!trail.can_forward(at(1, 90), any));
    assert_eq!(trail.back(at(1, 90), any), Some(at(1, 5)));
    assert!(!trail.can_back(at(1, 5), any));
    assert_eq!(trail.back(at(1, 5), any), None);
    assert_eq!(trail.forward(at(1, 5), any), Some(at(1, 90)));
    assert_eq!(trail.back(at(1, 90), any), Some(at(1, 5)));
}

#[test]
fn a_new_jump_clears_the_forward_places() {
    let mut trail = Trail::default();
    trail.leave(at(1, 5));
    trail.back(at(1, 90), any);
    assert!(trail.can_forward(at(1, 5), any));
    trail.leave(at(1, 5));
    assert!(!trail.can_forward(at(1, 40), any));
    assert_eq!(trail.back(at(1, 40), any), Some(at(1, 5)));
}

#[test]
fn the_stacks_keep_the_latest_places_up_to_the_cap() {
    let mut trail = Trail::default();
    let count = u32::try_from(CAP).unwrap() + 10;
    for caret in 0..count {
        trail.leave(at(1, caret));
    }
    let mut gone_back = Vec::new();
    let mut here = at(1, 1_000);
    while let Some(spot) = trail.back(here, any) {
        gone_back.push(spot.place.caret);
        here = spot;
    }
    assert_eq!(gone_back.len(), CAP);
    assert_eq!(gone_back.first(), Some(&TextSize::from(count - 1)));
    assert_eq!(gone_back.last(), Some(&TextSize::from(10)));
}

#[test]
fn leaving_the_same_selection_twice_keeps_one_place() {
    let mut trail = Trail::default();
    trail.leave(at(1, 5));
    trail.leave(at(1, 5));
    assert_eq!(trail.back(at(1, 90), any), Some(at(1, 5)));
    assert_eq!(trail.back(at(1, 5), any), None);
}

#[test]
fn places_follow_edits_of_their_editor_and_go_with_their_text() {
    let text = "aaaa\nbbbb\ncccc\n";
    let mut trail = Trail::default();
    trail.leave(at(1, 12));
    trail.leave(at(1, 7));
    trail.leave(at(2, 12));
    trail.leave(at(1, 1));
    // Two lines typed at the start move every later place of editor 1;
    // editor 2's place stays.
    let typed = Change::replace(text, TextRange::empty(TextSize::from(0)), "xx\nyy\n");
    trail.follow(1, &typed);
    let mut here = at(1, 100);
    let mut seen = Vec::new();
    while let Some(spot) = trail.back(here, any) {
        seen.push((spot.editor, u32::from(spot.place.caret)));
        here = spot;
    }
    assert_eq!(seen, [(1, 7), (2, 12), (1, 13), (1, 18)]);
    // The scroll is kept as it was: restoring reveals the caret if needed.
    assert_eq!(here.place.top, 120);
}

#[test]
fn a_place_inside_deleted_text_is_dropped() {
    let text = "aaaa\nbbbb\ncccc\n";
    let mut trail = Trail::default();
    trail.leave(at(1, 2));
    trail.leave(at(1, 7));
    let deleted = Change::replace(text, TextRange::new(5.into(), 10.into()), "");
    trail.follow(1, &deleted);
    assert_eq!(
        trail.back(at(1, 0), any).map(|spot| spot.place.caret),
        Some(2.into())
    );
    assert_eq!(trail.back(at(1, 2), any), None);
}

#[test]
fn places_of_a_gone_editor_are_skipped_and_forgotten() {
    let mut trail = Trail::default();
    trail.leave(at(1, 5));
    trail.leave(at(2, 8));
    let alive = |spot: &Spot| spot.editor != 2;
    assert!(trail.can_back(at(1, 50), alive));
    assert_eq!(trail.back(at(1, 50), alive), Some(at(1, 5)));
    trail.forget(1);
    assert!(!trail.can_forward(at(3, 0), any));
}
