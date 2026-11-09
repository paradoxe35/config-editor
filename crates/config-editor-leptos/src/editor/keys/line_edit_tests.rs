//! The line commands' model: lines moved, copied and deleted, the caret
//! and selection then, the text's last newline kept, and closed folds
//! taken whole (and kept closed).

use super::super::fold_ranges::FoldRange;
use super::line_edit::{Folded, LineCommand, LineEdit, line_edit};

const UP: LineCommand = LineCommand::Move { down: false };
const DOWN: LineCommand = LineCommand::Move { down: true };
const COPY_UP: LineCommand = LineCommand::Copy { down: false };
const COPY_DOWN: LineCommand = LineCommand::Copy { down: true };
const DELETE: LineCommand = LineCommand::Delete;

/// The text and selection (`|` marks the caret, `[` `]` a selection)
/// after `command` on `marked`, folds `folds` closed going as `units`; and
/// the folds then.
fn run_units(
    marked: &str,
    command: LineCommand,
    folds: &[FoldRange],
    units: &[FoldRange],
) -> Option<(String, Vec<FoldRange>)> {
    let length = |text: &str| text.encode_utf16().count();
    let (start, end) = match marked.find('|') {
        Some(at) => (length(&marked[..at]), length(&marked[..at])),
        None => (
            length(&marked[..marked.find('[').unwrap()]),
            length(&marked[..marked.find(']').unwrap()]) - 1,
        ),
    };
    let text: String = marked.chars().filter(|c| !"|[]".contains(*c)).collect();
    let LineEdit {
        text,
        selection,
        mut folds,
    } = line_edit(&text, (start, end), command, Folded { folds, units })?;
    let byte = |offset: usize| {
        text.char_indices()
            .scan(0, |units, (at, character)| {
                let here = *units;
                *units += character.len_utf16();
                Some((at, here))
            })
            .find(|(_, units)| *units == offset)
            .map_or(text.len(), |(at, _)| at)
    };
    let mut out = text.clone();
    if selection.0 == selection.1 {
        out.insert(byte(selection.0), '|');
    } else {
        out.insert(byte(selection.1), ']');
        out.insert(byte(selection.0), '[');
    }
    folds.sort_unstable();
    Some((out, folds))
}

/// [`run_units`] with each fold going as its own lines.
fn run_folded(
    marked: &str,
    command: LineCommand,
    folds: &[FoldRange],
) -> Option<(String, Vec<FoldRange>)> {
    run_units(marked, command, folds, folds)
}

fn run(marked: &str, command: LineCommand) -> Option<String> {
    run_folded(marked, command, &[]).map(|(text, _)| text)
}

#[test]
fn alt_arrows_move_the_lines() {
    assert_eq!(
        run("a: 1\nb: |2\nc: 3\n", UP).unwrap(),
        "b: |2\na: 1\nc: 3\n"
    );
    assert_eq!(
        run("a: 1\nb: |2\nc: 3\n", DOWN).unwrap(),
        "a: 1\nc: 3\nb: |2\n"
    );
    assert_eq!(run("a: |1\nb: 2\n", UP), None);
    assert_eq!(run("a: 1\nb: |2\n", DOWN), None);
    // The last line without a newline: the text still has none.
    assert_eq!(run("a: 1\nb: |2", UP).unwrap(), "b: |2\na: 1");
    assert_eq!(run("a: |1\nb: 2", DOWN).unwrap(), "b: 2\na: |1");
    // Selected lines move together; a selection ending at a line's start
    // leaves that line.
    assert_eq!(run("x\n[a\nb\n]c\n", DOWN).unwrap(), "x\nc\n[a\nb\n]");
    assert_eq!(run("x\n[a\nb]\nc\n", UP).unwrap(), "[a\nb]\nx\nc\n");
    // Not ASCII: columns are UTF-16.
    assert_eq!(run("é: 1\nö: «|x»\n", UP).unwrap(), "ö: «|x»\né: 1\n");
}

