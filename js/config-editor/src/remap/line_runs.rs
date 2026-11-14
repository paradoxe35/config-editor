//! The runs of lines two texts differ in, with no size past which the
//! answer stops being exact: common lines at both ends set aside, lines
//! written once in each text anchoring them (patience diff), and Myers run
//! only inside the gaps between anchors. A gap too big for Myers that no
//! line anchors is one changed run on its own: never the whole stretch
//! from the first change to the last (QA 31: past ~512 edit steps every
//! range between two changes grew over all of it).

use std::collections::HashMap;
use std::ops::Range;

/// A run of `old` lines and the run of `new` lines it became.
pub(super) type Run = (Range<usize>, Range<usize>);

/// Gaps up to this many lines (both texts together) go to Myers without
/// looking for anchors: its search costs at most this many lines' worth
/// per depth step.
const SMALL_GAP: usize = 128;

/// Myers depth cap, in changed lines: the search keeps `O(depth²)` numbers
/// and costs `O(lines × depth)`, so a gap needing more is one run.
const MAX_DEPTH: usize = 256;

/// Lines hashed for anchors, per line of the two texts: what the recursion
/// may spend before the gaps left go to Myers as they are (keeps the work
/// linear in the texts whatever their lines repeat).
const ANCHOR_BUDGET: usize = 16;

#[cfg(test)]
thread_local! {
    /// Lines compared or hashed so far (the scale tests count work).
    pub(crate) static WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts `lines` of work in tests; nothing otherwise.
#[inline]
fn note(lines: usize) {
    #[cfg(test)]
    WORK.with(|work| work.set(work.get() + lines));
    #[cfg(not(test))]
    let _ = lines;
}

/// The runs `old` and `new` differ in, in order, none touching another.
pub(super) fn line_runs(old: &[&str], new: &[&str]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut budget = ANCHOR_BUDGET * (old.len() + new.len());
    // Gaps left to diff, the next one last.
    let mut gaps: Vec<Run> = vec![(0..old.len(), 0..new.len())];
    while let Some((old_gap, new_gap)) = gaps.pop() {
        let (old_gap, new_gap) = trimmed(old, new, old_gap, new_gap);
        if old_gap.is_empty() || new_gap.is_empty() {
            push(&mut runs, old_gap, new_gap);
            continue;
        }
        let size = old_gap.len() + new_gap.len();
        if size > SMALL_GAP && size <= budget {
            budget -= size;
            let anchors = anchors(&old[old_gap.clone()], &new[new_gap.clone()]);
            if !anchors.is_empty() {
                let (mut old_start, mut new_start) = (old_gap.start, new_gap.start);
                let mut pieces = Vec::with_capacity(anchors.len() + 1);
                for (old_at, new_at) in anchors {
                    let (old_at, new_at) = (old_gap.start + old_at, new_gap.start + new_at);
                    pieces.push((old_start..old_at, new_start..new_at));
                    (old_start, new_start) = (old_at + 1, new_at + 1);
                }
                pieces.push((old_start..old_gap.end, new_start..new_gap.end));
                gaps.extend(pieces.into_iter().rev());
                continue;
            }
        }
        match myers(&old[old_gap.clone()], &new[new_gap.clone()]) {
            Some(found) => {
                for (old_run, new_run) in found {
                    push(
                        &mut runs,
                        old_gap.start + old_run.start..old_gap.start + old_run.end,
                        new_gap.start + new_run.start..new_gap.start + new_run.end,
                    );
                }
            }
            None => push(&mut runs, old_gap, new_gap),
        }
    }
    runs
}

/// `old_gap` and `new_gap` without the lines they start and end alike.
fn trimmed(old: &[&str], new: &[&str], old_gap: Range<usize>, new_gap: Range<usize>) -> Run {
    let head = old[old_gap.clone()]
        .iter()
        .zip(&new[new_gap.clone()])
        .take_while(|(a, b)| a == b)
        .count();
    let (old_gap, new_gap) = (
        old_gap.start + head..old_gap.end,
        new_gap.start + head..new_gap.end,
    );
    let tail = old[old_gap.clone()]
        .iter()
        .rev()
        .zip(new[new_gap.clone()].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    note(head + tail + 1);
    (
        old_gap.start..old_gap.end - tail,
        new_gap.start..new_gap.end - tail,
    )
}

/// A changed run after `runs`: joined to the last when they touch.
fn push(runs: &mut Vec<Run>, old_run: Range<usize>, new_run: Range<usize>) {
    if old_run.is_empty() && new_run.is_empty() {
        return;
    }
    match runs.last_mut() {
        Some((old_last, new_last))
            if old_last.end == old_run.start && new_last.end == new_run.start =>
        {
            old_last.end = old_run.end;
            new_last.end = new_run.end;
        }
        _ => runs.push((old_run, new_run)),
    }
}

/// The lines written once in `old` and once in `new` that stay in order
/// (the longest such sequence), as `(old index, new index)` pairs.
fn anchors(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    note(old.len() + new.len());
    // Per line: its count in each text (up to 2) and where it is in each.
    let mut seen: HashMap<&str, (u8, u8, usize, usize)> = HashMap::with_capacity(old.len());
    for (index, line) in old.iter().enumerate() {
        let entry = seen.entry(*line).or_insert((0, 0, index, 0));
        entry.0 = entry.0.saturating_add(1).min(2);
    }
    for (index, line) in new.iter().enumerate() {
        if let Some(entry) = seen.get_mut(line) {
            entry.1 = entry.1.saturating_add(1).min(2);
            entry.3 = index;
        }
    }
    let mut unique: Vec<(usize, usize)> = seen
        .into_values()
        .filter(|(in_old, in_new, _, _)| *in_old == 1 && *in_new == 1)
        .map(|(_, _, old_at, new_at)| (old_at, new_at))
        .collect();
    unique.sort_unstable();
    in_order(&unique)
}

/// The longest run of `pairs` (sorted by their first index) whose second
/// indexes rise too (patience sorting, `O(k log k)`).
fn in_order(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    // `tops[k]`: the pair ending the best rising run of length k + 1 so far.
    let mut tops: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; pairs.len()];
    for (index, (_, new_at)) in pairs.iter().enumerate() {
        let length = tops.partition_point(|top| pairs[*top].1 < *new_at);
        before[index] = length.checked_sub(1).map(|previous| tops[previous]);
        if length == tops.len() {
            tops.push(index);
        } else {
            tops[length] = index;
        }
    }
    let mut chain = Vec::with_capacity(tops.len());
    let mut at = tops.last().copied();
    while let Some(index) = at {
        chain.push(pairs[index]);
        at = before[index];
    }
    chain.reverse();
    chain
}

/// The runs `old` and `new` differ in (Myers), or `None` past
/// [`MAX_DEPTH`].
fn myers(old: &[&str], new: &[&str]) -> Option<Vec<Run>> {
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
            let from = x;
            while x < n && y < m && old[x as usize] == new[y as usize] {
                x += 1;
                y += 1;
            }
            note((x - from) as usize + 1);
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
fn runs_of(trace: &[Vec<isize>], depth: isize, n: isize, m: isize) -> Vec<Run> {
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
    let mut runs: Vec<Run> = Vec::new();
    for (x, y, inserted) in steps {
        let (old_end, new_end) = if inserted { (x, y + 1) } else { (x + 1, y) };
        push(
            &mut runs,
            x as usize..old_end as usize,
            y as usize..new_end as usize,
        );
    }
    runs
}
