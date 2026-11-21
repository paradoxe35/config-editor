//! Lines and columns of a text: where line `n` starts, which line and
//! column a byte is at, and the UTF-16 offset of a byte and back (the one
//! conversion between the browser's offsets and the text's). While
//! [`with_spans`](super::with_spans) runs for a text, its lines are split
//! once and every answer is a lookup (thousands of spans placed on a long
//! document each counted the lines before them); outside one, each answer
//! reads the text up to it.

use std::rc::Rc;

use super::super::text::{TextRange, TextSize, Utf16, slice, text_size};
use super::super::work_count::{Work, note};
use super::TextPosition;
use super::span_memo::{batch_cached_as, in_batch};

/// Where each line of a text starts (split on `\n`), and whether it is
/// ASCII (its columns are then its bytes).
pub(crate) struct Lines {
    starts: Vec<TextSize>,
    ascii: Vec<bool>,
    /// The UTF-16 offset each line starts at.
    units: Vec<Utf16>,
}

impl Lines {
    fn new(source: &str) -> Self {
        let mut starts = vec![TextSize::default()];
        let mut ascii = Vec::new();
        let mut units = vec![Utf16::default()];
        let mut plain = true;
        let mut unit = 0;
        for (at, character) in source.char_indices() {
            unit += character.len_utf16();
            if character == '\n' {
                starts.push(text_size(at + 1));
                units.push(Utf16::count(unit));
                ascii.push(plain);
                plain = true;
            } else if !character.is_ascii() {
                plain = false;
            }
        }
        ascii.push(plain);
        Self {
            starts,
            ascii,
            units,
        }
    }

    /// The end of line `index` (before its `\n`), the text's end for the
    /// last.
    fn end(&self, source: &str, index: usize) -> TextSize {
        self.starts
            .get(index + 1)
            .map_or(TextSize::of(source), |next| *next - TextSize::from(1))
    }

    /// The UTF-16 offset of `byte`, as [`units_at`] reads it from the top.
    fn units_at(&self, source: &str, byte: TextSize) -> Utf16 {
        let byte = byte.min(TextSize::of(source));
        let index = self.starts.partition_point(|start| *start <= byte) - 1;
        let start = self.starts[index];
        let within = if self.ascii[index] {
            Utf16::new(u32::from(byte - start))
        } else {
            Utf16::of(slice::within(source, TextRange::new(start, byte)))
        };
        self.units[index] + within
    }

    /// The byte at UTF-16 `offset`, as `utf16_to_byte_index` gives it.
    fn byte_at(&self, source: &str, offset: Utf16) -> TextSize {
        let index = self.units.partition_point(|unit| *unit <= offset) - 1;
        let (start, mut unit) = (self.starts[index], self.units[index]);
        if self.ascii[index] {
            let within = u32::from(offset - unit);
            if within <= u32::from(self.end(source, index) - start) {
                return start + TextSize::from(within);
            }
        }
        let rest = slice::after(source, start).unwrap_or_default();
        for (at, character) in rest.char_indices() {
            if unit >= offset {
                return start + text_size(at);
            }
            unit += Utf16::count(character.len_utf16());
        }
        TextSize::of(source)
    }

    /// The (one-based) line and column of `byte`.
    pub(crate) fn line_column(&self, source: &str, byte: TextSize) -> (usize, usize) {
        let byte = byte.min(TextSize::of(source));
        let index = self.starts.partition_point(|start| *start <= byte) - 1;
        let start = self.starts[index];
        let column = if self.ascii[index] {
            usize::from(byte - start)
        } else {
            slice::within(source, TextRange::new(start, byte))
                .chars()
                .count()
        };
        (index + 1, column + 1)
    }

    /// How many lines there are (an empty text has one).
    pub(crate) fn count(&self) -> usize {
        self.starts.len()
    }

    /// Line `index` (from 0) of `source` (the text split) without its `\n`.
    pub(crate) fn line<'a>(&self, source: &'a str, index: usize) -> Option<&'a str> {
        let start = *self.starts.get(index)?;
        slice::slice(source, TextRange::new(start, self.end(source, index)))
    }
}

/// The lines of `source`: split once per batch (each time outside one).
pub(crate) fn lines(source: &str) -> Rc<Lines> {
    batch_cached_as(Work::LineSplit, source, || Lines::new(source))
}

