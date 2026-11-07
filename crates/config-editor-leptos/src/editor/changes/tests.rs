use super::super::diff::{DiffKind, Hunk, diff_lines};
use super::super::line_rows::{RenderedLine, row_state};
use super::*;

fn hunk(current: std::ops::Range<usize>, baseline: &[&str], counts: (usize, usize, usize)) -> Hunk {
    Hunk {
        current,
        baseline: baseline.iter().map(|line| line.to_string()).collect(),
        added: counts.0,
        modified: counts.1,
        removed: counts.2,
    }
}

/// Reverting every hunk, last first (so earlier ranges stay put).
fn revert_all(baseline: &str, current: &str) -> String {
    let line_diff = diff_lines(baseline, current);
    let mut text = current.to_string();
    for hunk in line_diff.hunks.iter().rev() {
        text = revert_hunk(&text, hunk).0;
    }
    text
}

#[test]
fn hunks_cover_added_modified_and_removed_runs() {
    let line_diff = diff_lines(
        "a: 1\nb: 2\nc: 3\nd: 4\ne: 5\n",
        "a: 1\nb: 20\nnew: x\nc: 3\ne: 5\n",
    );
    assert_eq!(
        line_diff.hunks,
        vec![
            hunk(1..3, &["b: 2"], (1, 1, 0)),
            hunk(4..4, &["d: 4"], (0, 0, 1)),
        ]
    );
    assert_eq!(
        ChangeSummary::of(&line_diff),
        ChangeSummary {
            added: 1,
            removed: 1,
            modified: 1,
            hunks: 2
        }
    );
}

#[test]
fn empty_baseline_counts_every_line_as_added() {
    let line_diff = diff_lines("", "a: 1\nb: 2");
    assert_eq!(line_diff.current, vec![DiffKind::Added, DiffKind::Added]);
    assert_eq!(line_diff.hunks, vec![hunk(0..2, &[], (2, 0, 0))]);
    assert_eq!(revert_all("", "a: 1\nb: 2"), "");
    assert!(ChangeSummary::between("", "").is_empty());
}

#[test]
fn emptied_document_counts_every_line_as_removed() {
    let line_diff = diff_lines("a: 1\nb: 2", "");
    assert!(line_diff.deleted_after);
    assert_eq!(line_diff.deleted_lines_after, vec!["a: 1", "b: 2"]);
    assert_eq!(
        line_diff.hunks,
        vec![hunk(0..0, &["a: 1", "b: 2"], (0, 0, 2))]
    );
    assert_eq!(revert_all("a: 1\nb: 2", ""), "a: 1\nb: 2");
}

#[test]
fn changes_at_the_start_and_end() {
    let baseline = "first\nmiddle\nlast\n";
    let current = "FIRST\nmiddle\nlast\nextra\n";
    let line_diff = diff_lines(baseline, current);
    assert_eq!(
        line_diff.hunks,
        vec![
            hunk(0..1, &["first"], (0, 1, 0)),
            hunk(3..4, &[], (1, 0, 0)),
        ]
    );
    assert_eq!(revert_all(baseline, current), baseline);
}

#[test]
fn removed_only_hunks_at_the_start_middle_and_end() {
    let baseline = "one\ntwo\nthree\nfour\nfive";
    let current = "two\nfour";
    let line_diff = diff_lines(baseline, current);
    assert_eq!(
        line_diff.hunks,
        vec![
            hunk(0..0, &["one"], (0, 0, 1)),
            hunk(1..1, &["three"], (0, 0, 1)),
            hunk(2..2, &["five"], (0, 0, 1)),
        ]
    );
    assert_eq!(revert_all(baseline, current), baseline);
}

#[test]
fn reverting_one_hunk_restores_exactly_its_baseline_lines() {
    let baseline = "a: 1\nb: 2\nc: 3\nd: 4\n";
    let current = "a: 1\nb: 20\nc: 3\n";
    let line_diff = diff_lines(baseline, current);
    let (text, caret) = revert_hunk(current, &line_diff.hunks[0]);
    assert_eq!(text, "a: 1\nb: 2\nc: 3\n");
    assert_eq!(caret, "a: 1\n".len());
    // The other change is still there, and reverting it gives the baseline.
    let rest = diff_lines(baseline, &text);
    assert_eq!(rest.hunks.len(), 1);
    assert_eq!(revert_hunk(&text, &rest.hunks[0]).0, baseline);
}

#[test]
fn revert_keeps_blank_and_non_ascii_lines() {
    let baseline = "名前: 一\n\n  note: \"é\"\n";
    let current = "名前: 二\nnote: x\n";
    assert_eq!(revert_all(baseline, current), baseline);
    let line_diff = diff_lines(baseline, current);
    let (_, caret) = revert_hunk(current, line_diff.hunks.last().unwrap());
    // UTF-16 offset of the restored line's start.
    let (text, _) = revert_hunk(current, line_diff.hunks.last().unwrap());
    assert!(caret <= text.encode_utf16().count());
}

/// A small deterministic generator, so the matrix needs no dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

