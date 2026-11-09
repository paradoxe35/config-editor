//! Lines and columns of a text: where line `n` starts, which line and
//! column a byte is at. While [`with_spans`](super::with_spans) runs for a
//! text, its lines are split once and every answer is a lookup (thousands
//! of spans placed on a long document each counted the lines before them);
//! outside one, each answer reads the text up to it, as it always did.

use std::rc::Rc;

use super::super::work_count::{Work, note};
use super::TextPosition;
use super::span_memo::{batch_cached_as, in_batch};

/// Where each line of a text starts (split on `\n`), and whether it is
/// ASCII (its columns are then its bytes).
pub(crate) struct Lines {
    starts: Vec<usize>,
    ascii: Vec<bool>,
    /// The UTF-16 offset each line starts at.
    units: Vec<usize>,
}

impl Lines {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        let mut ascii = Vec::new();
        let mut units = vec![0];
        let mut plain = true;
        let mut unit = 0;
        for (at, character) in source.char_indices() {
            unit += character.len_utf16();
            if character == '\n' {
                starts.push(at + 1);
                units.push(unit);
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

    /// The UTF-16 offset of `byte` (a character boundary), as
    /// `source[..byte].encode_utf16().count()` gives it.
    fn units_at(&self, source: &str, byte: usize) -> usize {
        let byte = byte.min(source.len());
        let index = self.starts.partition_point(|start| *start <= byte) - 1;
        let start = self.starts[index];
        let within = if self.ascii[index] {
            byte - start
        } else {
            source[start..byte].encode_utf16().count()
        };
        self.units[index] + within
    }

    /// The byte at UTF-16 `offset`, as `utf16_to_byte_index` gives it.
    fn byte_at(&self, source: &str, offset: usize) -> usize {
        let index = self.units.partition_point(|unit| *unit <= offset) - 1;
        let (start, mut unit) = (self.starts[index], self.units[index]);
        if self.ascii[index] {
            let end = self
                .starts
                .get(index + 1)
                .map_or(source.len(), |next| next - 1);
            if offset - unit <= end - start {
                return start + offset - unit;
            }
        }
        for (at, character) in source[start..].char_indices() {
            if unit >= offset {
                return start + at;
            }
            unit += character.len_utf16();
        }
        source.len()
    }

    /// The (one-based) line and column of `byte`.
    fn line_column(&self, source: &str, byte: usize) -> (usize, usize) {
        let byte = byte.min(source.len());
        let index = self.starts.partition_point(|start| *start <= byte) - 1;
        let start = self.starts[index];
        let column = if self.ascii[index] {
            byte - start
        } else {
            source[start..byte].chars().count()
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
        let end = self
            .starts
            .get(index + 1)
            .map_or(source.len(), |next| next - 1);
        Some(&source[start..end])
    }
}

/// The lines of `source`: split once per batch (each time outside one).
pub(crate) fn lines(source: &str) -> Rc<Lines> {
    batch_cached_as(Work::LineSplit, source, || Lines::new(source))
}

/// The (one-based) line and column of byte `byte` of `source`.
pub(crate) fn line_column(source: &str, byte: usize) -> (usize, usize) {
    if in_batch(source) {
        return lines(source).line_column(source, byte);
    }
    note(Work::LineSeek);
    let before = &source[..byte.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, before[line_start..].chars().count() + 1)
}

/// The UTF-16 offset of byte `byte` of `source` (a character boundary):
/// `source[..byte].encode_utf16().count()`, a lookup while a batch runs
/// for the text.
pub(crate) fn units_at(source: &str, byte: usize) -> usize {
    if in_batch(source) {
        return lines(source).units_at(source, byte);
    }
    note(Work::LineSeek);
    source[..byte.min(source.len())].encode_utf16().count()
}

/// The byte at UTF-16 `offset` of `source` (see `utf16_to_byte_index`), a
/// lookup while a batch runs for the text.
pub(crate) fn byte_at(source: &str, offset: usize) -> Option<usize> {
    in_batch(source).then(|| lines(source).byte_at(source, offset))
}

/// `position.to_offset(source)`, from the lines read once for the text
/// while a batch runs for it, when only `\n` ends them
/// (`line_breaks_only`): one pass up to it each time otherwise.
pub(crate) fn offset_of(source: &str, position: TextPosition, line_breaks_only: bool) -> usize {
    let index = position.line.wrapping_sub(1);
    match split_line(source, index).filter(|_| line_breaks_only && position.line > 0) {
        Some(line) => {
            let column = line
                .char_indices()
                .nth(position.column.saturating_sub(1))
                .map_or(line.len(), |(at, _)| at);
            units_at(source, line_start(source, index) + column)
        }
        None => position.to_offset(source),
    }
}

/// Where line `index` (from 0, split on `\n`) of `source` starts; the
/// text's length past its last line.
pub(crate) fn line_start(source: &str, index: usize) -> usize {
    if in_batch(source) {
        let lines = lines(source);
        return lines.starts.get(index).copied().unwrap_or(source.len());
    }
    note(Work::LineSeek);
    source.split_inclusive('\n').take(index).map(str::len).sum()
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

#[cfg_attr(
    not(any(feature = "yaml", feature = "toml")),
    allow(dead_code, reason = "the YAML and TOML readers use it (and the tests)")
)]
/// Line `index` (from 0) of `source` as `source.lines().nth(index)` gives
/// it: without a `\r` before its `\n`, and no empty line after a final
/// line break.
pub(crate) fn text_line(source: &str, index: usize) -> Option<&str> {
    if in_batch(source) {
        let line = split_line(source, index)?;
        // The empty text after a final line break (or of an empty text).
        if line.is_empty() && line_start(source, index) == source.len() {
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
    use super::{line_column, line_start, offset_of, split_line, text_line, units_at};

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
                    .map(|at| line_column(text, at))
                    .collect();
                let lines: Vec<_> = (0..6)
                    .map(|index| {
                        (
                            line_start(text, index).min(text.len()),
                            split_line(text, index).map(str::to_string),
                            text_line(text, index).map(str::to_string),
                        )
                    })
                    .collect();
                // UTF-16 offsets both ways, and positions as offsets.
                let units: Vec<_> = (0..=text.len())
                    .filter(|at| text.is_char_boundary(*at))
                    .map(|at| units_at(text, at))
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
            let read_off: Vec<usize> = positions
                .clone()
                .map(|position| position.to_offset(text))
                .collect();
            if batched.4 != read_off {
                failures.push(format!("{text:?}: offsets {:?} vs {read_off:?}", batched.4));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
