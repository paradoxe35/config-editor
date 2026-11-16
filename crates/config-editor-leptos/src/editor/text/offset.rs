//! The browser's offsets: UTF-16 code units, never mixed with bytes.

use text_size::TextSize;

/// An offset in UTF-16 code units (a textarea's selection end), as the
/// browser counts them.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the browser's offsets move onto it from `usize`")
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Utf16(pub(crate) usize);

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the browser's offsets move onto it from `usize`")
)]
impl Utf16 {
    /// The offset of byte `at` (a character boundary) of `text`.
    pub(crate) fn of_byte(text: &str, at: TextSize) -> Option<Self> {
        text.get(..usize::from(at))
            .map(|head| Utf16(head.encode_utf16().count()))
    }

    /// The byte offset of this offset in `text`, when it falls on a
    /// character boundary there.
    pub(crate) fn to_byte(self, text: &str) -> Option<TextSize> {
        let mut units = 0;
        for (at, character) in text.char_indices() {
            if units == self.0 {
                return TextSize::try_from(at).ok();
            }
            units += character.len_utf16();
            if units > self.0 {
                return None;
            }
        }
        (units == self.0)
            .then(|| TextSize::try_from(text.len()).ok())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_convert_to_bytes_and_back() {
        let text = "aé😀b";
        for (units, bytes) in [(0, 0), (1, 1), (2, 3), (4, 7), (5, 8)] {
            let at = TextSize::from(bytes);
            assert_eq!(Utf16::of_byte(text, at), Some(Utf16(units)));
            assert_eq!(Utf16(units).to_byte(text), Some(at));
        }
        // Inside the emoji's surrogate pair: no byte.
        assert_eq!(Utf16(3).to_byte(text), None);
    }
}
