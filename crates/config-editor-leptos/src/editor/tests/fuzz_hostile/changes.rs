//! The changes between two texts, on random pairs (small and large, near
//! and far apart, with any line endings): the unified diff applied to the
//! first gives the second; the gutter's line diff has its hunks in order
//! and inside the text, each counting the lines it covers, every line
//! outside them unchanged, and reverting them all gives the first text
//! back (a final line break removed is no change, as documented); the
//! change count is the hunks'; the edit script turns the old lines into
//! the new. And the search's cost on a large text with few changes grows
//! with the text, not with its square.

use super::runner::{Cap, Case, Failure, Rng, cases, run};
use super::texts::{literal, shown};
use crate::ChangeSummary;
use crate::editor::changes::{Hunk, LineDiff, diff_lines, middle_edits, revert_hunk, unified_diff};
use crate::editor::diff::steps;
use crate::editor::diff::{DiffEdit, DiffKind, myers_edits};

#[derive(Clone, Debug)]
pub(super) struct PairCase {
    pub(super) baseline: String,
    pub(super) current: String,
}

/// `text` with line `line` (from 0) left out, if it has one.
fn without_line(text: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = text.split_inclusive(['\n']).collect();
    (line < lines.len()).then(|| {
        lines
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != line)
            .map(|(_, line)| *line)
            .collect()
    })
}

impl Case for PairCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn changes_repro() {{\n    super::changes::replay(super::changes::PairCase {{\n        baseline: {}.into(),\n        current: {}.into(),\n    }});\n}}",
            literal(&self.baseline),
            literal(&self.current),
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out = Vec::new();
        let lines = |text: &str| text.split_inclusive('\n').count();
        // Halves of both, then a line of one, then a line of both.
        for (keep_baseline, keep_current) in [(true, false), (false, true)] {
            let half = |text: &str, first: bool| -> String {
                let all: Vec<&str> = text.split_inclusive('\n').collect();
                let middle = all.len() / 2;
                if first {
                    all[..middle].concat()
                } else {
                    all[middle..].concat()
                }
            };
            for first in [true, false] {
                out.push(Self {
                    baseline: if keep_baseline {
                        half(&self.baseline, first)
                    } else {
                        self.baseline.clone()
                    },
                    current: if keep_current {
                        half(&self.current, first)
                    } else {
                        self.current.clone()
                    },
                });
            }
        }
        for line in 0..lines(&self.baseline).min(300) {
            if let Some(baseline) = without_line(&self.baseline, line) {
                if let Some(current) = without_line(&self.current, line) {
                    out.push(Self {
                        baseline: baseline.clone(),
                        current,
                    });
                }
                out.push(Self {
                    baseline,
                    current: self.current.clone(),
                });
            }
        }
        for line in 0..lines(&self.current).min(300) {
            if let Some(current) = without_line(&self.current, line) {
                out.push(Self {
                    baseline: self.baseline.clone(),
                    current,
                });
            }
        }
        out
    }

    fn size(&self) -> usize {
        self.baseline.len() + self.current.len()
    }
}

/// Lines short and alike, so the same ones repeat.
const LINES: [&str; 14] = [
    "", " ", "a: 1", "b: 2", "  c: 3", "}", "{", "- x", "# note", "é😀", "a: 1 ", "\t", "x", "]",
];

fn random_lines(rng: &mut Rng, count: usize) -> Vec<String> {
    (0..count)
        .map(|index| {
            if rng.chance(30) {
                format!("line {index}")
            } else {
                rng.pick(&LINES).to_string()
            }
        })
        .collect()
}

/// `lines` written with random line endings (`\n` mostly).
fn written(rng: &mut Rng, lines: &[String], last_break: bool) -> String {
    let mixed = rng.chance(25);
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        out.push_str(line);
        if index + 1 < lines.len() || last_break {
            out.push_str(if mixed {
                rng.pick(&["\n", "\r\n", "\r"])
            } else {
                "\n"
            });
        }
    }
    out
}

