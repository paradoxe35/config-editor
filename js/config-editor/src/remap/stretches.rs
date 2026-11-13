//! The stretches two texts differ in: their lines diffed (`line_runs`),
//! each changed run of lines narrowed to the characters that differ.

use std::ops::Range;

#[path = "line_runs.rs"]
pub(super) mod line_runs;

/// One stretch two texts differ in, as UTF-16 offsets: `from` in the first
/// text became `to` in the second. Stretches come in text order, with
/// unchanged text between them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Stretch {
    pub(super) from: Range<usize>,
    pub(super) to: Range<usize>,
}

/// The lines of a text (each with its break) and where each starts, in
/// bytes and UTF-16 units; one more entry for the end.
struct Lines<'a> {
    lines: Vec<&'a str>,
    bytes: Vec<usize>,
    units: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let (mut bytes, mut units) = (vec![0], vec![0]);
        for line in &lines {
            bytes.push(bytes.last().copied().unwrap_or(0) + line.len());
            units.push(units.last().copied().unwrap_or(0) + line.encode_utf16().count());
        }
        Self {
            lines,
            bytes,
            units,
        }
    }
}

/// The stretches `from` and `to` differ in.
pub(super) fn stretches(from: &str, to: &str) -> Vec<Stretch> {
    let (old, new) = (Lines::new(from), Lines::new(to));
    line_runs::line_runs(&old.lines, &new.lines)
        .into_iter()
        .filter_map(|(old_run, new_run)| narrowed(from, to, (&old, old_run), (&new, new_run)))
        .collect()
}

/// A changed run of lines, as the stretch of characters that differ in it.
fn narrowed(
    from: &str,
    to: &str,
    (old, old_run): (&Lines, Range<usize>),
    (new, new_run): (&Lines, Range<usize>),
) -> Option<Stretch> {
    let old_text = &from[old.bytes[old_run.start]..old.bytes[old_run.end]];
    let new_text = &to[new.bytes[new_run.start]..new.bytes[new_run.end]];
    let (mut head, mut head_bytes) = (0, 0);
    for (left, right) in old_text.chars().zip(new_text.chars()) {
        if left != right {
            break;
        }
        head += left.len_utf16();
        head_bytes += left.len_utf8();
    }
    let mut tail = 0;
    for (left, right) in old_text[head_bytes..]
        .chars()
        .rev()
        .zip(new_text[head_bytes..].chars().rev())
    {
        if left != right {
            break;
        }
        tail += left.len_utf16();
    }
    let from = old.units[old_run.start] + head..old.units[old_run.end] - tail;
    let to = new.units[new_run.start] + head..new.units[new_run.end] - tail;
    (!(from.is_empty() && to.is_empty())).then_some(Stretch { from, to })
}