#[test]
fn shift_alt_arrows_copy_the_lines() {
    assert_eq!(
        run("a: |1\nb: 2\n", COPY_DOWN).unwrap(),
        "a: 1\na: |1\nb: 2\n"
    );
    assert_eq!(
        run("a: |1\nb: 2\n", COPY_UP).unwrap(),
        "a: |1\na: 1\nb: 2\n"
    );
    assert_eq!(run("a: 1\nb: |2", COPY_DOWN).unwrap(), "a: 1\nb: 2\nb: |2");
    assert_eq!(run("[a\nb]\nc\n", COPY_DOWN).unwrap(), "a\nb\n[a\nb]\nc\n");
}

#[test]
fn ctrl_shift_k_deletes_the_lines_keeping_the_column() {
    assert_eq!(run("a: 1\nb: |2\nc: 3\n", DELETE).unwrap(), "a: 1\nc: |3\n");
    // Clamped to the line taking the place.
    assert_eq!(run("a: 1\nbbbb: |2\nc\n", DELETE).unwrap(), "a: 1\nc|\n");
    // The last line: the caret goes to the line above, the text keeps
    // its last newline (or its lack of one).
    assert_eq!(run("a: 1\nb: |2", DELETE).unwrap(), "a: |1");
    assert_eq!(run("a: 1\nb: |2\n", DELETE).unwrap(), "a: 1\n|");
    assert_eq!(run("[a\nb]\nc\n", DELETE).unwrap(), "|c\n");
    assert_eq!(run("a|\n", DELETE).unwrap(), "|");
    assert_eq!(run("|", DELETE), None);
}

/// The empty line after the text's last newline is where the text ends: a
/// copy of it adds one line, a move or a delete changes nothing.
#[test]
fn the_empty_line_after_the_last_newline() {
    assert_eq!(run("a: 1\nb: 2\n|", COPY_DOWN).unwrap(), "a: 1\nb: 2\n\n|");
    assert_eq!(run("a: 1\nb: 2\n|", COPY_UP).unwrap(), "a: 1\nb: 2\n|\n");
    assert_eq!(run("a: 1\nb: 2\n|", UP), None);
    assert_eq!(run("a: 1\nb: 2\n|", DOWN), None);
    assert_eq!(run("a: 1\nb: 2\n|", DELETE), None);
}

/// YAML-like text: `m` (line 1) folds its two children (lines 2, 3).
const FOLDED: &str = "a: 1\nm:\n  x: 1\n  y: 2\nz: 3\n";

fn fold(start: usize, end: usize) -> FoldRange {
    FoldRange::new(start, end)
}

#[test]
fn a_folded_line_moves_whole_and_stays_folded() {
    let folds = [fold(1, 3)];
    assert_eq!(
        run_folded("a: 1\nm:|\n  x: 1\n  y: 2\nz: 3\n", UP, &folds).unwrap(),
        ("m:|\n  x: 1\n  y: 2\na: 1\nz: 3\n".into(), vec![fold(0, 2)])
    );
    assert_eq!(
        run_folded("a: 1\nm:|\n  x: 1\n  y: 2\nz: 3\n", DOWN, &folds).unwrap(),
        ("a: 1\nz: 3\nm:|\n  x: 1\n  y: 2\n".into(), vec![fold(2, 4)])
    );
    assert_eq!(FOLDED.lines().count(), 5);
}

#[test]
fn a_line_moves_past_a_fold_whole() {
    let folds = [fold(1, 3)];
    assert_eq!(
        run_folded("a: |1\nm:\n  x: 1\n  y: 2\nz: 3\n", DOWN, &folds).unwrap(),
        ("m:\n  x: 1\n  y: 2\na: |1\nz: 3\n".into(), vec![fold(0, 2)])
    );
    assert_eq!(
        run_folded("a: 1\nm:\n  x: 1\n  y: 2\nz: |3\n", UP, &folds).unwrap(),
        ("a: 1\nz: |3\nm:\n  x: 1\n  y: 2\n".into(), vec![fold(2, 4)])
    );
    // Nested closed folds go as one.
    let nested = "a: |1\nm:\n  n:\n    x: 1\n  y: 2\nz: 3\n";
    assert_eq!(
        run_folded(nested, DOWN, &[fold(1, 4), fold(2, 3)]).unwrap(),
        (
            "m:\n  n:\n    x: 1\n  y: 2\na: |1\nz: 3\n".into(),
            vec![fold(0, 3), fold(1, 2)]
        )
    );
}