/// The (one-based) line and column of byte `byte` of `source`.
pub(crate) fn line_column(source: &str, byte: TextSize) -> (usize, usize) {
    if in_batch(source) {
        return lines(source).line_column(source, byte);
    }
    note(Work::LineSeek);
    let before = slice::up_to(source, byte);
    let line = before.matches('\n').count() + 1;
    let on_line = before.rsplit('\n').next().unwrap_or_default();
    (line, on_line.chars().count() + 1)
}

/// The line (from 0, split on `\n`) holding byte `byte` of `source`: a
/// lookup while a batch runs for the text.
pub(crate) fn line_of_byte(source: &str, byte: TextSize) -> usize {
    let byte = byte.min(TextSize::of(source));
    if in_batch(source) {
        let lines = lines(source);
        return lines.starts.partition_point(|start| *start <= byte) - 1;
    }
    note(Work::LineSeek);
    slice::up_to(source, byte).matches('\n').count()
}

/// The UTF-16 offset of byte `byte` of `source`: the units before it, a
/// lookup while a batch runs for the text. A byte inside a character
/// counts from the character's start, one past the end from the end.
pub(crate) fn units_at(source: &str, byte: TextSize) -> Utf16 {
    if in_batch(source) {
        return lines(source).units_at(source, byte);
    }
    note(Work::LineSeek);
    Utf16::of(slice::up_to(source, byte))
}

/// [`units_at`] where two outside edges meet: `byte` from a `str` method
/// and the answer for the public language API's UTF-16 offsets
/// ([`CompletionContext`](crate::language::CompletionContext) and the
/// like), both `usize` there.
pub(crate) fn api_units(source: &str, byte: usize) -> usize {
    usize::from(units_at(source, text_size(byte.min(source.len()))))
}

/// The byte at UTF-16 `offset` of `source` (see `utf16_to_byte_index`), a
/// lookup while a batch runs for the text.
pub(crate) fn byte_at(source: &str, offset: Utf16) -> Option<TextSize> {
    in_batch(source).then(|| lines(source).byte_at(source, offset))
}

/// The place of `range` of `source` as its line and its first and end
/// (one-based, in characters) columns on it.
#[cfg(any(feature = "json", feature = "toml", feature = "hcl", feature = "xml"))]
pub(crate) fn span_between(source: &str, range: TextRange) -> (usize, usize, usize) {
    let (line, column) = line_column(source, range.start());
    let end = range.end().min(TextSize::of(source));
    (
        line,
        column,
        column
            + slice::within(
                source,
                TextRange::new(range.start(), end.max(range.start())),
            )
            .chars()
            .count(),
    )
}

/// The first line of `source` with text, from its first character to its
/// end (one-based columns, in characters).
#[cfg(any(feature = "json", feature = "toml", feature = "hcl"))]
pub(crate) fn first_line_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let indent = line
            .chars()
            .take_while(|character| character.is_whitespace())
            .count();
        // A line of spaces alone has no text.
        line.chars().nth(indent)?;
        Some((index + 1, indent + 1, line.chars().count() + 1))
    })
}

/// The UTF-16 length of `text`.
pub(crate) fn utf16_len(text: &str) -> usize {
    // One unit per character, two for one outside the BMP (a 4-byte one).
    text.bytes()
        .map(|byte| usize::from(byte & 0xC0 != 0x80) + usize::from(byte >= 0xF0))
        .sum()
}

/// The byte offset of character `column` (from 0) of `line`; its length
/// past its end.
#[cfg(any(feature = "toml", feature = "hcl"))]
pub(crate) fn byte_at_column(line: &str, column: usize) -> TextSize {
    text_size(
        line.char_indices()
            .nth(column)
            .map_or(line.len(), |(index, _)| index),
    )
}

/// `position.to_offset(source)`, from the lines read once for the text
/// while a batch runs for it, when only `\n` ends them
/// (`line_breaks_only`): one pass up to it each time otherwise.
pub(crate) fn offset_of(source: &str, position: TextPosition, line_breaks_only: bool) -> Utf16 {
    let index = position.line.wrapping_sub(1);
    match split_line(source, index).filter(|_| line_breaks_only && position.line > 0) {
        Some(line) => {
            let column = line
                .char_indices()
                .nth(position.column.saturating_sub(1))
                .map_or(line.len(), |(at, _)| at);
            units_at(source, line_start(source, index) + text_size(column))
        }
        None => Utf16::count(position.to_offset(source)),
    }
}

