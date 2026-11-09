//! The projection, folds through edits, and the language-free building
//! blocks. The per-language ranges are in `languages/fold_matrix.rs`.

use super::super::fold_ranges::*;
use super::super::projection::{Projection, Side};
use super::follow::{follow_edit, keep_valid};
use crate::language::LanguageConfig;

fn fold(start: usize, end: usize) -> FoldRange {
    FoldRange::new(start, end)
}

/// The document without the lines `folds` hide, written the slow way.
fn naive_view(doc: &str, folds: &[FoldRange]) -> String {
    let lines: Vec<&str> = doc.split('\n').collect();
    let hidden = |index: usize| folds.iter().any(|fold| fold.hides(index));
    lines
        .iter()
        .enumerate()
        .filter(|(index, _)| !hidden(*index))
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// A small deterministic generator, so failures reproduce.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % below.max(1)
    }
}

const DOC: &str = "a:\n  b: 1\n  c: 2\nd:\n  e: é\n  f: 😀\ng: 3";

#[test]
fn nothing_folded_shows_the_document_itself() {
    let projection = Projection::new(DOC, &[]);
    assert_eq!(projection.view(DOC), DOC);
    assert_eq!(projection.rows(), 7);
    assert_eq!(projection.to_view(5), 5);
    assert_eq!(projection.line_of_row(4), 4);
}

#[test]
fn a_fold_cuts_its_lines_out_and_maps_offsets_and_rows() {
    let projection = Projection::new(DOC, &[fold(0, 2)]);
    assert_eq!(projection.view(DOC), "a:\nd:\n  e: é\n  f: 😀\ng: 3");
    assert_eq!(projection.rows(), 5);
    // The end of `a:` (offset 2) stays; the hidden text maps there.
    assert_eq!(projection.to_view(2), 2);
    assert_eq!(projection.to_view(5), 2);
    assert_eq!(projection.to_view(16), 2);
    // `d:` starts at 17 in the document, 3 on screen.
    assert_eq!(projection.to_view(17), 3);
    assert_eq!(projection.to_doc(3, Side::Before), 17);
    assert_eq!(projection.to_doc(2, Side::Before), 2);
    assert_eq!(projection.to_doc(2, Side::After), 16);
    assert_eq!(projection.row_of_line(0), 0);
    assert_eq!(projection.row_of_line(1), 0);
    assert_eq!(projection.row_of_line(3), 1);
    assert_eq!(projection.line_of_row(1), 3);
    assert!(projection.is_hidden(2) && !projection.is_hidden(3));
    assert_eq!(projection.hidden_at(5), Some((2, 16)));
    assert_eq!(projection.hidden_at(2), None);
    assert!(projection.hides_within(0, 4));
    assert!(projection.hides_within(2, 3));
    assert!(!projection.hides_within(17, 20));
}

#[test]
fn nested_and_touching_folds_hide_one_run() {
    let doc = "0\n1\n2\n3\n4\n5\n6";
    let projection = Projection::new(doc, &[fold(0, 2), fold(1, 2), fold(3, 4)]);
    assert_eq!(projection.view(doc), "0\n3\n5\n6");
    // A fold whose first line is hidden by another continues its run.
    let projection = Projection::new(doc, &[fold(0, 2), fold(2, 4)]);
    assert_eq!(projection.view(doc), "0\n5\n6");
    assert_eq!(projection.line_of_row(1), 5);
}

