//! The model's transitions: each reset carries the ranges as
//! given, afresh; nothing a reset hands the editor becomes the given ones.
use config_editor_leptos::{TextPosition, TextRange};

use super::HostLocks;

const T: &str = "name: a\nsecret: keep\nlisten: 1\n";
const ABOVE: &str = "z: 0\nname: a\nsecret: keep\nlisten: 1\n";
const TWO_ABOVE: &str = "x: 1\ny: 2\nname: a\nsecret: keep\nlisten: 1\n";

fn line(number: usize) -> TextRange {
    TextRange::new(TextPosition::new(number, 1), TextPosition::new(number, 13))
}

/// P1: resets in any order, and a text of the host's own after
/// them, each hand the lock on the line holding `secret: keep`: never
/// moved from what an earlier reset handed.
#[test]
fn resets_compose_from_the_ranges_as_given() {
    let sequences: &[&[(&str, usize)]] = &[
        &[(ABOVE, 3), (T, 2)],
        &[(ABOVE, 3), (TWO_ABOVE, 4), (T, 2)],
        &[(TWO_ABOVE, 4), (ABOVE, 3), (ABOVE, 3)],
    ];
    for sequence in sequences {
        let mut locks = HostLocks::given(Some("[2]".into()), T, T, vec![line(2)]);
        for (text, want) in *sequence {
            assert_eq!(locks.replaced(text), vec![line(*want)], "{sequence:?}");
        }
    }
}

/// P1: the same ranges given again stay while no text of the host's
/// own came; after one (a reset, or ranges not taken), they are read anew.
#[test]
fn the_same_ranges_stay_until_a_text_of_the_hosts_own() {
    let mut locks = HostLocks::given(Some("[2]".into()), T, T, vec![line(2)]);
    assert!(locks.again(Some("[2]")));
    assert!(!locks.again(Some("[3]")));
    locks.replaced(ABOVE);
    assert!(!locks.again(Some("[2]")));
    let mut locks = HostLocks::given(Some("[2]".into()), T, T, vec![line(2)]);
    locks.forget();
    assert!(!locks.again(Some("[2]")));
    // Unprintable ranges equal none.
    let locks = HostLocks::given(None, T, T, vec![line(2)]);
    assert!(!locks.again(None));
}

/// P1: ranges given next that do not fit a reset's text may have been
/// worked out on the text they came with before it (a host that renders the
/// ranges it had after a reset of its own): that text stays a place to read
/// them, through resets one after another, until ranges are given anew.
#[test]
fn ranges_after_a_reset_may_be_worked_out_on_the_text_before_it() {
    let mut locks = HostLocks::given(Some("[2]".into()), T, T, vec![line(2)]);
    assert_eq!(locks.came_with(), [T, T]);
    locks.replaced(ABOVE);
    assert_eq!(locks.came_with(), [ABOVE, T]);
    locks.replaced(TWO_ABOVE);
    assert_eq!(locks.came_with(), [TWO_ABOVE, T]);
    let locks = HostLocks::given(Some("[3]".into()), ABOVE, ABOVE, vec![line(3)]);
    assert_eq!(locks.came_with(), [ABOVE, ABOVE]);
}
