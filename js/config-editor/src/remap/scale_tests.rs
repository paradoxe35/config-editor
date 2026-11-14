//! Ranges moved between texts that differ in many places (QA 31: past
//! ~512 diff steps every range between the first and the last change grew
//! over all of it), and the diff's work counted: linear in the texts for a
//! few changes, and bounded per line with many.
use config_editor_leptos::{TextPosition, TextRange};

use super::Remap;
use super::stretches::line_runs::WORK;

fn rng(mut seed: u64) -> impl FnMut(usize) -> usize {
    move |bound: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % bound.max(1) as u64) as usize
    }
}

/// Line `number` (one-based) of `text`, whole, as a range.
fn line_range(text: &str, number: usize) -> TextRange {
    let line = text.split('\n').nth(number - 1).expect("a line");
    let columns = line.chars().count();
    TextRange::new(
        TextPosition::new(number, 1),
        TextPosition::new(number, columns + 1),
    )
}

/// Each of `kept` (`(line in from, line in to)`, both unchanged) maps to
/// its own text exactly.
fn assert_exact(from: &str, to: &str, kept: &[(usize, usize)]) {
    let remap = Remap::new(from, to);
    let mut wrong = Vec::new();
    for (from_line, to_line) in kept {
        let range = line_range(from, *from_line);
        let mapped = remap.range(range);
        if mapped != line_range(to, *to_line) {
            wrong.push(format!(
                "line {from_line} -> {to_line}: {mapped:?} ({:?})",
                mapped.slice(to).chars().take(60).collect::<String>()
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} lines moved wrong, first: {:?}",
        wrong.len(),
        kept.len(),
        &wrong[..wrong.len().min(3)]
    );
}

/// `count` services whose `image` line (the same in each) is replaced in
/// every one, as Replace All does: every other line maps exactly, and an
/// image line's range stays on its own line.
#[test]
fn a_replace_all_over_many_lines_moves_every_other_line_exactly() {
    for count in [300, 1_000, 5_000] {
        let service = |index: usize, version: &str| {
            format!(
                "  svc{index}:\n    image: nginx:{version}\n    port: {}\n",
                8000 + index
            )
        };
        let from: String = std::iter::once("services:\n".to_string())
            .chain((0..count).map(|index| service(index, "1.0")))
            .collect();
        let to = from.replace("nginx:1.0", "nginx:1.1");
        let kept: Vec<(usize, usize)> = (1..=from.split('\n').count())
            .filter(|line| !from.split('\n').nth(line - 1).unwrap().contains("image"))
            .map(|line| (line, line))
            .collect();
        assert_exact(&from, &to, &kept);
        let remap = Remap::new(&from, &to);
        for index in [0, count / 2, count - 1] {
            let line = 3 + 3 * index;
            let mapped = remap.range(line_range(&from, line));
            assert_eq!(
                (mapped.start.line, mapped.end.line),
                (line, line),
                "{count}: image line {line}"
            );
        }
    }
}

/// Every other line of a long text changed (each line written once): every
/// line left as it was maps exactly, at 300, 1000 and 5000 changes.
#[test]
fn many_separate_changes_leave_every_unchanged_line_exact() {
    for changes in [300, 1_000, 5_000] {
        let lines = 2 * changes + 10;
        let from: String = (0..lines).map(|line| format!("key{line}: 1\n")).collect();
        let to: String = (0..lines)
            .map(|line| match line % 2 {
                1 if line > 10 => format!("key{line}: 2\n"),
                _ => format!("key{line}: 1\n"),
            })
            .collect();
        let kept: Vec<(usize, usize)> = (0..lines)
            .filter(|line| line % 2 == 0 || *line <= 10)
            .map(|line| (line + 1, line + 1))
            .collect();
        assert_exact(&from, &to, &kept);
    }
}

/// A paste of 600 lines and an edit far below it: every other line maps
/// exactly, those between the two included.
#[test]
fn a_long_paste_and_another_edit_leave_the_lines_between_exact() {
    let from: String = (0..2_000).map(|line| format!("line {line}\n")).collect();
    let mut to_lines: Vec<String> = (0..2_000).map(|line| format!("line {line}\n")).collect();
    to_lines[1_500] = "line 1500 edited\n".to_string();
    let pasted: Vec<String> = (0..600).map(|line| format!("pasted {line}\n")).collect();
    to_lines.splice(100..100, pasted);
    let to: String = to_lines.concat();
    let kept: Vec<(usize, usize)> = (0..2_000)
        .filter(|line| *line != 1_500)
        .map(|line| (line + 1, line + 1 + if line >= 100 { 600 } else { 0 }))
        .collect();
    assert_exact(&from, &to, &kept);
}

/// A text of lines each written once, after random line edits (replaced,
/// inserted, deleted: up to a few thousand): every line kept maps exactly.
#[test]
fn random_line_edits_leave_every_kept_line_exact() {
    let mut next = rng(0x9e37_79b9_7f4a_7c15);
    for round in 0..40 {
        let lines = 200 + next(4_000);
        let from: Vec<String> = (0..lines).map(|line| format!("l{line} é😀\n")).collect();
        // Each line of `to`: the line of `from` it is, or new.
        let mut to: Vec<(String, Option<usize>)> = from
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, line)| (line, Some(index)))
            .collect();
        let mut fresh = 0;
        for _ in 0..1 + next(if round % 2 == 0 { 50 } else { 2_000 }) {
            let at = next(to.len());
            fresh += 1;
            match next(3) {
                0 => to[at] = (format!("new{fresh}\n"), None),
                1 => to.insert(at, (format!("new{fresh}\n"), None)),
                _ if to.len() > 1 => {
                    to.remove(at);
                }
                _ => {}
            }
        }
        let kept: Vec<(usize, usize)> = to
            .iter()
            .enumerate()
            .filter_map(|(at, (_, line))| line.map(|line| (line + 1, at + 1)))
            .collect();
        let to: String = to.into_iter().map(|(line, _)| line).collect();
        assert_exact(&from.concat(), &to, &kept);
    }
}