fn random_document(random: &mut Lcg) -> String {
    const WORDS: &[&str] = &["a: 1", "b: 2", "", "  - x", "c = 3", "<d/>", "}"];
    let count = random.next(8);
    let mut text = (0..count)
        .map(|_| WORDS[random.next(WORDS.len())])
        .collect::<Vec<_>>()
        .join("\n");
    if random.next(2) == 0 && !text.is_empty() {
        text.push('\n');
    }
    text
}

#[test]
fn reverting_every_hunk_gives_the_baseline_property() {
    let mut random = Lcg(7);
    for _ in 0..3000 {
        let baseline = random_document(&mut random);
        let current = random_document(&mut random);
        let reverted = revert_all(&baseline, &current);
        // Only a dropped final newline shows no marker, so it is no change.
        assert!(
            reverted == baseline || format!("{reverted}\n") == baseline,
            "{baseline:?} -> {current:?} reverted to {reverted:?}"
        );
        // Each hunk alone takes away exactly one change.
        let line_diff = diff_lines(&baseline, &current);
        for hunk in &line_diff.hunks {
            let (text, _) = revert_hunk(&current, hunk);
            let after = ChangeSummary::between(&baseline, &text);
            assert!(
                after.hunks < line_diff.hunks.len(),
                "{baseline:?} -> {current:?}: reverting {hunk:?} left {after:?}"
            );
        }
        let summary = ChangeSummary::of(&line_diff);
        let marked = line_diff
            .current
            .iter()
            .filter(|kind| **kind != DiffKind::Unchanged)
            .count();
        assert_eq!(summary.added + summary.modified, marked);
        let removed = line_diff
            .deleted_lines_before
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            + line_diff.deleted_lines_after.len();
        assert_eq!(summary.removed, removed);
    }
}

fn rendered(text: &str, line_diff: &super::super::diff::LineDiff) -> Vec<RenderedLine> {
    text.split('\n')
        .enumerate()
        .map(|(index, line)| RenderedLine {
            text: line.to_string(),
            has_error: false,
            change: line_diff
                .current
                .get(index)
                .copied()
                .unwrap_or(DiffKind::Unchanged),
            marks: None,
        })
        .collect()
}

#[test]
fn every_marker_preview_finds_its_hunk() {
    let mut random = Lcg(11);
    for _ in 0..2000 {
        let baseline = random_document(&mut random);
        let current = random_document(&mut random);
        let line_diff = diff_lines(&baseline, &current);
        let rows = rendered(&current, &line_diff);
        for index in 0..rows.len() {
            let row = row_state(index, &rows, rows.len(), &line_diff, 0);
            for peek in [
                row.modified_peek,
                row.added_peek,
                row.deleted_peek,
                row.deleted_after_peek,
            ]
            .into_iter()
            .flatten()
            {
                assert!(
                    hunk_for_peek(&line_diff, &peek).is_some(),
                    "{baseline:?} -> {current:?}: no hunk for {peek:?}"
                );
            }
        }
    }
}

#[test]
fn next_and_previous_change_wrap_around() {
    let line_diff = diff_lines("a\nb\nc\nd\ne\nf", "a\nB\nc\nd\nf");
    // Changes start on line 1 (B) and before line 4 (removed e, at f).
    let starts: Vec<usize> = line_diff
        .hunks
        .iter()
        .map(|hunk| change_line(hunk, 5))
        .collect();
    assert_eq!(starts, vec![1, 4]);
    let next = |line| change_line(adjacent_change(&line_diff, 5, line, true).unwrap(), 5);
    let previous = |line| change_line(adjacent_change(&line_diff, 5, line, false).unwrap(), 5);
    assert_eq!(next(0), 1);
    assert_eq!(next(1), 4);
    assert_eq!(next(4), 1);
    assert_eq!(previous(4), 1);
    assert_eq!(previous(1), 4);
    assert!(adjacent_change(&diff_lines("a", "a"), 1, 0, true).is_none());
}

#[test]
fn an_added_line_preview_survives_edits_elsewhere_only() {
    use super::super::diff::{PeekKind, peek_survives_edit};
    let baseline = "a: 1\nb: 2\nc: 3\n";
    let lines = |text: &str| -> Vec<String> { text.split('\n').map(str::to_string).collect() };
    let current = "a: 1\nnew: 3\nb: 2\nc: 3\n";
    let line_diff = diff_lines(baseline, current);
    let rows = rendered(current, &line_diff);
    let peek = row_state(1, &rows, rows.len(), &line_diff, 0)
        .added_peek
        .expect("line 2 is added");
    assert_eq!(peek.kind, PeekKind::Added);
    assert_eq!(peek.after, vec!["new: 3"]);
    assert!(peek_survives_edit(&peek, &lines(current), &line_diff));
    let elsewhere = "a: 1\nnew: 3\nb: 2\nc: 30\n";
    assert!(peek_survives_edit(
        &peek,
        &lines(elsewhere),
        &diff_lines(baseline, elsewhere)
    ));
    let own_line = "a: 1\nnew: 4\nb: 2\nc: 3\n";
    assert!(!peek_survives_edit(
        &peek,
        &lines(own_line),
        &diff_lines(baseline, own_line)
    ));
}

#[test]
fn removed_at_the_end_moves_to_the_last_line() {
    let line_diff = diff_lines("a\nb\nc", "a");
    assert_eq!(change_line(&line_diff.hunks[0], 1), 0);
}
