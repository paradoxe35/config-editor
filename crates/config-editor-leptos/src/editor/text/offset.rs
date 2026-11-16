//! The browser's offsets: UTF-16 code units, never mixed with bytes.

use std::ops::{Add, AddAssign, Sub, SubAssign};

use text_size::{TextRange, TextSize};

/// An offset (or a length) in UTF-16 code units, as the browser counts
/// them: a textarea's selection, a JS string index.
///
/// Held as a `u32`, as the DOM's selection offsets are: a JS string is at
/// most 2^30 code units long, and on wasm32 no text is longer than 4 GiB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Utf16(u32);

impl Utf16 {
    /// The offset `units` code units in.
    pub(crate) const fn new(units: u32) -> Self {
        Self(units)
    }

    /// The length of `text` in code units.
    pub(crate) fn of(text: &str) -> Self {
        Self::count(text.encode_utf16().count())
    }

    /// A count of code units made by the editor's own code (the length of
    /// a text it holds).
    pub(crate) fn count(units: usize) -> Self {
        Self(u32::try_from(units).expect("a text holds under 4 GiB: wasm32 addresses no more"))
    }

    /// The offset of byte `at` (a character boundary) of `text`.
    pub(crate) fn of_byte(text: &str, at: TextSize) -> Option<Self> {
        text.get(..usize::from(at)).map(Self::of)
    }

    /// The byte offset of this offset in `text`, when it falls on a
    /// character boundary there.
    #[cfg(test)]
    pub(crate) fn to_byte(self, text: &str) -> Option<TextSize> {
        let mut units = 0;
        for (at, character) in text.char_indices() {
            if units == self.0 {
                return TextSize::try_from(at).ok();
            }
            units += u32::try_from(character.len_utf16()).ok()?;
            if units > self.0 {
                return None;
            }
        }
        (units == self.0)
            .then(|| TextSize::try_from(text.len()).ok())
            .flatten()
    }
}

impl From<Utf16> for u32 {
    fn from(units: Utf16) -> Self {
        units.0
    }
}

impl From<Utf16> for usize {
    fn from(units: Utf16) -> Self {
        // wasm32 and the native targets the crate builds for have a `usize`
        // of at least 32 bits.
        units.0 as usize
    }
}

impl Add for Utf16 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Utf16 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl AddAssign for Utf16 {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl SubAssign for Utf16 {
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

/// The byte length of `text` (or a byte count of it) as an offset: no text
/// is longer than 4 GiB, which is all wasm32 addresses.
pub(crate) fn text_size(len: usize) -> TextSize {
    TextSize::try_from(len).expect("a text holds under 4 GiB: wasm32 addresses no more")
}

/// A byte range of a text given as `usize` (a `str` method's or another
/// crate's), as a [`TextRange`]; an end before the start is the start.
pub(crate) fn byte_range(range: std::ops::Range<usize>) -> TextRange {
    let start = text_size(range.start);
    TextRange::new(start, text_size(range.end).max(start))
}

/// The UTF-16 offset of byte `at` of `text`, as a `usize`: for the code
/// that still counts the text in UTF-16 units (the public language API's
/// carets and ranges, and the areas that pass them on).
pub(crate) fn utf16_of(text: &str, at: TextSize) -> usize {
    let at = at.min(TextSize::of(text));
    usize::from(super::super::paths::units_at(text, at))
}

/// The byte of `text` at UTF-16 offset `units` (a `usize` from code that
/// counts in UTF-16, see [`utf16_of`]); one inside a character is its end.
pub(crate) fn byte_of(text: &str, units: usize) -> TextSize {
    text_size(super::super::keyboard::utf16_to_byte_index(text, units))
}

/// A byte range given from outside (a custom language's edits), `None`
/// when it ends before it starts or past what an offset holds.
pub(crate) fn checked_range(range: &std::ops::Range<usize>) -> Option<TextRange> {
    let (start, end) = (
        TextSize::try_from(range.start).ok()?,
        TextSize::try_from(range.end).ok()?,
    );
    (start <= end).then(|| TextRange::new(start, end))
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
