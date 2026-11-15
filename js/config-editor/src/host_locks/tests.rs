//! The model's transitions (QA 38): each reset carries the ranges as
//! given, afresh; nothing a reset hands the editor becomes the given ones.
use config_editor_leptos::{TextPosition, TextRange};

use super::{Again, HostLocks, Listed};

const T: &str = "name: a\nsecret: keep\nlisten: 1\n";
const ABOVE: &str = "z: 0\nname: a\nsecret: keep\nlisten: 1\n";
const TWO_ABOVE: &str = "x: 1\ny: 2\nname: a\nsecret: keep\nlisten: 1\n";

fn line(number: usize) -> TextRange {
    TextRange::new(TextPosition::new(number, 1), TextPosition::new(number, 13))
}

/// H38-1, H38-2: resets in any order, and a text of the host's own after
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

/// FW38-1: the same ranges given again stay while no text of the host's
/// own came; after one (a reset, or ranges not taken), they are read anew.
#[test]
fn the_same_ranges_stay_until_a_text_of_the_hosts_own() {
    let mut locks = HostLocks::given(Some("[2]".into()), T, T, vec![line(2)]);
    assert_eq!(locks.again(Some("[2]")), Some(vec![line(2)]));
    assert_eq!(locks.again(Some("[3]")), None);
    locks.replaced(ABOVE);
    assert_eq!(locks.again(Some("[2]")), None);
    let mut locks = HostLocks::given(Some("[2]".into()), T, T, vec![line(2)]);
    locks.forget();
    assert_eq!(locks.again(Some("[2]")), None);
    // Unprintable ranges equal none.
    let locks = HostLocks::given(None, T, T, vec![line(2)]);
    assert_eq!(locks.again(None), None);
}

/// A list: ranges listed again stay (their record too), ranges read are
/// recorded with the text they came with, and a text from before a reset
/// keeps the record from before the list, its ranges not taken.
#[test]
fn a_list_records_what_its_editor_took() {
    let mut listed = Listed::default();
    let list = |listed: &mut Listed, print: &str, value: &str, replaces: bool, read: TextRange| {
        let result = listed
            .list::<()>(
                "m",
                Some(print.into()),
                value,
                || replaces,
                |_| Ok(vec![read]),
            )
            .expect("listed");
        listed.finish();
        result
    };
    let (ranges, again) = list(&mut listed, "[2]", T, true, line(2));
    assert!(again.is_none() && ranges == vec![line(2)]);
    listed.taken();
    // Given again with an echo: they stay.
    let (ranges, again) = list(&mut listed, "[2]", ABOVE, false, line(9));
    assert!(matches!(again, Some(Again::Stayed)) && ranges == vec![line(2)]);
    listed.took("m", T, None, true, false);
    listed.taken();
    // They came with the echo: the next ones may be worked out on it.
    assert_eq!(listed.get("m").map(HostLocks::came_with), Some(ABOVE));
    // Other ranges with an echo, followed to the document.
    let (_, again) = list(&mut listed, "[3]", ABOVE, false, line(3));
    assert!(again.is_none());
    listed.took("m", TWO_ABOVE, Some(vec![line(4)]), false, false);
    listed.taken();
    assert_eq!(
        listed.get_mut("m").map(|locks| locks.replaced(TWO_ABOVE)),
        Some(vec![line(4)])
    );
    // From before a reset: the record from before the list, not taken.
    let mut listed = Listed::default();
    list(&mut listed, "[2]", T, true, line(2));
    listed.taken();
    list(&mut listed, "[1]", ABOVE, false, line(1));
    listed.took("m", T, None, true, true);
    listed.taken();
    let locks = listed.get("m").expect("listed");
    assert_eq!(locks.again(Some("[2]")), None);
    assert_eq!(locks.came_with(), T);
}
