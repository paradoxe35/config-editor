//! The Myers edit script between two lists of lines, used by the
//! conversion (keeping untouched lines) and the read-only check, and the
//! line kinds the gutter marks (see `changes`).

/// How a line differs from the baseline, as the gutter marks it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DiffKind {
    Unchanged,
    Added,
    Modified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DiffEdit {
    Equal,
    Insert,
    Delete,
}

#[cfg(test)]
thread_local! {
    /// Steps of the line diffs' searches on this thread (see [`steps`]).
    static STEPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `count` steps of a line diff's search (counted on this thread in tests):
/// a diagonal tried, a line passed along one, a cell of the frontier kept
/// for the way back ([`myers_edits`]), a cell of the table of common lines
/// (`changes::middle_edits`). Apart from `work_count`, whose matrices
/// measure small documents, where these searches are exact and bounded.
#[inline(always)]
pub(super) fn step(count: usize) {
    #[cfg(test)]
    STEPS.with(|steps| steps.set(steps.get() + count));
    #[cfg(not(test))]
    let _ = count;
}

/// The steps counted so far on this thread.
#[cfg(test)]
#[allow(
    dead_code,
    reason = "read by the hostile random suite, which needs every feature"
)]
pub(super) fn steps() -> usize {
    STEPS.with(std::cell::Cell::get)
}

/// Myers depth cap, in edit steps: the search costs `O(depth²)`, so beyond
/// this the changed region is diffed as one delete-then-insert block.
pub(super) const MYERS_MAX_DEPTH: usize = 1024;

pub(super) fn myers_edits(old: &[&str], new: &[&str]) -> Option<Vec<DiffEdit>> {
    let max_depth = old.len().saturating_add(new.len()).min(MYERS_MAX_DEPTH);
    let offset = max_depth as isize + 1;
    let mut frontier = vec![0isize; max_depth * 2 + 3];
    frontier[(offset + 1) as usize] = 0;
    let mut trace = Vec::with_capacity(max_depth + 1);

    for depth in 0..=max_depth {
        trace.push(frontier.clone());
        step(frontier.len());
        let depth_signed = depth as isize;
        let mut diagonal = -depth_signed;
        while diagonal <= depth_signed {
            step(1);
            let index = (offset + diagonal) as usize;
            let mut x = if diagonal == -depth_signed
                || (diagonal != depth_signed && frontier[index - 1] < frontier[index + 1])
            {
                frontier[index + 1]
            } else {
                frontier[index - 1] + 1
            };
            let mut y = x - diagonal;
            while x < old.len() as isize
                && y < new.len() as isize
                && old[x as usize] == new[y as usize]
            {
                step(1);
                x += 1;
                y += 1;
            }
            frontier[index] = x;
            if x >= old.len() as isize && y >= new.len() as isize {
                return Some(backtrack_myers(&trace, depth, offset, old.len(), new.len()));
            }
            diagonal += 2;
        }
    }
    None
}

fn backtrack_myers(
    trace: &[Vec<isize>],
    depth: usize,
    offset: isize,
    old_len: usize,
    new_len: usize,
) -> Vec<DiffEdit> {
    let (mut x, mut y) = (old_len as isize, new_len as isize);
    let mut edits = Vec::with_capacity(old_len + new_len);
    for depth in (1..=depth).rev() {
        let diagonal = x - y;
        let frontier = &trace[depth];
        let index = (offset + diagonal) as usize;
        let previous_diagonal = if diagonal == -(depth as isize)
            || (diagonal != depth as isize && frontier[index - 1] < frontier[index + 1])
        {
            diagonal + 1
        } else {
            diagonal - 1
        };
        let previous_x = frontier[(offset + previous_diagonal) as usize];
        let previous_y = previous_x - previous_diagonal;
        while x > previous_x && y > previous_y {
            edits.push(DiffEdit::Equal);
            x -= 1;
            y -= 1;
        }
        if x == previous_x {
            edits.push(DiffEdit::Insert);
        } else {
            edits.push(DiffEdit::Delete);
        }
        x = previous_x;
        y = previous_y;
    }
    while x > 0 && y > 0 {
        edits.push(DiffEdit::Equal);
        x -= 1;
        y -= 1;
    }
    while x > 0 {
        edits.push(DiffEdit::Delete);
        x -= 1;
    }
    while y > 0 {
        edits.push(DiffEdit::Insert);
        y -= 1;
    }
    edits.reverse();
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn myers_fallback_preserves_context_for_large_line_ranges() {
        let old: Vec<_> = (0..1200).map(|line| format!("line-{line}")).collect();
        let mut new = old.clone();
        new.insert(600, "inserted".to_string());
        let old_refs: Vec<_> = old.iter().map(String::as_str).collect();
        let new_refs: Vec<_> = new.iter().map(String::as_str).collect();

        let edits = myers_edits(&old_refs, &new_refs).unwrap();
        assert_eq!(
            edits
                .iter()
                .filter(|edit| matches!(edit, DiffEdit::Equal))
                .count(),
            1200
        );
        assert_eq!(
            edits
                .iter()
                .filter(|edit| matches!(edit, DiffEdit::Insert))
                .count(),
            1
        );
        assert_eq!(
            edits
                .iter()
                .filter(|edit| matches!(edit, DiffEdit::Delete))
                .count(),
            0
        );
    }
}
