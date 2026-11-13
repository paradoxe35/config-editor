use super::*;
use crate::editor::highlight::runs_of;
use crate::editor::languages::Language;
use crate::editor::work_count::counted;

/// A long line of each language: one value holding `length` bytes.
fn long_line(language: Language, length: usize) -> String {
    let value = "a".repeat(length);
    match language.label() {
        "JSON" => format!("{{\"k\": \"{value}\"}}"),
        "XML" => format!("<k>{value}</k>"),
        "TOML" | "HCL" => format!("k = \"{value}\""),
        _ => format!("k: \"{value}\""),
    }
}

/// The pieces of `line` after `previous`, as the code layer asks for them.
fn next(previous: Option<RowPieces>, line: &str, marks: &[Mark], language: Language) -> RowPieces {
    let runs = runs_of(line, &language.behavior().highlight(line));
    let (pieces, _) = RowPieces::next(previous, line, &runs, marks);
    check(&pieces, line, &runs, marks);
    pieces
}

/// The pieces draw `line` as it draws whole: each is what its range of the
/// line draws, cut where a piece may start.
fn check(pieces: &RowPieces, line: &str, runs: &[Run], marks: &[Mark]) {
    let mut at = 0;
    for piece in pieces.pieces() {
        assert_eq!(piece.start, at);
        let end = at + piece.content.text.len();
        assert!(at == 0 || can_cut(line, at, marks), "a cut at {at}");
        assert_eq!(*piece.content, piece_text(line, at..end, runs, marks));
        at = end;
    }
    assert_eq!(at, line.len());
}

/// The bytes built into new pieces by typing `typed` at byte `at`.
fn built_by_typing(language: Language, length: usize, at: fn(&str) -> usize, typed: &str) -> usize {
    let mut line = long_line(language, length);
    let mut pieces = next(None, &line, &[], language);
    let mut built = 0;
    for character in typed.chars() {
        line.insert(at(&line), character);
        let (after, bytes) = counted(Work::RowText, || next(Some(pieces), &line, &[], language));
        pieces = after;
        built = built.max(bytes);
    }
    built
}

/// Where the value's text ends (before its closing quote or tag).
fn value_end(line: &str) -> usize {
    line.rfind('a').expect("a value") + 1
}

fn middle(line: &str) -> usize {
    line.len() / 2
}

fn value_start(line: &str) -> usize {
    line.find('a').expect("a value")
}

#[test]
fn a_keystroke_in_a_long_row_builds_the_pieces_it_changed_not_the_row() {
    for &language in Language::ALL {
        for (place, at) in [
            ("end", value_end as fn(&str) -> usize),
            ("middle", middle),
            ("start", value_start),
        ] {
            let short = built_by_typing(language, 100_000, at, "xxxxx");
            let long = built_by_typing(language, 1_000_000, at, "xxxxx");
            let name = format!("{} at the value's {place}", language.label());
            assert!(short <= 2 * MAX_PIECE, "{name}: {short} bytes at 100k");
            assert!(long <= 2 * MAX_PIECE, "{name}: {long} bytes at 1M");
        }
    }
}

#[test]
fn a_row_typed_into_past_a_piece_s_size_is_cut_again() {
    // Any language the build has: the cut does not depend on it.
    let Some(&language) = Language::ALL.first() else {
        return;
    };
    let mut line = long_line(language, 20_000);
    let mut pieces = next(None, &line, &[], language);
    for _ in 0..3 * MAX_PIECE {
        line.insert(value_end(&line), 'x');
        pieces = next(Some(pieces), &line, &[], language);
    }
    let longest = pieces
        .pieces()
        .iter()
        .map(|piece| piece.content.text.len())
        .max();
    assert!(longest <= Some(MAX_PIECE), "{longest:?}");
}

#[test]
fn edits_anywhere_keep_the_pieces_true_to_the_line() {
    // Typing, deleting and pasting at random places, with marks, a line of
    // two-byte characters among ASCII ones, and highlighting that changes
    // past the edit (a quote typed): every piece draws what the line does.
    let mut seed = 7u64;
    let mut random = move |below: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as usize % below.max(1)
    };
    for &language in Language::ALL {
        let mut line = format!("k: \"{}\"", "aé, b".repeat(4_000));
        let marks: Vec<Mark> = vec![(100..9_000, "config-editor__locked")];
        let mut pieces = next(None, &line, &marks, language);
        for _ in 0..200 {
            let mut at = random(line.len());
            while !line.is_char_boundary(at) {
                at -= 1;
            }
            match random(4) {
                0 => line.insert(at, '"'),
                1 => line.insert_str(at, &"zz é".repeat(random(3_000))),
                _ => {
                    let mut end = (at + random(5_000)).min(line.len());
                    while !line.is_char_boundary(end) {
                        end -= 1;
                    }
                    line.replace_range(at..end, "");
                }
            }
            if !splits(&line) {
                line.push_str(&"q".repeat(LONG_ROW));
            }
            pieces = next(Some(pieces), &line, &marks, language);
        }
    }
}

#[test]
fn rows_that_would_draw_otherwise_in_pieces_are_drawn_whole() {
    assert!(splits(&"a".repeat(LONG_ROW + 1)));
    assert!(!splits(&"a".repeat(LONG_ROW)), "short");
    assert!(!splits(&format!("\t{}", "a".repeat(LONG_ROW))), "tab stops");
    assert!(
        !splits(&format!("שלום {}", "a".repeat(LONG_ROW))),
        "right to left"
    );
    assert!(
        !splits(&format!("\u{202E}{}", "a".repeat(LONG_ROW))),
        "an override"
    );
    assert!(
        splits(&format!("é 日本 {}", "a".repeat(LONG_ROW))),
        "left to right"
    );
}

#[test]
fn a_piece_starts_before_an_ascii_character_outside_the_marks() {
    let line = "aé\u{301}bcd";
    let marks: Vec<Mark> = vec![(3..6, "config-editor__secret")];
    let starts: Vec<usize> = (0..=line.len())
        .filter(|at| can_cut(line, *at, &marks))
        .collect();
    // Not inside `é`, not before its combining accent, not inside the mark.
    assert_eq!(starts, vec![6, 7]);
}
