//! Lines and columns of a text: where line `n` starts, which line and
//! column a byte is at. While [`with_spans`](super::with_spans) runs for a
//! text, its lines are split once and every answer is a lookup (thousands
//! of spans placed on a long document each counted the lines before them);
//! outside one, each answer reads the text up to it, as it always did.

use std::rc::Rc;

use super::super::work_count::Work;
use super::span_memo::{batch_cached_as, in_batch};

/// Where each line of a text starts (split on `\n`), and whether it is
/// ASCII (its columns are then its bytes).
pub(crate) struct Lines {
    starts: Vec<usize>,
    ascii: Vec<bool>,
}

impl Lines {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        let mut ascii = Vec::new();
        let mut plain = true;
        for (at, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(at + 1);
                ascii.push(plain);
                plain = true;
            } else if !byte.is_ascii() {
                plain = false;
            }
        }
        ascii.push(plain);
        Self { starts, ascii }
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
    let before = &source[..byte.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, before[line_start..].chars().count() + 1)
}

/// Where line `index` (from 0, split on `\n`) of `source` starts; the
/// text's length past its last line.
pub(crate) fn line_start(source: &str, index: usize) -> usize {
    if in_batch(source) {
        let lines = lines(source);
        return lines.starts.get(index).copied().unwrap_or(source.len());
    }
    source.split_inclusive('\n').take(index).map(str::len).sum()
}

/// Line `index` (from 0) of `source` without its `\n`, as
/// `source.split('\n').nth(index)` gives it.
pub(crate) fn split_line(source: &str, index: usize) -> Option<&str> {
    if in_batch(source) {
        return lines(source).line(source, index);
    }
    source.split('\n').nth(index)
}

#[cfg_attr(
    not(feature = "yaml"),
    allow(dead_code, reason = "the YAML reader uses it (and the tests)")
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
    source.lines().nth(index)
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::{line_column, line_start, split_line, text_line};

    /// Inside a batch, every answer is the one read from the top.
    #[test]
    fn batched_answers_are_the_ones_read_from_the_top() {
        let texts = ["", "a", "a\n", "a\r\nbé\n\nc", "\n\n", "é\nx\r\n"];
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
                (columns, lines)
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
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
