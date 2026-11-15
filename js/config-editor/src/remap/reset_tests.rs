//! A host's ranges carried into a new text of its own (QA 38, H38-3: a
//! newer version of the file locked every changed line after the lock, and
//! a lock whose line moved further down was dropped).
use config_editor_leptos::{TextPosition, TextRange};

use super::super::carried;
use super::ALIGNED;

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
    (
        "the locked line indented",
        "name: a\nsub:\n  secret: keep\nlisten: 1\n",
        3,
    ),
    (
        "the locked line twice",
        "secret: keep\nname: a\nsecret: keep\nlisten: 1\n",
        3,
    ),
    ("unrelated text", "host: h\nport: 2\nmode: m\n", 2),
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
                assert!(
                    !slice.contains('\n') || form == "line and break",
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
        assert_eq!(carried[0].slice(new), want, "{old:?} -> {new:?}");
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

/// The work per range is bounded however large a changed run is: a reset
/// to a text with every line changed aligns each lock within a window.
#[test]
fn aligning_does_bounded_work_per_range() {
    for size in [2_000, 20_000] {
        let old: String = (0..size).map(|at| format!("key{at}: {at}\n")).collect();
        let new: String = (0..size).map(|at| format!("other{at}: x\n")).collect();
        let ranges: Vec<TextRange> = (1..=size)
            .step_by(4)
            .map(|line| TextRange::new(TextPosition::new(line, 1), TextPosition::new(line, 4)))
            .collect();
        let before = ALIGNED.with(std::cell::Cell::get);
        let carried = carried(&old, &new, &ranges);
        let work = ALIGNED.with(std::cell::Cell::get) - before;
        assert_eq!(carried.len(), ranges.len());
        assert!(
            work <= ranges.len() * (2 * super::ALIGN_WINDOW + 1),
            "{size} lines: {work} lines compared for {} ranges",
            ranges.len()
        );
    }
}