#[test]
fn copy_delete_of_a_folded_line_take_its_hidden_lines() {
    let folds = [fold(1, 3), fold(5, 6)];
    let text = "a: 1\nm:|\n  x: 1\n  y: 2\nz: 3\nw:\n  v: 1\n";
    assert_eq!(
        run_folded(text, COPY_DOWN, &folds).unwrap(),
        (
            "a: 1\nm:\n  x: 1\n  y: 2\nm:|\n  x: 1\n  y: 2\nz: 3\nw:\n  v: 1\n".into(),
            vec![fold(1, 3), fold(4, 6), fold(8, 9)]
        )
    );
    assert_eq!(
        run_folded(text, COPY_UP, &folds).unwrap(),
        (
            "a: 1\nm:|\n  x: 1\n  y: 2\nm:\n  x: 1\n  y: 2\nz: 3\nw:\n  v: 1\n".into(),
            vec![fold(1, 3), fold(4, 6), fold(8, 9)]
        )
    );
    assert_eq!(
        run_folded(text, DELETE, &folds).unwrap(),
        ("a: 1\nz:| 3\nw:\n  v: 1\n".into(), vec![fold(2, 3)])
    );
    // A selection from above through a folded line takes the fold too.
    assert_eq!(
        run_folded("[a: 1\nm:]\n  x: 1\n  y: 2\nz: 3\n", DELETE, &[fold(1, 3)]).unwrap(),
        ("|z: 3\n".into(), vec![])
    );
}

/// A fold whose construct closes on a line it shows (`}`, an end tag):
/// the folded line goes with that line too, as a cut of it takes it.
#[test]
fn a_folded_line_takes_the_line_closing_it() {
    // `b {` (line 1) folds line 2; `}` (line 3) closes it.
    let text = "a = 1\nb {|\n  c = 2\n}\nd = 3\n";
    let (folds, units) = ([fold(1, 2)], [fold(1, 3)]);
    assert_eq!(
        run_units(text, DOWN, &folds, &units).unwrap(),
        ("a = 1\nd = 3\nb {|\n  c = 2\n}\n".into(), vec![fold(2, 3)])
    );
    assert_eq!(
        run_units(text, DELETE, &folds, &units).unwrap(),
        ("a = 1\nd =| 3\n".into(), vec![])
    );
    assert_eq!(
        run_units("a = |1\nb {\n  c = 2\n}\nd = 3\n", DOWN, &folds, &units).unwrap(),
        ("b {\n  c = 2\n}\na = |1\nd = 3\n".into(), vec![fold(0, 1)])
    );
    assert_eq!(
        run_units("a = 1\nb {\n  c = 2\n}\nd = |3\n", UP, &folds, &units).unwrap(),
        ("a = 1\nd = |3\nb {\n  c = 2\n}\n".into(), vec![fold(2, 3)])
    );
    // The closing line alone is a line of its own.
    assert_eq!(
        run_units("a = 1\nb {\n  c = 2\n}|\nd = 3\n", DOWN, &folds, &units).unwrap(),
        ("a = 1\nb {\n  c = 2\nd = 3\n}|\n".into(), vec![fold(1, 2)])
    );
}

/// The lines each fold goes as, by the language's constructs.
#[cfg(all(feature = "yaml", feature = "json", feature = "xml"))]
#[test]
fn fold_units_reach_the_closing_line() {
    use super::super::languages::Language;
    use super::line_edit::fold_units;
    let cases = [
        (Language::YAML, "a:\n  b: 1\nc: 2\n", fold(0, 1), fold(0, 1)),
        (
            Language::JSON,
            "{\n  \"a\": {\n    \"b\": 1\n  },\n  \"c\": 2\n}\n",
            fold(1, 2),
            fold(1, 3),
        ),
        (
            Language::XML,
            "<r>\n  <a>\n    <b>1</b>\n  </a>\n</r>\n",
            fold(1, 2),
            fold(1, 3),
        ),
    ];
    for (language, text, folded, unit) in cases {
        assert_eq!(
            fold_units(text, &[folded], language.behavior()),
            vec![unit],
            "{language}"
        );
    }
}