/// Where line `index` (from 0, split on `\n`) of `source` starts; the
/// text's length past its last line.
pub(crate) fn line_start(source: &str, index: usize) -> TextSize {
    if in_batch(source) {
        let lines = lines(source);
        return lines
            .starts
            .get(index)
            .copied()
            .unwrap_or(TextSize::of(source));
    }
    note(Work::LineSeek);
    text_size(source.split_inclusive('\n').take(index).map(str::len).sum())
}

/// Line `index` (from 0) of `source` without its `\n`, as
/// `source.split('\n').nth(index)` gives it.
pub(crate) fn split_line(source: &str, index: usize) -> Option<&str> {
    if in_batch(source) {
        return lines(source).line(source, index);
    }
    note(Work::LineSeek);
    source.split('\n').nth(index)
}

/// Line `index` (from 0) of `source` as `source.lines().nth(index)` gives
/// it: without a `\r` before its `\n`, and no empty line after a final
/// line break.
#[cfg(any(feature = "yaml", feature = "toml"))]
pub(crate) fn text_line(source: &str, index: usize) -> Option<&str> {
    if in_batch(source) {
        let line = split_line(source, index)?;
        // The empty text after a final line break (or of an empty text).
        if line.is_empty() && line_start(source, index) == TextSize::of(source) {
            return None;
        }
        return Some(line.strip_suffix('\r').unwrap_or(line));
    }
    note(Work::LineSeek);
    source.lines().nth(index)
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::super::super::keyboard::utf16_to_byte_index;
    use super::super::TextPosition;
    use super::{
        TextSize, Utf16, line_column, line_start, offset_of, split_line, text_line, text_size,
        units_at,
    };

    /// Inside a batch, every answer is the one read from the top.
    #[test]
    fn batched_answers_are_the_ones_read_from_the_top() {
        let texts = [
            "",
            "a",
            "a\n",
            "a\r\nbé\n\nc",
            "\n\n",
            "é\nx\r\n",
            "a😀b\n😀\nxyz\n",
        ];
        let mut failures = Vec::new();
        for text in texts {
            let read = |text: &str| {
                let columns: Vec<_> = (0..=text.len())
                    .filter(|at| text.is_char_boundary(*at))
                    .map(|at| line_column(text, text_size(at)))
                    .collect();
                let lines: Vec<_> = (0..6)
                    .map(|index| {
                        (
                            line_start(text, index).min(TextSize::of(text)),
                            split_line(text, index).map(str::to_string),
                            text_line(text, index).map(str::to_string),
                        )
                    })
                    .collect();
                // UTF-16 offsets both ways, and positions as offsets.
                let units: Vec<_> = (0..=text.len())
                    .filter(|at| text.is_char_boundary(*at))
                    .map(|at| units_at(text, text_size(at)))
                    .collect();
                let bytes: Vec<_> = (0..text.encode_utf16().count() + 3)
                    .map(|unit| utf16_to_byte_index(text, unit))
                    .collect();
                let only_lf = !text.contains('\r');
                let offsets: Vec<_> = (0..6)
                    .flat_map(|line| (0..8).map(move |column| TextPosition::new(line, column)))
                    .map(|position| offset_of(text, position, only_lf))
                    .collect();
                (columns, lines, units, bytes, offsets)
            };
            let alone = read(text);
            let batched = super::super::with_spans(
                super::super::super::Language::YAML.behavior(),
                text,
                &[],
                || read(text),
            );
            if alone != batched {
                failures.push(format!("{text:?}: {alone:?} vs {batched:?}"));
            }
            // A position's offset is `to_offset`'s.
            let positions =
                (0..6).flat_map(|line| (0..8).map(move |column| TextPosition::new(line, column)));
            let read_off: Vec<Utf16> = positions
                .clone()
                .map(|position| Utf16::count(position.to_offset(text)))
                .collect();
            if batched.4 != read_off {
                failures.push(format!("{text:?}: offsets {:?} vs {read_off:?}", batched.4));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
