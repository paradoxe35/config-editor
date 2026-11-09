//! A TOML text's lines and the table each line is in, as the caret's
//! context reads them: split from the top each time, or, while a batch
//! runs for the text (see `paths::paths_at`), split once and the tables
//! walked once, kept every few lines, so the context at any line is a
//! lookup and a short walk.

use std::rc::Rc;

use super::super::super::paths::{Checkpoints, batch_cached, in_batch, walked_from_top};
use super::parse::{Tables, parse_header, split_toml_comment};

/// Where each line starts and ends.
type Ranges = Vec<(usize, usize)>;

/// A text's lines, split as `str::lines` (`by_breaks: false`) or
/// `split('\n')` splits them.
pub(super) struct TextLines<'a> {
    source: &'a str,
    ranges: Rc<Ranges>,
    kept: Option<Rc<Kept>>,
}

/// The tables walked once over a text's lines, and where `line_at` puts
/// each line.
struct Kept {
    tables: Checkpoints<Tables>,
    /// Each line's offset as `line_at` counts it (its length and one).
    offsets: Vec<usize>,
}

/// The lines of a text, split one way: a type of its own for each, so a
/// batch keeps both.
struct ByLines(Rc<Ranges>, Rc<Kept>);
struct ByBreaks(Rc<Ranges>, Rc<Kept>);

fn ranges(source: &str, by_breaks: bool) -> Ranges {
    let base = source.as_ptr() as usize;
    let at = |line: &str| line.as_ptr() as usize - base;
    let lines: Box<dyn Iterator<Item = &str>> = if by_breaks {
        Box::new(source.split('\n'))
    } else {
        Box::new(source.lines())
    };
    lines.map(|line| (at(line), at(line) + line.len())).collect()
}

fn kept(source: &str, ranges: &Ranges) -> Kept {
    walked_from_top(source);
    let tables = Checkpoints::walk(Tables::default(), ranges.len(), |tables, index| {
        let (start, end) = ranges[index];
        enter_line(tables, &source[start..end]);
    });
    let mut offsets = Vec::with_capacity(ranges.len());
    let mut offset = 0;
    for (start, end) in ranges {
        offsets.push(offset);
        offset += end - start + 1;
    }
    Kept { tables, offsets }
}

/// The tables past `line`: a header enters its table.
fn enter_line(tables: &mut Tables, line: &str) {
    let (code, _) = split_toml_comment(line);
    if let Some((segments, is_array)) = parse_header(code) {
        tables.enter(&segments, is_array);
    }
}

impl<'a> TextLines<'a> {
    /// The lines of `source` as `str::lines` gives them, or as
    /// `split('\n')` does with `by_breaks`.
    pub(super) fn new(source: &'a str, by_breaks: bool) -> Self {
        if !in_batch(source) {
            return TextLines {
                source,
                ranges: Rc::new(ranges(source, by_breaks)),
                kept: None,
            };
        }
        let (ranges, kept) = if by_breaks {
            let read = batch_cached(source, || {
                let ranges = ranges(source, true);
                let kept = kept(source, &ranges);
                ByBreaks(Rc::new(ranges), Rc::new(kept))
            });
            (read.0.clone(), read.1.clone())
        } else {
            let read = batch_cached(source, || {
                let ranges = ranges(source, false);
                let kept = kept(source, &ranges);
                ByLines(Rc::new(ranges), Rc::new(kept))
            });
            (read.0.clone(), read.1.clone())
        };
        TextLines {
            source,
            ranges,
            kept: Some(kept),
        }
    }

    pub(super) fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Line `index`.
    pub(super) fn get(&self, index: usize) -> Option<&'a str> {
        let (start, end) = *self.ranges.get(index)?;
        Some(&self.source[start..end])
    }

    /// The lines from `index` on.
    pub(super) fn from(&self, index: usize) -> impl Iterator<Item = &'a str> + '_ {
        (index..self.len()).filter_map(|index| self.get(index))
    }

    /// The tables after the first `count` lines (`table_paths` of them, its
    /// last path, with the arrays of tables met).
    pub(super) fn tables_after(&self, count: usize) -> Tables {
        let count = count.min(self.len());
        let step = |tables: &mut Tables, index: usize| {
            let (start, end) = self.ranges[index];
            enter_line(tables, &self.source[start..end]);
        };
        match &self.kept {
            Some(kept) => kept.tables.before(count, step),
            None => {
                let mut tables = Tables::default();
                for index in 0..count {
                    step(&mut tables, index);
                }
                tables
            }
        }
    }

    /// The index of the line holding `byte` and its offset, counted as each
    /// line's length and one (see `line_at`).
    pub(super) fn line_at(&self, byte: usize) -> (usize, usize) {
        if let Some(kept) = &self.kept {
            // The first line whose end, so counted, is at or past `byte`.
            let found = kept.offsets.partition_point(|offset| *offset < byte);
            // `byte <= offset + len` holds from some line on; the lines
            // before `found` start before `byte`, so it is `found - 1` or
            // `found`.
            for candidate in [found.saturating_sub(1), found] {
                if let (Some(offset), Some((start, end))) =
                    (kept.offsets.get(candidate), self.ranges.get(candidate))
                    && byte <= offset + (end - start)
                {
                    return (candidate, *offset);
                }
            }
            return (self.len(), self.source.len());
        }
        let mut offset = 0;
        for index in 0..self.len() {
            let (start, end) = self.ranges[index];
            if byte <= offset + (end - start) {
                return (index, offset);
            }
            offset += end - start + 1;
        }
        (self.len(), self.source.len())
    }
}
