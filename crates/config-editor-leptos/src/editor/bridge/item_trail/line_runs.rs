//! The lines an edit spanning lines changed, told from the lines it wrote
//! back as they were: a large edit (Replace all, the whole text pasted
//! over itself with a few changes) rewrites a few lines among many it
//! keeps, and an item on the lines kept is the item it was (QA `SP37-1`:
//! each item of such an edit cut and looked for again was taken for the
//! next one written alike when its own lines changed, and every item of
//! the document counted as moved).

use super::super::super::work_count::{Work, note};
use std::ops::Range;

/// How many lines on from a change the lines written back as they were
/// are looked for on each side.
const REACH: usize = 32;

/// A run of lines an edit changed: the bytes of its removed text, and of
/// its inserted text, they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Changed {
    pub(super) removed: Range<usize>,
    pub(super) inserted: Range<usize>,
}

/// Each line of `text` with its line break, and where it starts (and,
/// one more, the end).
fn lines(text: &str) -> (Vec<&str>, Vec<usize>) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut at = 0;
    for line in &lines {
        starts.push(at);
        at += line.len();
    }
    starts.push(at);
    (lines, starts)
}

/// The runs of lines `removed` → `inserted` changed, in order, between
/// lines written as they were: matched from each change to the nearest
/// line both write again, the lines past a change no near line ends
/// matched from the end.
pub(super) fn changed(removed: &str, inserted: &str) -> Vec<Changed> {
    let ((old, old_at), (new, new_at)) = (lines(removed), lines(inserted));
    let run = |old: Range<usize>, new: Range<usize>| Changed {
        removed: old_at[old.start]..old_at[old.end],
        inserted: new_at[new.start]..new_at[new.end],
    };
    let mut runs = Vec::new();
    let (mut i, mut j) = (0, 0);
    loop {
        while i < old.len() && j < new.len() && old[i] == new[j] {
            note(Work::TrailStep);
            i += 1;
            j += 1;
        }
        if i == old.len() && j == new.len() {
            break;
        }
        // The nearest such line: the fewest lines passed on the side
        // passing more (a line rewritten is one line changed on each side,
        // not two items moved), then on both.
        let resumed = (1..=REACH).find_map(|reach| {
            (0..=reach)
                .flat_map(|other| [(other, reach), (reach, other)])
                .find_map(|(skipped_old, skipped_new)| {
                    let (to_old, to_new) = (i + skipped_old, j + skipped_new);
                    note(Work::TrailStep);
                    let ends = to_old == old.len() && to_new == new.len();
                    let meets =
                        to_old < old.len() && to_new < new.len() && old[to_old] == new[to_new];
                    (ends || meets).then_some((to_old, to_new))
                })
        });
        match resumed {
            Some((to_old, to_new)) => {
                runs.push(run(i..to_old, j..to_new));
                (i, j) = (to_old, to_new);
            }
            None => {
                // The lines both end with, written as they were.
                let (mut end_old, mut end_new) = (old.len(), new.len());
                while end_old > i && end_new > j && old[end_old - 1] == new[end_new - 1] {
                    note(Work::TrailStep);
                    end_old -= 1;
                    end_new -= 1;
                }
                runs.push(run(i..end_old, j..end_new));
                break;
            }
        }
    }
    runs
}

/// How far a place of the removed text at byte `at` moves into the
/// inserted text, by the changed runs before it: with a run inserting
/// lines at `at` itself when `inserted_at` (a line starting there comes
/// after them; one ending there, before).
pub(super) fn moved_by(runs: &[Changed], at: usize, inserted_at: bool) -> isize {
    let before = runs.partition_point(|run| {
        run.removed.end < at || run.removed.end == at && (inserted_at || run.removed.start < at)
    });
    before.checked_sub(1).map_or(0, |last| {
        let run = &runs[last];
        run.inserted.end as isize - run.removed.end as isize
    })
}

/// The changed run whose removed lines hold byte `at` (from its first
/// byte, on).
pub(super) fn holding(runs: &[Changed], at: usize) -> Option<&Changed> {
    let after = runs.partition_point(|run| run.removed.start <= at);
    let run = &runs[after.checked_sub(1)?];
    (at < run.removed.end).then_some(run)
}

#[cfg(test)]
mod tests {
    use super::{Changed, changed};

    #[test]
    fn lines_written_back_as_they_were_are_no_change() {
        // A line rewritten among lines written alike is that line changed,
        // not the next ones moved.
        let removed = "- a\n  b: 1\n- a\n  b: 1\n- a\n  b: 1\n";
        let inserted = removed.replacen("b: 1", "b: 2", 1);
        assert_eq!(
            changed(removed, &inserted),
            vec![Changed {
                removed: 4..11,
                inserted: 4..11
            }]
        );
        let removed = "a: 1\nb: 2\nc: 3\nd: 4\n";
        let inserted = "a: 1\nb: 9\nc: 3\nx: 0\nd: 4\n";
        assert_eq!(
            changed(removed, inserted),
            vec![
                Changed {
                    removed: 5..10,
                    inserted: 5..10
                },
                Changed {
                    removed: 15..15,
                    inserted: 15..20
                },
            ]
        );
        // Two items swapped: one is kept, the other inserted and removed.
        let swapped = changed("a\nb\n", "b\na\n");
        assert_eq!(
            swapped,
            vec![
                Changed {
                    removed: 0..0,
                    inserted: 0..2
                },
                Changed {
                    removed: 2..4,
                    inserted: 4..4
                },
            ]
        );
        // Past the reach, the lines both end with are kept.
        let far: String = (0..100).map(|line| format!("n{line}\n")).collect();
        let runs = changed("x\ny\n", &format!("{far}y\n"));
        assert_eq!(
            runs,
            vec![Changed {
                removed: 0..2,
                inserted: 0..far.len()
            }]
        );
    }
}
