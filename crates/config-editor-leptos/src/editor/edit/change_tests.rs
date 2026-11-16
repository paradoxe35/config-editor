//! The laws of a change set, on random texts and changes (seeded, so a
//! failure repeats): applying a change and its inverse gives the text back,
//! a composed change makes what the two make in turn, a change rebased
//! over another meets it (`a.compose(b.map(a))` makes what
//! `b.compose(a.map(b, true))` makes), and positions map in order.

use super::super::text::{TextSize, Utf16};
use super::change::{Assoc, Bytes, Change, CodeUnits, Moved};

/// A small seeded generator (xorshift).
struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % bound.max(1) as u64).unwrap_or(0)
    }
}

const PIECES: [&str; 8] = ["a", "bc", "\n", " ", "é", "😀", "xyz\n", ""];

fn text(rng: &mut Rng) -> String {
    (0..rng.below(12))
        .map(|_| PIECES[rng.below(PIECES.len())])
        .collect()
}

/// The character boundaries of `text`.
fn boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(at, _)| at)
        .chain([text.len()])
        .collect()
}

/// A random change of `text`: a few replacements, sorted and apart.
fn change(rng: &mut Rng, text: &str) -> Change<Bytes> {
    let bounds = boundaries(text);
    let mut cuts: Vec<usize> = (0..rng.below(5) * 2)
        .map(|_| bounds[rng.below(bounds.len())])
        .collect();
    cuts.sort_unstable();
    let edits: Vec<_> = cuts
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .map(|pair| {
            (
                pair[0]..pair[1],
                PIECES[rng.below(PIECES.len())].to_string(),
            )
        })
        .collect();
    // Two replacements at one place are one (the later one's text kept).
    let mut apart: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for (range, insert) in edits {
        match apart.last() {
            Some((last, _)) if last.end > range.start => {}
            _ => apart.push((range, insert)),
        }
    }
    Change::replacing(text.len(), apart).expect("sorted and apart")
}

