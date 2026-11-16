//! The byte-wise reading agrees with comparing the texts' UTF-16 units, on
//! random texts and edits (ASCII, 2- and 3-byte characters, characters
//! outside the BMP, line breaks).

use super::super::text::Utf16;
use super::{changed_range, edited_bytes, utf16_len};

/// [`edited_bytes`] with the caret given and the range read as UTF-16
/// units, to compare with the reference.
fn edit_range(before: &str, after: &str, caret: Utf16) -> (usize, usize) {
    let caret = caret.to_byte(after).expect("a caret between characters");
    let range = edited_bytes(before, after, caret);
    let units = |at| usize::from(Utf16::of_byte(before, at).expect("on a character boundary"));
    (units(range.start()), units(range.end()))
}

/// What the texts' UTF-16 units read (the reference).
mod units {
    pub(super) fn changed_range(before: &str, after: &str) -> (usize, usize) {
        let old: Vec<u16> = before.encode_utf16().collect();
        let new: Vec<u16> = after.encode_utf16().collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        (prefix, old.len() - suffix)
    }

    pub(super) fn edit_range(before: &str, after: &str, caret: usize) -> (usize, usize) {
        let (start, end) = changed_range(before, after);
        let old: Vec<u16> = before.encode_utf16().collect();
        let new: Vec<u16> = after.encode_utf16().collect();
        let removed = end - start;
        let Some(inserted) = (new.len() + removed).checked_sub(old.len()) else {
            return (start, end);
        };
        if let Some(from) = caret.checked_sub(inserted)
            && caret <= new.len()
            && from + removed <= old.len()
            && old[..from] == new[..from]
            && old[from + removed..] == new[caret..]
        {
            return (from, from + removed);
        }
        (start, end)
    }
}

struct Random(u64);

impl Random {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        ((self.0 >> 33) % bound as u64) as usize
    }

    /// A short text of a few kinds of characters (two outside the BMP whose
    /// surrogates all differ, so units and characters agree on where texts
    /// part).
    fn text(&mut self, max: usize) -> String {
        const CHARS: [char; 8] = ['a', 'b', '\n', ' ', 'é', '€', '😀', '𝄞'];
        (0..self.below(max + 1))
            .map(|_| CHARS[self.below(CHARS.len())])
            .collect()
    }
}

/// `text`'s UTF-16 offset of character `index`.
fn offset_of(text: &str, index: usize) -> usize {
    text.chars().take(index).map(char::len_utf16).sum()
}

#[test]
fn reads_what_the_utf16_units_read() {
    let mut random = Random(0x5eed);
    for _ in 0..20_000 {
        let before = random.text(12);
        let chars: Vec<char> = before.chars().collect();
        let from = random.below(chars.len() + 1);
        let to = from + random.below(chars.len() - from + 1);
        let inserted = random.text(4);
        let after: String = chars[..from]
            .iter()
            .chain(inserted.chars().collect::<Vec<_>>().iter())
            .chain(&chars[to..])
            .collect();
        let caret = offset_of(&before, from) + utf16_len(&inserted);
        assert_eq!(
            utf16_len(&before),
            before.encode_utf16().count(),
            "{before:?}"
        );
        assert_eq!(
            changed_range(&before, &after),
            units::changed_range(&before, &after),
            "{before:?} → {after:?}"
        );
        assert_eq!(
            edit_range(&before, &after, Utf16::count(caret)),
            units::edit_range(&before, &after, caret),
            "{before:?} → {after:?} at {caret}"
        );
    }
}

/// Long texts are compared a block of bytes at a time while they agree:
/// the same answers as unit by unit, wherever the edit falls against the
/// blocks (inside one, on a block's edge, a character split across one).
#[test]
fn reads_long_texts_as_the_utf16_units_do() {
    let mut random = Random(0x10_0000);
    for _ in 0..3_000 {
        let head = random.text(1_200);
        let tail = random.text(1_200);
        let removed = random.text(3);
        let inserted = random.text(3);
        let before = format!("{head}{removed}{tail}");
        let after = format!("{head}{inserted}{tail}");
        let caret = utf16_len(&head) + utf16_len(&inserted);
        assert_eq!(
            changed_range(&before, &after),
            units::changed_range(&before, &after),
            "{} + {removed:?} → {inserted:?}",
            head.len()
        );
        assert_eq!(
            edit_range(&before, &after, Utf16::count(caret)),
            units::edit_range(&before, &after, caret)
        );
    }
}
