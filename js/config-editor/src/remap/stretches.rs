//! The stretches two texts differ in: their lines diffed (`line_runs`),
//! each changed run of lines narrowed to the characters that differ. A run
//! of repeated lines (none written once in it, on either side: a value
//! replaced in a list of the same items) that replaced as many lines as it
//! had, each line by one alike (the same text around what changed), is
//! narrowed line by line, so a range on one of its lines stays on that
//! line (QA 32: such a list was one stretch, and every range in it grew
//! over all of it). Any other run stays one stretch: its lines tell where
//! their characters went, and a range over it only grows.

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
        .flat_map(|(old_run, new_run)| line_by_line(&old.lines, &new.lines, old_run, new_run))
        .filter_map(|(old_run, new_run)| narrowed(from, to, (&old, old_run), (&new, new_run)))
        .collect()
}

/// A changed run, as one run per line changed when its lines all repeat
/// and it replaced each of them by one alike (see [`alike`]); else whole.
fn line_by_line(
    old: &[&str],
    new: &[&str],
    old_run: Range<usize>,
    new_run: Range<usize>,
) -> Vec<(Range<usize>, Range<usize>)> {
    let pairs = old_run.clone().zip(new_run.clone());
    if old_run.len() < 2
        || old_run.len() != new_run.len()
        || !pairs
            .clone()
            .all(|(old_at, new_at)| alike(old[old_at], new[new_at]))
        || !all_repeated(&old[old_run.clone()])
        || !all_repeated(&new[new_run.clone()])
    {
        return vec![(old_run, new_run)];
    }
    pairs
        .filter(|(old_at, new_at)| old[*old_at] != new[*new_at])
        .map(|(old_at, new_at)| (old_at..old_at + 1, new_at..new_at + 1))
        .collect()
}

/// Whether no line of `lines` is written once in it.
fn all_repeated(lines: &[&str]) -> bool {
    let mut counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::with_capacity(lines.len());
    for line in lines {
        *counts.entry(line).or_default() += 1;
    }
    counts.values().all(|count| *count > 1)
}

/// Whether line `new` reads as `old` changed in place: the text they share
/// at their start and end is at least half of each.
fn alike(old: &str, new: &str) -> bool {
    let head = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    let tail = old.as_bytes()[head..]
        .iter()
        .rev()
        .zip(new.as_bytes()[head..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let shared = head + tail;
    2 * shared >= old.len() && 2 * shared >= new.len()
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