/// Round trips over random documents and folds: rows, offsets and the
/// view agree with the slow way.
#[test]
fn the_projection_agrees_with_the_slow_way() {
    let mut random = Lcg(7);
    let mut failures = Vec::new();
    for case in 0..300 {
        let count = 1 + random.next(12);
        let lines: Vec<String> = (0..count)
            .map(|_| ["", "x", "é", "😀b", "  key: v"][random.next(5)].to_string())
            .collect();
        let doc = lines.join("\n");
        let folds: Vec<FoldRange> = (0..random.next(4))
            .map(|_| {
                let start = random.next(count);
                fold(start, start + 1 + random.next(3))
            })
            .filter(|fold| fold.end < count)
            .collect();
        let projection = Projection::new(&doc, &folds);
        let view = projection.view(&doc).to_string();
        if view != naive_view(&doc, &folds) {
            failures.push(format!("{case}: view of {doc:?} with {folds:?}"));
            continue;
        }
        let mut row = 0;
        for line in 0..count {
            if projection.is_hidden(line) {
                continue;
            }
            if projection.row_of_line(line) != row || projection.line_of_row(row) != line {
                failures.push(format!(
                    "{case}: line {line} row {row} in {doc:?} {folds:?}"
                ));
            }
            row += 1;
        }
        if row != projection.rows() {
            failures.push(format!("{case}: {row} rows, said {}", projection.rows()));
        }
        // Every visible document offset maps there and back.
        let total = doc.encode_utf16().count();
        for offset in 0..=total {
            if projection.hidden_at(offset).is_none() {
                let back = projection.to_doc(projection.to_view(offset), Side::Before);
                if back != offset {
                    failures.push(format!("{case}: offset {offset} came back {back}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// An edit the browser made on screen lands in the document without
/// taking the hidden lines with it.
#[test]
fn edits_on_screen_map_into_the_document() {
    let projection = Projection::new(DOC, &[fold(0, 2)]);
    let view = projection.view(DOC).to_string();
    let cases = [
        // Typing in the folded line: the hidden lines follow it.
        ("a:", "aa:", "aa:\n  b: 1\n  c: 2\nd:"),
        // At its end: still before the hidden lines.
        ("a:\nd", "a: x\nd", "a: x\n  b: 1\n  c: 2\nd"),
        // Deleting the line break after the fold joins its last line.
        ("a:\nd:", "a:d:", "a:\n  b: 1\n  c: 2d:"),
        // Typing on the line after the fold.
        ("\nd:", "\nd:!", "\n  c: 2\nd:!"),
    ];
    let mut failures = Vec::new();
    for (from, to, expected) in cases {
        let edited = view.replacen(from, to, 1);
        let doc = projection.unproject(DOC, &edited);
        if !doc.contains(expected) {
            failures.push(format!("{from:?} -> {to:?}: {doc:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn folds_follow_edits_and_open_when_touched() {
    let old = "a:\n  b: 1\nc:\n  d: 2\ne:\n  f: 3";
    let folds = [fold(0, 1), fold(2, 3), fold(4, 5)];
    let cases: [(&str, &str, &[FoldRange]); 6] = [
        // A line added before the middle fold moves the later ones.
        ("c:\n", "x: 0\nc:\n", &[fold(0, 1), fold(3, 4), fold(5, 6)]),
        // Typing in the folded line before its end keeps it.
        ("c:", "cc:", &[fold(0, 1), fold(2, 3), fold(4, 5)]),
        // At its end, the fold opens.
        ("c:\n", "c: \n", &[fold(0, 1), fold(4, 5)]),
        // Inside the hidden lines: opens.
        ("d: 2", "d: 22", &[fold(0, 1), fold(4, 5)]),
        // The line break after the fold's last line: opens it, and the
        // next fold moves up.
        ("2\ne", "2e", &[fold(0, 1), fold(3, 4)]),
        // Removing a line before the last fold moves it up.
        ("  d: 2\n", "", &[fold(0, 1), fold(3, 4)]),
    ];
    let mut failures = Vec::new();
    for (from, to, expected) in cases {
        let new = old.replacen(from, to, 1);
        let moved = follow_edit(&folds, old, &new);
        if moved != expected {
            failures.push(format!(
                "{from:?} -> {to:?}: {moved:?}, expected {expected:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_fold_whose_structure_is_gone_goes() {
    let ranges = [fold(0, 3), fold(1, 2), fold(5, 9)];
    assert_eq!(
        keep_valid(&[fold(1, 2), fold(4, 6), fold(5, 7)], &ranges),
        [fold(1, 2), fold(5, 9)]
    );
}

#[test]
fn indentation_folds_over_more_indented_lines() {
    let source = "a:\n  b:\n    c: 1\n\n  # note\n  d: 2\n# end\ne: 3\n  f";
    let ranges = indent_fold_ranges(
        source,
        |_, line| {
            if line.trim_start().starts_with('#') {
                IndentLine::Skip
            } else {
                IndentLine::Code
            }
        },
        |_, _| false,
    );
    let mut ranges = merge_fold_ranges(ranges, 9);
    ranges.sort();
    assert_eq!(ranges, [fold(0, 5), fold(1, 2), fold(7, 8)]);
}

const BRACES: LanguageConfig = LanguageConfig {
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    brackets: &[('{', '}'), ('[', ']')],
    separators: &[],
    quotes: &["\""],
    literal_quotes: &[],
    map_brackets: None,
    list_brackets: None,
    item_separator: None,
    item_marker: None,
    written_separator: None,
};

#[test]
fn brackets_fold_with_their_closer_visible_and_skip_strings_and_comments() {
    let cases: [(&str, &[FoldRange]); 7] = [
        ("{\n  a\n}", &[fold(0, 1)]),
        // A closer after other text on its line is hidden with it.
        ("[\n  1,\n  2]", &[fold(0, 2)]),
        // Empty and one-line containers fold nothing.
        ("{}\n[\n]\n{ a }", &[]),
        // Brackets in strings and comments are text.
        ("{\n  \"}\"\n  // }\n}", &[fold(0, 2)]),
        // Unterminated: nothing for the open one, its closed child folds.
        ("{\n  [\n    1\n  ]\n", &[fold(1, 2)]),
        // A block comment over lines folds whole.
        ("/* a\n b\n*/\nx", &[fold(0, 2)]),
        // A string the line does not close ends there.
        ("\"{\n{\n}", &[]),
    ];
    let mut failures = Vec::new();
    for (source, expected) in cases {
        let ranges = merge_fold_ranges(
            bracket_fold_ranges(source, &BRACES, &[]),
            source.split('\n').count(),
        );
        if ranges != expected {
            failures.push(format!("{source:?}: {ranges:?}, expected {expected:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn comment_runs_and_string_bodies_fold() {
    assert_eq!(
        comment_fold_ranges("# a\n# b\nx\n# c\n  # d\n  # e", &["#"], &[]),
        [fold(0, 1), fold(3, 5)]
    );
    // A body line is text, not a comment.
    assert_eq!(
        comment_fold_ranges("# a\n# b", &["#"], std::slice::from_ref(&(1..2))),
        []
    );
    assert_eq!(
        string_body_fold_ranges(&[(2..5), (0..1), (7..7)]),
        [fold(1, 4)]
    );
}

#[test]
fn the_default_folds_indentation_brackets_and_comments() {
    let config = LanguageConfig {
        line_comments: &[";"],
        brackets: &[('[', ']')],
        ..LanguageConfig::default()
    };
    let source = "; a\n; b\n[s]\nk = [\n  1\n]\nouter\n  inner";
    assert_eq!(
        default_fold_ranges(source, &config, &[]),
        [fold(0, 1), fold(3, 4), fold(6, 7)]
    );
}

/// QA 20: a closed fold's chip told a hidden problem only by its colour;
/// its label and title name the most serious one.
#[test]
fn a_fold_chip_names_the_most_serious_problem_it_hides() {
    use super::texts::chip_label;
    use crate::{EditorStrings, Severity};
    let strings = EditorStrings::default();
    let cases = [
        (None, "Unfold lines 3–7"),
        (
            Some((Severity::Error, 1)),
            "Unfold lines 3–7, hides a problem: Error",
        ),
        (
            Some((Severity::Warning, 1)),
            "Unfold lines 3–7, hides a problem: Warning",
        ),
        (
            Some((Severity::Info, 1)),
            "Unfold lines 3–7, hides a problem: Info",
        ),
        (
            Some((Severity::Hint, 1)),
            "Unfold lines 3–7, hides a problem: Hint",
        ),
        // QA 21: two hidden problems were worded as one.
        (
            Some((Severity::Error, 2)),
            "Unfold lines 3–7, hides 2 problems, the most serious: Error",
        ),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .map(|(hidden, expected)| (chip_label(&strings, 2, 6, hidden), expected))
        .filter(|(found, expected)| found != expected)
        .map(|(found, expected)| format!("expected {expected:?}, found {found:?}"))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
