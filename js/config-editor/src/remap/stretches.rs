//! The stretches two texts differ in: their lines diffed (Myers), each
//! changed run of lines narrowed to the characters that differ.

use std::ops::Range;

/// One stretch two texts differ in, as UTF-16 offsets: `from` in the first
/// text became `to` in the second. Stretches come in text order, with
/// unchanged text between them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Stretch {
    pub(super) from: Range<usize>,
    pub(super) to: Range<usize>,
}

/// Myers depth cap, in changed lines: the search keeps `O(depth²)` numbers,
/// so past it the lines between the first and last change are one stretch
/// (a range there grows over them: never smaller).
const MAX_DEPTH: usize = 512;

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
    let (old_lines, new_lines) = (&old.lines, &new.lines);
    let prefix = old_lines
        .iter()
        .zip(new_lines)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = old_lines[prefix..]
        .iter()
        .rev()
        .zip(new_lines[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_middle = &old_lines[prefix..old_lines.len() - suffix];
    let new_middle = &new_lines[prefix..new_lines.len() - suffix];
    let runs = changed_runs(old_middle, new_middle)
        .unwrap_or_else(|| vec![(0..old_middle.len(), 0..new_middle.len())]);
    runs.into_iter()
        .filter_map(|(old_run, new_run)| {
            let old_run = old_run.start + prefix..old_run.end + prefix;
            let new_run = new_run.start + prefix..new_run.end + prefix;
            narrowed(from, to, (&old, old_run), (&new, new_run))
        })
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

/// The runs of lines `old` and `new` differ in (Myers): pairs of a run of
/// `old` and the run of `new` it became, in order. `None` past
/// [`MAX_DEPTH`].
fn changed_runs(old: &[&str], new: &[&str]) -> Option<Vec<(Range<usize>, Range<usize>)>> {
    let (n, m) = (old.len() as isize, new.len() as isize);
    let max = (old.len() + new.len()).min(MAX_DEPTH) as isize;
    let offset = max + 1;
    let mut frontier = vec![0isize; (2 * max + 3) as usize];
    // The frontier before each depth, for diagonals `-depth..=depth`.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    for depth in 0..=max {
        for diagonal in (-depth..=depth).step_by(2) {
            let at = (offset + diagonal) as usize;
            let mut x = if diagonal == -depth
                || (diagonal != depth && frontier[at - 1] < frontier[at + 1])
            {
                frontier[at + 1]
            } else {
                frontier[at - 1] + 1
            };
            let mut y = x - diagonal;
            while x < n && y < m && old[x as usize] == new[y as usize] {
                x += 1;
                y += 1;
            }
            frontier[at] = x;
            if x >= n && y >= m {
                return Some(runs_of(&trace, depth, n, m));
            }
        }
        trace.push(frontier[(offset - depth) as usize..=(offset + depth) as usize].to_vec());
    }
    None
}

/// The changed runs of the path Myers found at `depth`, walked back.
fn runs_of(
    trace: &[Vec<isize>],
    depth: isize,
    n: isize,
    m: isize,
) -> Vec<(Range<usize>, Range<usize>)> {
    let (mut x, mut y) = (n, m);
    // Each step back: the line of `old` deleted or of `new` inserted.
    let mut steps: Vec<(isize, isize, bool)> = Vec::new();
    for depth in (1..=depth).rev() {
        let previous = &trace[(depth - 1) as usize];
        let at = |diagonal: isize| previous[(diagonal + depth - 1) as usize];
        let diagonal = x - y;
        let inserted =
            diagonal == -depth || (diagonal != depth && at(diagonal - 1) < at(diagonal + 1));
        let previous_diagonal = if inserted { diagonal + 1 } else { diagonal - 1 };
        let previous_x = at(previous_diagonal);
        let previous_y = previous_x - previous_diagonal;
        steps.push((previous_x, previous_y, inserted));
        (x, y) = (previous_x, previous_y);
    }
    steps.reverse();
    let mut runs: Vec<(Range<usize>, Range<usize>)> = Vec::new();
    for (x, y, inserted) in steps {
        let (old_end, new_end) = if inserted { (x, y + 1) } else { (x + 1, y) };
        let (x, y, old_end, new_end) = (x as usize, y as usize, old_end as usize, new_end as usize);
        match runs.last_mut() {
            // Next to the run before: the same run.
            Some((old_run, new_run)) if old_run.end == x && new_run.end == y => {
                old_run.end = old_end;
                new_run.end = new_end;
            }
            _ => runs.push((x..old_end, y..new_end)),
        }
    }
    runs
}