/// The diff's work (lines compared or hashed) for `from` and `to`.
fn work(from: &str, to: &str) -> usize {
    WORK.with(|work| work.set(0));
    let _ = Remap::new(from, to);
    WORK.with(std::cell::Cell::get)
}

/// A long text with `changes` lines changed, spread evenly.
fn changed(lines: usize, changes: usize, repeated: bool) -> (String, String) {
    let line = |index: usize| {
        if repeated {
            "  }\n".to_string()
        } else {
            format!("key{index}: 1\n")
        }
    };
    let from: String = (0..lines).map(line).collect();
    let every = lines / (changes + 1);
    let to: String = (0..lines)
        .map(|index| {
            if index % every == every / 2 && index / every < changes {
                format!("changed{index}\n")
            } else {
                line(index)
            }
        })
        .collect();
    (from, to)
}

/// A few changes: the work grows with the text, no faster. Many changes,
/// or lines all alike: a bounded amount of work per line.
#[test]
fn the_diff_works_in_time_linear_in_the_text() {
    let small = work(&changed(2_000, 10, false).0, &changed(2_000, 10, false).1);
    let large = work(&changed(40_000, 10, false).0, &changed(40_000, 10, false).1);
    assert!(
        large <= 30 * small,
        "20× the lines took {large} work against {small}"
    );
    for (lines, changes, repeated, per_line) in [
        (20_000, 5_000, false, 10),
        (20_000, 600, false, 10),
        (20_000, 300, true, 30),
        (20_000, 5_000, true, 30),
    ] {
        let (from, to) = changed(lines, changes, repeated);
        let done = work(&from, &to);
        assert!(
            done <= per_line * lines,
            "{changes} changes in {lines} lines (repeated: {repeated}): {done} work"
        );
    }
}