/// `lines` with `count` random edits.
fn edited(rng: &mut Rng, lines: &[String], count: usize) -> Vec<String> {
    let mut out = lines.to_vec();
    for _ in 0..count {
        let at = rng.below(out.len() + 1);
        match rng.below(4) {
            0 => out.insert(at, rng.pick(&LINES).to_string()),
            1 if at < out.len() => {
                out.remove(at);
            }
            2 if at < out.len() => out[at] = format!("{} changed", out[at]),
            _ => {
                // A run of lines moved elsewhere.
                if at < out.len() {
                    let end = (at + 1 + rng.below(5)).min(out.len());
                    let run: Vec<String> = out.drain(at..end).collect();
                    let to = rng.below(out.len() + 1);
                    for (offset, line) in run.into_iter().enumerate() {
                        out.insert(to + offset, line);
                    }
                }
            }
        }
    }
    out
}

fn generate(rng: &mut Rng) -> Option<PairCase> {
    let count = match rng.below(10) {
        0 => 0,
        1..=6 => rng.below(40),
        7 | 8 => 200 + rng.below(1500),
        _ => 3000 + rng.below(3000),
    };
    let lines = random_lines(rng, count);
    let other = if rng.chance(15) {
        let other = rng.below(count + 5);
        random_lines(rng, other)
    } else {
        let changes = if rng.chance(70) {
            rng.below(6)
        } else {
            rng.below(count / 2 + 3)
        };
        edited(rng, &lines, changes)
    };
    let (baseline_break, current_break) = (rng.chance(70), rng.chance(70));
    Some(PairCase {
        baseline: written(rng, &lines, baseline_break),
        current: written(rng, &other, current_break),
    })
}

/// `baseline` (LF) with a unified diff applied; why not, if it does not
/// apply.
fn apply_patch(baseline: &str, patch: &str) -> Result<String, String> {
    if patch.is_empty() {
        return Ok(baseline.to_string());
    }
    let old: Vec<&str> = baseline.split_inclusive('\n').collect();
    let mut body = patch
        .strip_prefix("--- baseline\n+++ current\n")
        .ok_or("no headers")?
        .split_inclusive('\n')
        .peekable();
    let mut out = String::new();
    let mut next_old = 0;
    while let Some(header) = body.next() {
        let header = header
            .strip_prefix("@@ -")
            .and_then(|rest| rest.strip_suffix(" @@\n"))
            .ok_or_else(|| format!("not a hunk header: {header:?}"))?;
        let (old_range, new_range) = header.split_once(" +").ok_or("no + range")?;
        let parse = |range: &str| -> Result<(usize, usize), String> {
            let (start, count) = range.split_once(',').unwrap_or((range, "1"));
            Ok((
                start.parse().map_err(|_| format!("start {start:?}"))?,
                count.parse().map_err(|_| format!("count {count:?}"))?,
            ))
        };
        let ((old_start, old_count), (_, new_count)) = (parse(old_range)?, parse(new_range)?);
        // A range of no lines names the line before it.
        let first = if old_count == 0 {
            old_start
        } else {
            old_start.saturating_sub(1)
        };
        if first < next_old || first > old.len() {
            return Err(format!("a hunk at old line {old_start} out of order"));
        }
        out.extend(old[next_old..first].iter().copied());
        next_old = first;
        let (mut seen_old, mut seen_new) = (0, 0);
        while seen_old < old_count || seen_new < new_count {
            let line = body.next().ok_or("a hunk ends early")?;
            let (sign, rest) = line.split_at(1);
            let mut content = rest.to_string();
            if body.peek() == Some(&"\\ No newline at end of file\n") {
                body.next();
                content.pop();
            }
            match sign {
                " " | "-" => {
                    if old.get(next_old) != Some(&content.as_str()) {
                        return Err(format!(
                            "old line {} is {:?}, the patch says {content:?}",
                            next_old + 1,
                            old.get(next_old)
                        ));
                    }
                    next_old += 1;
                    seen_old += 1;
                    if sign == " " {
                        out.push_str(&content);
                        seen_new += 1;
                    }
                }
                "+" => {
                    out.push_str(&content);
                    seen_new += 1;
                }
                _ => return Err(format!("a line of no kind: {line:?}")),
            }
        }
        if seen_old != old_count || seen_new != new_count {
            return Err(format!(
                "a hunk's counts are {old_count},{new_count}, its lines {seen_old},{seen_new}"
            ));
        }
    }
    out.extend(old[next_old..].iter().copied());
    Ok(out)
}

