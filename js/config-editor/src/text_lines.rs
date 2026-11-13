//! A host's text split into lines once, for the ranges given in it: each
//! range is checked against its lines and turned into lines and columns by
//! lookups (QA 21: each of 3000 ranges split the whole text again, and an
//! update froze for most of a second). LF, CRLF and a lone CR each end a
//! line, as everywhere else in the API. A leading byte order mark is no
//! part of the document: string indices count it (they index the text as
//! given), lines and columns do not (line 1, column 1 is the character
//! after it, as in the editor).

use config_editor_leptos::TextPosition;

/// A byte order mark.
const MARK: char = '\u{feff}';

/// `text` without a leading byte order mark: the document, whose lines and
/// columns positions name.
pub(crate) fn document(text: &str) -> &str {
    text.strip_prefix(MARK).unwrap_or(text)
}

/// Where each line of a text starts and where its content ends (before its
/// break), and the text's length in UTF-16 units.
pub(crate) struct TextLines<'a> {
    text: &'a str,
    /// `(byte, UTF-16)` start of each line.
    starts: Vec<(usize, usize)>,
    /// The byte where each line's content ends.
    ends: Vec<usize>,
    /// The UTF-16 start of each line in the text's LF form (the text on
    /// screen in the host's view), where each break is one unit.
    lf_starts: Vec<usize>,
    units: usize,
}

impl<'a> TextLines<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        // Line 1 starts after a byte order mark (one UTF-16 unit).
        let mark = usize::from(text.starts_with(MARK));
        let mut starts = vec![(mark * MARK.len_utf8(), mark)];
        let mut ends = Vec::new();
        let mut lf_starts = vec![0];
        let mut units = mark;
        // Units the LF form on screen has not: a mark, each CRLF's CR.
        let mut dropped = mark;
        let mut characters = text.char_indices().skip(mark).peekable();
        while let Some((byte, character)) = characters.next() {
            units += character.len_utf16();
            let ending = match character {
                '\r' if characters.next_if(|(_, next)| *next == '\n').is_some() => {
                    units += 1;
                    dropped += 1;
                    2
                }
                '\r' | '\n' => 1,
                _ => continue,
            };
            ends.push(byte);
            starts.push((byte + ending, units));
            lf_starts.push(units - dropped);
        }
        ends.push(text.len());
        Self {
            text,
            starts,
            ends,
            lf_starts,
            units,
        }
    }

    /// The position of UTF-16 `offset` in the text's LF form (each break
    /// one unit, as the editor's own `rangeOf` counts in the host's view);
    /// `None` past its end or inside a character.
    pub(crate) fn position_in_lf(&self, offset: usize) -> Option<TextPosition> {
        let index = self.lf_starts.partition_point(|start| *start <= offset) - 1;
        let mut units = self.lf_starts[index];
        let mut column = 1;
        let line = self.line(index + 1);
        for character in line.chars() {
            if units >= offset {
                break;
            }
            units += character.len_utf16();
            column += 1;
        }
        (units == offset).then(|| TextPosition::new(index + 1, column))
    }

    /// How many lines the text has (an empty one has one).
    pub(crate) fn count(&self) -> usize {
        self.starts.len()
    }

    /// The text's length in UTF-16 units.
    pub(crate) fn units(&self) -> usize {
        self.units
    }

    /// Line `line` (one-based) without its break; `""` past the last.
    fn line(&self, line: usize) -> &'a str {
        let index = line.wrapping_sub(1);
        match (self.starts.get(index), self.ends.get(index)) {
            (Some(&(start, _)), Some(&end)) => &self.text[start..end],
            _ => "",
        }
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
        // Before a byte order mark: where the document starts.
        let offset = offset.max(self.starts[0].1);
        let index = self.starts.partition_point(|(_, units)| *units <= offset) - 1;
        let mut units = self.starts[index].1;
        let mut column = 1;
        // With its break: an offset between a CRLF's CR and LF is past the
        // CR, as `from_offset` counts it.
        let end = self
            .starts
            .get(index + 1)
            .map_or(self.text.len(), |(next, _)| *next);
        for character in self.text[self.starts[index].0..end].chars() {
            if units >= offset {
                break;
            }
            units += character.len_utf16();
            column += 1;
        }
        TextPosition::new(index + 1, column)
    }
}

