//! A host's range followed through an edit: a line moved takes its range
//! along, a copied one keeps it on one copy (it used to shrink to nothing where the edit was, and
//! the value then took typing); text before it leaves it, text added above
//! moves it.

use super::follow::follow_ranges;

/// UTF-16 range of `part` in `text` (ASCII here).
fn range_of(text: &str, part: &str) -> std::ops::Range<usize> {
    let at = text.find(part).expect("the part");
    at..at + part.len()
}

#[test]
fn a_moved_line_takes_its_range_along() {
    let before = "a: 1\nport: 512\nb: 2\n";
    let range = range_of(before, "512");
    // Moved up (Alt+Up), and down (the line below moved up past it).
    let up = "port: 512\na: 1\nb: 2\n";
    assert_eq!(
        follow_ranges(std::slice::from_ref(&range), before, up, 0),
        [range_of(up, "512")]
    );
    let down = "a: 1\nb: 2\nport: 512\n";
    assert_eq!(
        follow_ranges(std::slice::from_ref(&range), before, down, 5),
        [range_of(down, "512")]
    );
}

#[test]
fn a_copied_line_keeps_its_range_on_one_of_the_copies() {
    let before = "a: 1\nport: 512\nb: 2\n";
    let range = range_of(before, "512");
    let copied = "a: 1\nport: 512\nport: 512\nb: 2\n";
    for caret in [5, 15, 25] {
        let [followed] = &follow_ranges(std::slice::from_ref(&range), before, copied, caret)[..]
        else {
            panic!("one range");
        };
        assert_eq!(&copied[followed.clone()], "512", "caret {caret}");
    }
}

#[test]
fn edits_before_and_after_a_range_shift_or_leave_it() {
    let before = "a: 1\nport: 512\n";
    let range = range_of(before, "512");
    let added_above = "x: 0\na: 1\nport: 512\n";
    assert_eq!(
        follow_ranges(std::slice::from_ref(&range), before, added_above, 5),
        [range_of(added_above, "512")]
    );
    let added_below = "a: 1\nport: 512\nz: 9\n";
    assert_eq!(
        follow_ranges(std::slice::from_ref(&range), before, added_below, 19),
        [range]
    );
}
