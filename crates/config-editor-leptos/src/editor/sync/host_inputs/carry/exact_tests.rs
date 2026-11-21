//! A host's ranges carried by what the diff knows (a newer
//! version of the file locked every changed line after the lock, and a lock
//! whose line moved further down was dropped; the host-sync random test: a
//! lock sharing its line with other text lost its place, and the nearest
//! copy of its text was the wrong one of two equal values).
use crate::editor::paths::{TextPosition, TextRange};

use super::super::{carried, moved_between};
use super::{Carried, DIFFED, Exact};

const T: &str = "name: a\nsecret: keep\nlisten: 1\n";

/// The forms a lock on line `line` of `text` comes in: the line's text,
/// the line with its break (to the next line's start), and its value
/// (after `: `).
fn forms(text: &str, line: usize) -> Vec<(&'static str, TextRange)> {
    let content = text.split('\n').nth(line - 1).expect("a line");
    let width = content.chars().count();
    let value = content
        .find(": ")
        .map_or(1, |at| content[..at].chars().count() + 3);
    vec![
        (
            "line",
            TextRange::new(
                TextPosition::new(line, 1),
                TextPosition::new(line, width + 1),
            ),
        ),
        (
            "line and break",
            TextRange::new(TextPosition::new(line, 1), TextPosition::new(line + 1, 1)),
        ),
        (
            "value",
            TextRange::new(
                TextPosition::new(line, value),
                TextPosition::new(line, width + 1),
            ),
        ),
    ]
}

/// The lines (one-based) a range of `text` touches: its last line left
/// out when it ends at that line's start.
fn lines(range: TextRange) -> Vec<usize> {
    let end = if range.end.column == 1 && range.end.line > range.start.line {
        range.end.line - 1
    } else {
        range.end.line
    };
    (range.start.line..=end).collect()
}

/// Every reset kind: the text it gives, and the line the lock on `T`'s
/// line 2 (`secret: keep`) goes to there.
const RESETS: &[(&str, &str, usize)] = &[
    ("the same text", T, 2),
    (
        "a line inserted above",
        "z: 0\nname: a\nsecret: keep\nlisten: 1\n",
        3,
    ),
    (
        "two lines inserted above",
        "x: 1\ny: 2\nname: a\nsecret: keep\nlisten: 1\n",
        4,
    ),
    (
        "a line inserted below",
        "name: a\nsecret: keep\nnew: 1\nlisten: 1\n",
        2,
    ),
    (
        "the locked line changed",
        "name: a\nsecret: other\nlisten: 1\n",
        2,
    ),
    (
        "the locked line moved later",
        "name: a\nlisten: 1\nsecret: keep\n",
        3,
    ),
    (
        "the locked line moved earlier",
        "secret: keep\nname: a\nlisten: 1\n",
        1,
    ),
    (
        "the lines around it changed",
        "name: b\nsecret: keep\nlisten: 2\n",
        2,
    ),
    // The two alignments differ (`s` of `sub:`): its run's lines (0).
    (
        "the locked line indented",
        "name: a\nsub:\n  secret: keep\nlisten: 1\n",
        0,
    ),
    (
        "the locked line twice",
        "secret: keep\nname: a\nsecret: keep\nlisten: 1\n",
        3,
    ),
    // Nothing of it left: the whole lines of its run (line 0).
    ("unrelated text", "host: h\nport: 2\nmode: m\n", 0),
    ("the locked line removed", "name: a\nlisten: 1\n", 2),
    (
        "CRLF line breaks",
        "name: a\r\nsecret: keep\r\nlisten: 1\r\n",
        2,
    ),
];