fn lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Why `diff` does not hold together for `current` (LF).
fn bad_line_diff(diff: &LineDiff, current: &str, baseline_empty: bool) -> Option<String> {
    let lines = diff.current.len();
    if diff.deleted_before.len() != lines
        || diff.deleted_lines_before.len() != lines
        || diff.modified_before.len() != lines
    {
        return Some("its line lists differ in length".into());
    }
    // An empty text has no lines (one, when the baseline is empty too).
    let shown = if current.is_empty() {
        usize::from(baseline_empty)
    } else {
        current.split('\n').count()
    };
    if lines != shown {
        return Some(format!("{lines} line kinds for {shown} lines"));
    }
    let mut reach = 0;
    let mut inside = vec![false; lines];
    for hunk in &diff.hunks {
        if hunk.current.start < reach
            || hunk.current.end > lines
            || hunk.current.start > hunk.current.end
        {
            return Some(format!(
                "hunk {hunk:?} out of order or outside {lines} lines"
            ));
        }
        if hunk.current.len() != hunk.added + hunk.modified {
            return Some(format!(
                "hunk {hunk:?} covers other than its added and changed lines"
            ));
        }
        if hunk.baseline.len() < hunk.modified + hunk.removed {
            return Some(format!(
                "hunk {hunk:?} keeps fewer lines than it changed and removed"
            ));
        }
        for line in hunk.current.clone() {
            inside[line] = true;
        }
        reach = hunk.current.end;
    }
    for (line, kind) in diff.current.iter().enumerate() {
        if (*kind != DiffKind::Unchanged) != inside[line] {
            return Some(format!(
                "line {line} is {kind:?}, inside a hunk: {}",
                inside[line]
            ));
        }
    }
    None
}

/// Applies `edits` to `old`; whether it gives `new`.
fn script_gives(old: &[&str], new: &[&str], edits: &[DiffEdit]) -> bool {
    let (mut at_old, mut at_new) = (0, 0);
    for edit in edits {
        match edit {
            DiffEdit::Equal => {
                if old.get(at_old) != new.get(at_new) || at_old >= old.len() {
                    return false;
                }
                at_old += 1;
                at_new += 1;
            }
            DiffEdit::Delete => at_old += 1,
            DiffEdit::Insert => at_new += 1,
        }
    }
    at_old == old.len() && at_new == new.len()
}

pub(super) fn check(case: &PairCase) -> Result<(), Failure> {
    let (baseline, current) = (lf(&case.baseline), lf(&case.current));
    let pair = || format!("{} -> {}", shown(&case.baseline), shown(&case.current));
    for context in [0, 3] {
        let patch = unified_diff(&case.baseline, &case.current, context);
        if patch.is_empty() != (baseline == current) {
            return Err(Failure::new(
                "the unified diff is empty for texts that differ, or not for the same",
                pair(),
            ));
        }
        match apply_patch(&baseline, &patch) {
            Ok(patched) if patched == current => {}
            Ok(patched) => {
                return Err(Failure::new(
                    "the unified diff applied does not give the current text",
                    format!("{} gives {}\n{patch}", pair(), shown(&patched)),
                ));
            }
            Err(why) => {
                return Err(Failure::new(
                    "the unified diff does not apply to the baseline",
                    format!("{}: {why}\n{patch}", pair()),
                ));
            }
        }
    }
    let diff = diff_lines(&case.baseline, &case.current);
    if let Some(why) = bad_line_diff(&diff, &current, baseline.is_empty()) {
        return Err(Failure::new(
            "the line diff does not hold together",
            format!("{}: {why}", pair()),
        ));
    }
    let mut reverted = current.clone();
    for hunk in diff.hunks.iter().rev() {
        reverted = revert_hunk(&reverted, hunk).0;
    }
    // Only a final line break removed is no change.
    let same =
        reverted == baseline || (!current.ends_with('\n') && format!("{reverted}\n") == baseline);
    if !same {
        return Err(Failure::new(
            "reverting every change does not give the baseline",
            format!("{} reverts to {}", pair(), shown(&reverted)),
        ));
    }
    let summary = ChangeSummary::between(&case.baseline, &case.current);
    let sum = |field: fn(&Hunk) -> usize| -> usize { diff.hunks.iter().map(field).sum() };
    if summary.hunks != diff.hunks.len()
        || summary.added != sum(|hunk| hunk.added)
        || summary.removed != sum(|hunk| hunk.removed)
        || summary.modified != sum(|hunk| hunk.modified)
    {
        return Err(Failure::new(
            "the change count is not the hunks'",
            format!("{}: {summary:?}", pair()),
        ));
    }
    let old: Vec<&str> = baseline.split('\n').collect();
    let new: Vec<&str> = current.split('\n').collect();
    if !script_gives(&old, &new, &middle_edits(&old, &new)) {
        return Err(Failure::new(
            "the edit script does not turn the old lines into the new",
            pair(),
        ));
    }
    if let Some(edits) = myers_edits(&old, &new)
        && !script_gives(&old, &new, &edits)
    {
        return Err(Failure::new(
            "the Myers script does not turn the old lines into the new",
            pair(),
        ));
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: PairCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

const CAP: Cap = Cap::Thread {
    time: std::time::Duration::from_secs(30),
    stack: 4 << 20,
};

#[test]
fn changes_between_random_texts_apply_and_revert() {
    run("changes", cases(300), CAP, generate, check);
}

/// A large text with a few changes, at a size: its lines, and the changes
/// at the same fractions of it.
#[derive(Clone, Debug)]
pub(super) struct CostCase {
    /// Changes, each at a fraction (per thousand) of the text: an insert,
    /// a delete or a change, by `kind % 3`.
    pub(super) changes: Vec<(u16, u8)>,
    /// Whether every line is the same (no line holds once).
    pub(super) alike: bool,
}

impl Case for CostCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn diff_cost_repro() {{\n    super::changes::replay_cost(super::changes::CostCase {{\n        changes: vec!{:?},\n        alike: {},\n    }});\n}}",
            self.changes, self.alike,
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out: Vec<Self> = (0..self.changes.len())
            .map(|index| {
                let mut changes = self.changes.clone();
                changes.remove(index);
                Self {
                    changes,
                    alike: self.alike,
                }
            })
            .collect();
        if self.alike {
            out.push(Self {
                alike: false,
                ..self.clone()
            });
        }
        out
    }

    fn size(&self) -> usize {
        self.changes.len() * 2 + usize::from(self.alike)
    }
}

