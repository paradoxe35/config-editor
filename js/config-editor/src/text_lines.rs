//! A host's text split into lines once, for the ranges given in it: each
//! range is checked against its lines and turned into lines and columns by
//! lookups (QA 21: each of 3000 ranges split the whole text again, and an
//! update froze for most of a second).

use config_editor_leptos::TextPosition;

/// Where each line of a text starts (split on `\n`), in bytes and in
/// UTF-16 units, and the text's length in UTF-16 units.
pub(crate) struct TextLines<'a> {
    text: &'a str,
    /// `(byte, UTF-16)` start of each line.
    starts: Vec<(usize, usize)>,
    units: usize,
}

impl<'a> TextLines<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let mut starts = vec![(0, 0)];
        let mut units = 0;
        for (byte, character) in text.char_indices() {
            units += character.len_utf16();
            if character == '\n' {
                starts.push((byte + 1, units));
            }
        }
        Self {
            text,
            starts,
            units,
        }
    }

    /// How many lines the text has (an empty one has one).
    pub(crate) fn count(&self) -> usize {
        self.starts.len()
    }

    /// The text's length in UTF-16 units.
    pub(crate) fn units(&self) -> usize {
        self.units
    }

    /// Line `line` (one-based) without its `\n`; `""` past the last.
    fn line(&self, line: usize) -> &'a str {
        let Some(&(start, _)) = self.starts.get(line.wrapping_sub(1)) else {
            return "";
        };
        let end = self
            .starts
            .get(line)
            .map_or(self.text.len(), |(next, _)| next - 1);
        &self.text[start..end]
    }

    /// The characters of line `line` (one-based); 0 past the last.
    pub(crate) fn width(&self, line: usize) -> usize {
        self.line(line).chars().count()
    }

    /// The UTF-16 offset of `position`, as [`TextPosition::to_offset`]
    /// gives it: a column past its line's end is the line's end, a line
    /// past the text the text's end.
    pub(crate) fn offset(&self, position: TextPosition) -> usize {
        let Some(&(_, start)) = self.starts.get(position.line.saturating_sub(1)) else {
            return self.units;
        };
        let within: usize = self
            .line(position.line.max(1))
            .chars()
            .take(position.column.saturating_sub(1))
            .map(char::len_utf16)
            .sum();
        (start + within).min(self.units)
    }

    /// The position of UTF-16 `offset`, as [`TextPosition::from_offset`]
    /// gives it.
    pub(crate) fn position(&self, offset: usize) -> TextPosition {
        let index = self.starts.partition_point(|(_, units)| *units <= offset) - 1;
        let mut units = self.starts[index].1;
        let mut column = 1;
        for character in self.line(index + 1).chars() {
            if units >= offset {
                break;
            }
            units += character.len_utf16();
            column += 1;
        }
        TextPosition::new(index + 1, column)
    }
}