/// Each lock lands on one line, the one holding its text when there is
/// one, its columns on that text: never dropped, never over another line.
#[test]
fn every_reset_kind_carries_each_lock_form_to_one_line() {
    for (kind, reset, line) in RESETS {
        let read = reset.replace("\r\n", "\n");
        if *line == 0 {
            // On its text where both alignments agree, else the whole
            // lines of its changed run, its text among them.
            for (form, lock) in forms(T, 2) {
                let range = carried(T, reset, &[lock])[0];
                let slice = range.slice(&read);
                if slice == lock.slice(T) {
                    continue;
                }
                assert!(slice.ends_with('\n'), "{kind}, {form}: {range:?}");
                if read.contains("secret: keep") {
                    assert!(slice.contains("secret: keep"), "{kind}, {form}: {slice:?}");
                }
            }
            continue;
        }
        let target = read.split('\n').nth(line - 1).expect("the line");
        for (form, lock) in forms(T, 2) {
            let carried = carried(T, reset, &[lock]);
            assert_eq!(carried.len(), 1, "{kind}, {form}");
            let range = carried[0];
            assert_eq!(lines(range), vec![*line], "{kind}, {form}: {range:?}");
            let slice = range.slice(&read);
            if target.trim() == "secret: keep" {
                let want = lock.slice(T).trim_start().to_string();
                assert_eq!(slice.trim_start(), want, "{kind}, {form}");
            } else {
                // Its line, with its break where locked whole.
                assert!(
                    !slice.trim_end_matches('\n').contains('\n'),
                    "{kind}, {form}"
                );
                assert!(
                    target.ends_with(slice.trim_end_matches('\n')),
                    "{kind}, {form}: {slice:?}"
                );
            }
        }
    }
}

/// A newer version of the file, every line changed: a lock on one line
/// stays on the line aligned with it, in each language's shape.
#[test]
fn a_newer_version_of_the_file_locks_only_the_aligned_line() {
    let cases = [
        (
            "# managed by ops v1\nname: a\nport: 1\n",
            "# managed by ops v2\nname: b\nport: 2\nmode: m\n",
            1,
            "# managed by ops v2",
        ),
        (
            "{\n  \"version\": 1,\n  \"name\": \"a\"\n}\n",
            "{\n  \"version\": 2,\n  \"name\": \"b\",\n  \"port\": 3\n}\n",
            2,
            "  \"version\": 2,",
        ),
        (
            "{\n  \"version\": 1,\n  \"name\": \"a\"\n}\n",
            "{\n  \"id\": 7,\n  \"version\": 2,\n  \"name\": \"b\"\n}\n",
            2,
            "  \"version\": 2,",
        ),
        (
            "version = 1\nname = \"a\"\n",
            "version = 2\nname = \"b\"\nport = 3\n",
            1,
            "version = 2",
        ),
    ];
    for (old, new, line, want) in cases {
        let width = old.split('\n').nth(line - 1).unwrap().chars().count();
        let lock = TextRange::new(
            TextPosition::new(line, 1),
            TextPosition::new(line, width + 1),
        );
        let carried = carried(old, new, &[lock]);
        // Its line; with a key added in the same changed run, where the
        // two alignments differ, the run's lines (over-locked, safe).
        let slice = carried[0].slice(new);
        assert!(slice.contains(want), "{old:?} -> {new:?}: {slice:?}");
    }
}

/// A lock over several lines keeps its lines where they stay together,
/// and an empty range stays empty.
#[test]
fn a_block_and_an_empty_range_carry() {
    let old = "a: 1\nserver:\n  host: h\n  port: 1\nz: 9\n";
    let new = "top: 0\na: 1\nserver:\n  host: h\n  port: 1\nz: 9\n";
    let block = TextRange::new(TextPosition::new(2, 1), TextPosition::new(4, 10));
    assert_eq!(
        carried(old, new, &[block])[0].slice(new),
        "server:\n  host: h\n  port: 1"
    );
    let empty = TextRange::new(TextPosition::new(3, 3), TextPosition::new(3, 3));
    let placed = carried(old, new, &[empty])[0];
    assert_eq!(
        placed,
        TextRange::new(TextPosition::new(4, 3), TextPosition::new(4, 3))
    );
    // An empty text: the lock on its only line.
    let placed = carried(old, "", &[block])[0];
    assert_eq!(placed.start, TextPosition::new(1, 1));
}