fn generate_cost(rng: &mut Rng) -> Option<CostCase> {
    Some(CostCase {
        changes: (0..1 + rng.below(8))
            .map(|_| (rng.below(1000) as u16, rng.below(3) as u8))
            .collect(),
        alike: rng.chance(30),
    })
}

/// The case's two texts at `lines` lines.
fn cost_texts(case: &CostCase, lines: usize) -> (String, String) {
    let old: Vec<String> = (0..lines)
        .map(|line| {
            if case.alike {
                format!("k: {}", line % 3)
            } else {
                format!("k{line}: {line}")
            }
        })
        .collect();
    let mut new = old.clone();
    let mut places: Vec<(usize, u8)> = case
        .changes
        .iter()
        .map(|(at, kind)| (*at as usize * lines / 1000, *kind))
        .collect();
    places.sort_unstable();
    for (at, kind) in places.into_iter().rev() {
        let at = at.min(new.len().saturating_sub(1));
        match kind % 3 {
            0 => new.insert(at, "new".into()),
            1 => {
                new.remove(at);
            }
            _ => new[at] = "changed".into(),
        }
    }
    (old.join("\n") + "\n", new.join("\n") + "\n")
}

/// The diff's search at 1, 2 and 4 times this many lines.
const COST_LINES: usize = 3000;

/// Counts this small are compared as if this large: the exact search of
/// a small changed region (a table of up to a million cells, and the
/// frontier of a capped depth) is bounded, not growth.
const COST_FLOOR: usize = 2_000_000;

pub(super) fn check_cost(case: &CostCase) -> Result<(), Failure> {
    let mut costs = Vec::new();
    for size in [1, 2, 4] {
        let (old, new) = cost_texts(case, COST_LINES * size);
        let before = steps();
        let _ = diff_lines(&old, &new);
        let _ = unified_diff(&old, &new, 3);
        costs.push(steps() - before);
    }
    for pair in [0, 1] {
        if costs[pair + 1] * 2 > 5 * costs[pair].max(COST_FLOOR) {
            return Err(Failure::new(
                format!(
                    "the diff's search grows faster than the text (lines alike: {})",
                    case.alike
                ),
                format!("{costs:?} steps at {COST_LINES}, twice and four times the lines"),
            ));
        }
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay_cost(case: CostCase) {
    if let Err(failure) = check_cost(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn the_diff_of_a_large_text_with_few_changes_grows_with_the_text() {
    run("diff-cost", cases(16), CAP, generate_cost, check_cost);
}