#[cfg(test)]
mod tests {
    use config_editor_leptos::TextPosition;

    use super::{TextLines, document};

    /// Every answer is the one `TextPosition` gives reading the text from
    /// the top, whichever breaks end its lines.
    #[test]
    fn lines_end_at_lf_crlf_and_lone_cr_as_text_positions_count_them() {
        let texts = [
            "",
            "a",
            "a\n",
            "a\rb",
            "a\r\nb\rc\nd",
            "\r\r\n\n",
            "é😀\rx\r\n",
            "a\r",
        ];
        let mut failures = Vec::new();
        for text in texts {
            let lines = TextLines::new(text);
            let units = text.encode_utf16().count();
            if lines.units() != units {
                failures.push(format!("{text:?}: {} units", lines.units()));
            }
            let end = TextPosition::from_offset(text, units);
            if lines.count() != end.line {
                failures.push(format!(
                    "{text:?}: {} lines, not {}",
                    lines.count(),
                    end.line
                ));
            }
            for offset in 0..=units {
                let expected = TextPosition::from_offset(text, offset);
                let position = lines.position(offset);
                if position != expected {
                    failures.push(format!(
                        "{text:?} @{offset}: {position:?}, not {expected:?}"
                    ));
                }
            }
            // The LF form's offsets: what the editor's `rangeOf` gives.
            let lf = text.replace("\r\n", "\n").replace('\r', "\n");
            let lf_units: Vec<u16> = lf.encode_utf16().collect();
            for offset in 0..=lf_units.len() + 1 {
                let inside = lf_units
                    .get(offset)
                    .is_some_and(|unit| (0xDC00..0xE000).contains(unit));
                let expected = (offset <= lf_units.len() && !inside)
                    .then(|| TextPosition::from_offset(&lf, offset));
                if lines.position_in_lf(offset) != expected {
                    failures.push(format!(
                        "{text:?} LF @{offset}: {:?}, not {expected:?}",
                        lines.position_in_lf(offset)
                    ));
                }
            }
            for line in 1..=lines.count() + 1 {
                let width = text
                    .split("\r\n")
                    .flat_map(|part| part.split(['\r', '\n']))
                    .nth(line - 1)
                    .map_or(0, |content| content.chars().count());
                if lines.width(line) != width {
                    failures.push(format!("{text:?} line {line}: width {}", lines.width(line)));
                }
                for column in 1..=6 {
                    let position = TextPosition::new(line, column);
                    let (offset, expected) = (lines.offset(position), position.to_offset(text));
                    if offset != expected {
                        failures.push(format!(
                            "{text:?} {line}:{column}: offset {offset}, not {expected}"
                        ));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// A byte order mark is no column (line 1, column 1 is the character
    /// after it, as in the editor) but a string index (`L30-4`: a host's
    /// `{ start, end }` in its own text landed one character right).
    #[test]
    fn a_byte_order_mark_is_an_index_but_no_column() {
        for text in ["", "port: x\n", "a: 1\r\nport: x\r\n", "é😀\rx"] {
            let marked = format!("\u{feff}{text}");
            assert_eq!(document(&marked), text);
            let (plain, lines) = (TextLines::new(text), TextLines::new(&marked));
            assert_eq!(lines.units(), plain.units() + 1, "{text:?}");
            assert_eq!(lines.count(), plain.count(), "{text:?}");
            assert_eq!(lines.position(0), TextPosition::new(1, 1), "{text:?}");
            for offset in 0..=plain.units() {
                assert_eq!(
                    lines.position(offset + 1),
                    plain.position(offset),
                    "{text:?} @{offset}"
                );
                assert_eq!(
                    lines.position_in_lf(offset),
                    plain.position_in_lf(offset),
                    "{text:?} LF @{offset}"
                );
            }
            for line in 1..=plain.count() + 1 {
                assert_eq!(lines.width(line), plain.width(line), "{text:?} line {line}");
                for column in 1..=6 {
                    let position = TextPosition::new(line, column);
                    assert_eq!(
                        lines.offset(position),
                        plain.offset(position) + 1,
                        "{text:?} {position:?}"
                    );
                }
            }
        }
    }
}