fn seed() -> u64 {
    std::env::var("CHANGE_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(0x2545_f491_4f6c_dd1d)
}

/// Random cases per property: `CHANGE_CASES`, else `default` (the sweep
/// raises it).
fn cases(default: usize) -> usize {
    std::env::var("CHANGE_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(default)
}

#[test]
fn a_change_and_its_inverse_give_the_text_back() {
    let mut rng = Rng(seed());
    for _ in 0..cases(2000) {
        let before = text(&mut rng);
        let change = change(&mut rng, &before);
        let after = change.apply(&before).expect("made for it");
        assert_eq!(after.len(), change.len_after());
        let back = change.invert(&before).expect("inverts");
        assert_eq!(back.apply(&after).as_deref(), Some(before.as_str()));
    }
}

#[test]
fn a_composed_change_makes_what_the_two_make_in_turn() {
    let mut rng = Rng(seed() ^ 1);
    for _ in 0..cases(2000) {
        let before = text(&mut rng);
        let first = change(&mut rng, &before);
        let middle = first.apply(&before).expect("made for it");
        let second = change(&mut rng, &middle);
        let after = second.apply(&middle).expect("made for it");
        let both = first.compose(&second).expect("composes");
        assert_eq!(
            both.apply(&before),
            Some(after),
            "{before:?} {first:?} {second:?}"
        );
    }
}

#[test]
fn two_changes_rebased_over_each_other_meet() {
    let mut rng = Rng(seed() ^ 2);
    for _ in 0..cases(2000) {
        let text = text(&mut rng);
        let a = change(&mut rng, &text);
        let b = change(&mut rng, &text);
        let one = a
            .compose(&b.map(&a, false).expect("rebases"))
            .expect("composes");
        let other = b
            .compose(&a.map(&b, true).expect("rebases"))
            .expect("composes");
        assert_eq!(one.apply(&text), other.apply(&text), "{text:?} {a:?} {b:?}");
    }
}

#[test]
fn positions_map_in_order_and_kept_text_stays_on_its_text() {
    let mut rng = Rng(seed() ^ 3);
    for _ in 0..cases(2000) {
        let before = text(&mut rng);
        let change = change(&mut rng, &before);
        let after = change.apply(&before).expect("made for it");
        let bounds = boundaries(&before);
        let mapped: Vec<usize> = bounds
            .iter()
            .map(|at| change.map_pos(*at, Assoc::Before).pos)
            .collect();
        assert!(
            mapped.windows(2).all(|pair| pair[0] <= pair[1]),
            "{before:?} {change:?}"
        );
        for pair in bounds.windows(2) {
            let range = pair[0]..pair[1];
            if let Some(now) = change.map_range(&range) {
                assert_eq!(&after[now], &before[range], "{before:?} {change:?}");
            }
        }
    }
}

#[test]
fn text_inserted_at_a_range_goes_outside_it() {
    let change: Change<Bytes> =
        Change::replacing(6, [(2..2, "x".to_string()), (4..4, "y".to_string())]).unwrap();
    // `ab|cd|ef`: the range `cd` keeps out of both insertions.
    assert_eq!(change.map_range(&(2..4)), Some(3..5));
    assert_eq!(change.apply("abcdef").as_deref(), Some("abxcdyef"));
}

#[test]
fn a_moved_stretch_carries_its_positions() {
    // `a\nb\n` with the line `a\n` moved below `b\n`.
    let before = "a\nb\n";
    let change: Change<Bytes> =
        Change::replacing(4, [(0..2, String::new()), (4..4, "a\n".to_string())])
            .unwrap()
            .with_moved([Moved {
                from: 0,
                len: 1,
                to: 2,
            }]);
    let after = change.apply(before).unwrap();
    assert_eq!(after, "b\na\n");
    assert_eq!(change.map_range(&(0..1)), Some(2..3));
    let back = change.invert(before).unwrap();
    assert_eq!(back.apply(&after).as_deref(), Some(before));
    assert_eq!(back.map_range(&(2..3)), Some(0..1));
}

#[test]
fn a_change_counts_in_utf16_as_the_browser_does() {
    let before = "é😀ab";
    let change = Change::replacing_selection(before, "é😀xb", 6..7).unwrap();
    let units: Change<CodeUnits> = change.in_utf16(before).unwrap();
    // `é` is one unit, `😀` two.
    assert_eq!(units.edits().next().map(|(range, _)| range), Some(3..4));
    assert_eq!(units.map_range(&(4..5)), Some(4..5));
    let b = TextSize::from(7);
    assert_eq!(change.map_offset(b, Assoc::Before), b);
    assert_eq!(Utf16::of_byte(before, b), Some(Utf16(4)));
}

#[test]
fn a_narrowed_change_leaves_out_what_it_writes_back() {
    let before = "a: 1\nb: 2\n";
    let whole: Change<Bytes> = Change::replacing(
        before.len(),
        [(0..before.len(), "a: 1\nb: 3\n".to_string())],
    )
    .unwrap();
    let narrow = whole.narrowed(before).unwrap();
    assert_eq!(narrow.edits().next(), Some((8..9, "3")));
    // Written over itself: no change.
    let same: Change<Bytes> =
        Change::replacing(before.len(), [(0..4, "a: 1".to_string())]).unwrap();
    assert!(same.narrowed(before).unwrap().edits().next().is_none());
}

/// A change read off two texts makes the one from the other, and one
/// read line by line keeps each line's change on its line.
#[test]
fn a_change_read_off_two_texts_makes_the_second() {
    let mut rng = Rng(seed() ^ 4);
    for _ in 0..cases(2000) {
        let before = text(&mut rng);
        let after = change(&mut rng, &before)
            .apply(&before)
            .expect("made for it");
        assert_eq!(
            Change::diffed(&before, &after).apply(&before).as_deref(),
            Some(after.as_str())
        );
        if let Some(lines) = Change::line_by_line(&before, &after) {
            assert_eq!(lines.apply(&before).as_deref(), Some(after.as_str()));
        }
    }
    // Two alike lines commented: each line its own insertion, so a span
    // on the line between them is kept.
    let lines = Change::line_by_line("a\nb\na\n", "# a\nb\n# a\n").unwrap();
    assert_eq!(
        lines.edits().collect::<Vec<_>>(),
        vec![(0..0, "# "), (4..4, "# ")]
    );
    assert_eq!(lines.map_range(&(2..3)), Some(4..5));
}
