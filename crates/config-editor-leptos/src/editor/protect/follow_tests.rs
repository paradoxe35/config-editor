//! A host's range followed through an edit: a line moved takes its range
//! along, a copied one keeps it on one copy (it used to shrink to nothing where the edit was, and
//! the value then took typing); text before it leaves it, text added above
//! moves it.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use super::super::text::{TextRange, TextSize, text_size};
use super::follow::follow_ranges;

/// The range of `part` in `text`.
fn range_of(text: &str, part: &str) -> TextRange {
    let at = text.find(part).expect("the part");
    TextRange::at(text_size(at), TextSize::of(part))
}

#[test]
fn a_moved_line_takes_its_range_along() {
    let before = "a: 1\nport: 512\nb: 2\n";
    let range = range_of(before, "512");
    // Moved up (Alt+Up), and down (the line below moved up past it).
    let up = "port: 512\na: 1\nb: 2\n";
    assert_eq!(
        follow_ranges(std::slice::from_ref(&range), before, up, 0.into(), None),
        [range_of(up, "512")]
    );
    let down = "a: 1\nb: 2\nport: 512\n";
    assert_eq!(
        follow_ranges(std::slice::from_ref(&range), before, down, 5.into(), None),
        [range_of(down, "512")]
    );
}

#[test]
fn a_copied_line_keeps_its_range_on_one_of_the_copies() {
    let before = "a: 1\nport: 512\nb: 2\n";
    let range = range_of(before, "512");
    let copied = "a: 1\nport: 512\nport: 512\nb: 2\n";
    for caret in [5u32, 15, 25] {
        let [followed] = &follow_ranges(
            std::slice::from_ref(&range),
            before,
            copied,
            caret.into(),
            None,
        )[..] else {
            panic!("one range");
        };
        assert_eq!(&copied[*followed], "512", "caret {caret}");
    }
}

#[test]
fn edits_before_and_after_a_range_shift_or_leave_it() {
    let before = "a: 1\nport: 512\n";
    let range = range_of(before, "512");
    let added_above = "x: 0\na: 1\nport: 512\n";
    assert_eq!(
        follow_ranges(
            std::slice::from_ref(&range),
            before,
            added_above,
            5.into(),
            None
        ),
        [range_of(added_above, "512")]
    );
    let added_below = "a: 1\nport: 512\nz: 9\n";
    assert_eq!(
        follow_ranges(
            std::slice::from_ref(&range),
            before,
            added_below,
            19.into(),
            None
        ),
        [range]
    );
}
