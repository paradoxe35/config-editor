//! The byte-wise reading agrees with comparing the texts' UTF-16 units, on
//! random texts and edits (ASCII, 2- and 3-byte characters, characters
//! outside the BMP, line breaks).

use super::{changed_range, edit_range, map_through_change, replaces, utf16_len};

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

    pub(super) fn replaces(before: &str, after: &str, (start, end): (usize, usize)) -> bool {
        let old: Vec<u16> = before.encode_utf16().collect();
        let new: Vec<u16> = after.encode_utf16().collect();
        let kept = old.len().checked_sub(end).filter(|_| start <= end);
        kept.is_some_and(|kept| {
            new.len() >= start + kept
                && old[..start] == new[..start]
                && old[end..] == new[new.len() - kept..]
        })
    }

    pub(super) fn map_through_change(old: &str, new: &str, offset: usize) -> usize {
        let (start, end) = changed_range(old, new);
        let (old_len, new_len) = (old.encode_utf16().count(), new.encode_utf16().count());
        if offset <= start {
            offset
        } else if offset >= end {
            offset + new_len - old_len
        } else {
            start
        }
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
            edit_range(&before, &after, caret),
            units::edit_range(&before, &after, caret),
            "{before:?} → {after:?} at {caret}"
        );
        let selection = (offset_of(&before, from), offset_of(&before, to));
        let guess = (
            offset_of(&before, random.below(chars.len() + 1)),
            offset_of(&before, random.below(chars.len() + 1)),
        );
        for range in [selection, guess] {
            assert_eq!(
                replaces(&before, &after, range),
                units::replaces(&before, &after, range),
                "{before:?} → {after:?} replacing {range:?}"
            );
        }
        let offset = offset_of(&before, random.below(chars.len() + 1));
        assert_eq!(
            map_through_change(&before, &after, offset),
            units::map_through_change(&before, &after, offset),
            "{before:?} → {after:?} at {offset}"
        );
    }
}