/// Diffing by character costs at most the run sizes, capped: resets at
/// two sizes.
#[test]
fn diffing_by_character_does_bounded_work() {
    for size in [2_000, 20_000] {
        let old: String = (0..size).map(|at| format!("key{at}: {at}\n")).collect();
        // Every other line changed: many small runs.
        let new: String = (0..size)
            .map(|at| {
                if at % 2 == 0 {
                    format!("key{at}: {at}x\n")
                } else {
                    format!("key{at}: {at}\n")
                }
            })
            .collect();
        let ranges: Vec<TextRange> = (1..=size)
            .step_by(4)
            .map(|line| TextRange::new(TextPosition::new(line, 1), TextPosition::new(line, 4)))
            .collect();
        let before = DIFFED.with(std::cell::Cell::get);
        let placed = carried(&old, &new, &ranges);
        let work = DIFFED.with(std::cell::Cell::get) - before;
        assert_eq!(placed.len(), ranges.len());
        // Each run twice: from its start and from its end.
        assert!(
            work <= 2 * (old.len() + new.len()),
            "{size} lines: {work} characters diffed"
        );
        // Every line changed: one run past the cap, not diffed by character.
        let new: String = (0..size).map(|at| format!("other{at}: x\n")).collect();
        let before = DIFFED.with(std::cell::Cell::get);
        carried(&old, &new, &ranges[..10]);
        assert_eq!(DIFFED.with(std::cell::Cell::get), before, "{size} lines");
    }
}

/// A lock on part of a line (the line break before it deleted) lands on
/// its own text exactly: in a reset's text and in an echo's document.
#[test]
fn a_lock_sharing_its_line_lands_on_its_text() {
    let joined = "name: a\nport: 8secret: keep";
    let lock = TextRange::new(TextPosition::new(2, 8), TextPosition::new(2, 20));
    assert_eq!(lock.slice(joined), "secret: keep");
    for to in [
        "name: a\nport: 80\nsecret: keep",
        "x: 1\nname: a\nport: 80\nsecret: keep\n",
        "name: a\nport: 8Xsecret: keep",
        "nae: a\nport: 8secret: keep",
    ] {
        let placed = carried(joined, to, &[lock])[0];
        assert_eq!(placed.slice(to), "secret: keep", "{to:?}");
        let (moved, _) = moved_between(joined, to, Some(vec![lock]), None);
        assert_eq!(moved.expect("moved")[0].slice(to), "secret: keep", "{to:?}");
    }
}

/// Two equal values on one line, the second locked, a value added before
/// them: the lock stays on its own copy (the nearest copy of its text was
/// the first one, and the locked value was editable).
#[test]
fn a_lock_on_one_of_two_equal_values_stays_on_its_own() {
    // (old, new, line, locked columns in old, the same value's in new)
    let cases = [
        ("x = [0, 1]\n", "x = [1, 0, 1]\n", 1, (9, 10), (12, 13)),
        (
            "a: 1\nb: [0, true]\n",
            "a: 1\nb: [true, 0, true]\n",
            2,
            (9, 13),
            (15, 19),
        ),
    ];
    for (old, new, line, (from, to), (want_from, want_to)) in cases {
        let lock = TextRange::new(TextPosition::new(line, from), TextPosition::new(line, to));
        let want = TextRange::new(
            TextPosition::new(line, want_from),
            TextPosition::new(line, want_to),
        );
        assert_eq!(
            carried(old, new, &[lock])[0],
            want,
            "reset {old:?} -> {new:?}"
        );
        let (moved, _) = moved_between(old, new, Some(vec![lock]), None);
        assert_eq!(moved.expect("moved")[0], want, "echo {old:?} -> {new:?}");
    }
}

/// A range carried exactly covers its old text; one the diff cannot follow
/// is over-locked, never placed on a guess, and never dropped.
#[test]
fn a_range_covers_its_old_text_or_is_over_locked() {
    let old = "a: 1\nb: 2\nc: 3\n";
    let lock = TextRange::new(TextPosition::new(2, 1), TextPosition::new(2, 5));
    for new in [
        "a: 1\nb: 2\nc: 3\n",
        "z\na: 1\nb: 2\nc: 3\n",
        "a: 1\nq: 9\nc: 3\n",
        "c: 3\na: 1\n",
        "",
    ] {
        let (placed, how) = Exact::new(old, new, false).carry(lock);
        let placed = placed[0];
        match how {
            Carried::Exact => assert_eq!(placed.slice(new), "b: 2", "{new:?}"),
            Carried::Grown | Carried::Lines => {
                assert!(!placed.slice(new).is_empty() || new.is_empty(), "{new:?}")
            }
        }
    }
}
