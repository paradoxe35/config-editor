//! Moving ranges between two texts.
use config_editor_leptos::{TextPosition, TextRange};

use super::{Remap, moved_between, reads_as};
use crate::text_lines::TextLines;

/// Ranges given for the text on screen of a BOM or CRLF document name
/// the places they name there: nothing moves (hard rounds 1–2's
/// `D1.host-1`, one character right after a BOM, and `D24.host-1`,
/// every line after the first locked in a CRLF file).
#[test]
fn ranges_for_the_shown_text_of_a_bom_or_crlf_document_stay() {
    let range = TextRange::new(TextPosition::new(3, 3), TextPosition::new(3, 7));
    let first = TextRange::new(TextPosition::new(1, 1), TextPosition::new(1, 2));
    let shown = "server:\n  port: 8080\n  host: localhost\n";
    for stored in [
        "server:\r\n  port: 8080\r\n  host: localhost\r\n",
        "\u{feff}server:\n  port: 8080\n  host: localhost\n",
        "\u{feff}server:\r\n  port: 8080\r\n  host: localhost\r\n",
        "server:\r  port: 8080\r  host: localhost\r",
    ] {
        for lock in [range, first] {
            let (moved, _) = moved_between(shown, stored, Some(vec![lock]), None);
            assert_eq!(moved, Some(vec![lock]), "{stored:?}");
            let (back, _) = moved_between(stored, shown, Some(vec![lock]), None);
            assert_eq!(back, Some(vec![lock]), "{stored:?} back");
        }
    }
    // A text typed past still moves them.
    let (moved, _) = moved_between(
        shown,
        "\u{feff}# new\r\nserver:\r\n  port: 8080\r\n  host: localhost\r\n",
        Some(vec![range]),
        None,
    );
    assert_eq!(
        moved,
        Some(vec![TextRange::new(
            TextPosition::new(4, 3),
            TextPosition::new(4, 7)
        )])
    );
}

fn rng(mut seed: u64) -> impl FnMut(usize) -> usize {
    move |bound: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % bound.max(1) as u64) as usize
    }
}

/// Typing in one place (a key, a line, a paste, a deletion): a range
/// away from it covers the same text, wherever the lines moved.
#[test]
fn a_range_away_from_the_edit_covers_the_same_text() {
    let from = "name: web\r\nid: srv-42\nnote: é😀 x\n";
    let lock = TextRange::new(TextPosition::new(2, 1), TextPosition::new(2, 11));
    for to in [
        "#abcdefgh\nname: web\r\nid: srv-42\nnote: é😀 x\n",
        "name: web\r\nid: srv-42\nnote: é😀 xyz\n",
        "na\r\nid: srv-42\nnote: é😀 x\n",
        "name: web\r\nid: srv-42\nnote: x\n",
    ] {
        let mapped = Remap::new(from, to).range(lock);
        assert_eq!(mapped.slice(to), "id: srv-42", "{to:?}");
    }
}

/// Text typed at a range's edge is not taken in; an empty range there
/// stays empty.
#[test]
fn text_typed_at_a_ranges_edge_stays_out_of_it() {
    let range = TextRange::new(TextPosition::new(1, 3), TextPosition::new(1, 5));
    let empty = TextRange::new(TextPosition::new(1, 3), TextPosition::new(1, 3));
    assert_eq!(
        Remap::new("abcdef", "abXcdef")
            .range(range)
            .slice("abXcdef"),
        "cd"
    );
    assert_eq!(
        Remap::new("abcdef", "abcdXef")
            .range(range)
            .slice("abcdXef"),
        "cd"
    );
    assert_eq!(
        Remap::new("abcdef", "abXcdef")
            .range(empty)
            .slice("abXcdef"),
        ""
    );
    // Typed inside: the range grows over it.
    assert_eq!(
        Remap::new("abcdef", "abcXdef")
            .range(range)
            .slice("abcXdef"),
        "cXd"
    );
}

/// A text of unique characters (but its line breaks) and where each of
/// them went after random edits: `None` for what an edit inserted.
struct Edited {
    from: Vec<char>,
    to: Vec<(char, Option<usize>)>,
}

/// The lines of `chars` (each with its break) as the ids they hold.
fn lines_of<T: Copy>(chars: &[(char, T)]) -> Vec<Vec<T>> {
    let mut lines = vec![Vec::new()];
    for (char, id) in chars {
        lines.last_mut().expect("a line").push(*id);
        if *char == '\n' {
            lines.push(Vec::new());
        }
    }
    lines
}

/// One to five random edits of a random text: deletions, and insertions of
/// characters the text does not have (or line breaks).
fn edited(next: &mut impl FnMut(usize) -> usize) -> Edited {
    let mut pool = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain("éßñü中文😀🎉ΩЖ".chars());
    let mut from = Vec::new();
    for _ in 0..1 + next(30) {
        from.push(pool.next().expect("unique characters"));
        // No empty line: each holds a character no other line has.
        if next(4) == 0 {
            from.push('\n');
        }
    }
    let mut to: Vec<(char, Option<usize>)> = from
        .iter()
        .enumerate()
        .map(|(index, c)| (*c, Some(index)))
        .collect();
    for _ in 0..1 + next(5) {
        let at = next(to.len() + 1);
        if next(2) == 0 {
            let removed = (1 + next(4)).min(to.len() - at);
            to.drain(at..at + removed);
        } else {
            let inserted: Vec<(char, Option<usize>)> = (0..1 + next(3))
                .map(|_| match next(4) {
                    0 => ('\n', None),
                    _ => (pool.next().expect("unique characters"), None),
                })
                .collect();
            to.splice(at..at, inserted);
        }
    }
    Edited { from, to }
}

/// `text` as a host may store it: LF or CRLF, with or without a byte
/// order mark.
fn stored(text: &str, form: usize) -> String {
    let text = if form % 2 == 1 {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    if form >= 2 {
        format!("\u{feff}{text}")
    } else {
        text
    }
}

/// Random texts changed by one to five edits (each side LF or CRLF, with or
/// without a byte order mark), and random ranges of them: every character
/// of a range that the edits kept is inside the range mapped (a lock never
/// shrinks), and a range on lines no edit touched covers the same text,
/// edits before and after it included (`H30-2`: two separate edits locked
/// everything between them).
#[test]
fn a_range_between_any_number_of_edits_maps_exactly_and_never_shrinks() {
    let mut next = rng(0x2545_f491_4f6c_dd1d);
    let (mut exact, mut between) = (0, 0);
    for round in 0..20_000 {
        let Edited { from, to } = edited(&mut next);
        let to_chars: Vec<char> = to.iter().map(|(c, _)| *c).collect();
        let text_from: String = from.iter().collect();
        let text_to: String = to_chars.iter().collect();
        // Empty lines are alike: which one the edits kept is not in the text.
        if text_to.starts_with('\n') || text_to.contains("\n\n") {
            continue;
        }
        let units = |text: &[char], chars: usize| -> usize {
            text[..chars].iter().copied().map(char::len_utf16).sum()
        };
        let (a, b) = (next(from.len() + 1), next(from.len() + 1));
        let (start, end) = (a.min(b), a.max(b));
        let lines = TextLines::new(&text_from);
        let range = TextRange::new(
            lines.position(units(&from, start)),
            lines.position(units(&from, end)),
        );
        let (given, document) = (stored(&text_from, next(4)), stored(&text_to, next(4)));
        let (mapped, _) = moved_between(&given, &document, Some(vec![range]), None);
        let mapped = mapped.expect("ranges")[0];
        let to_lines = TextLines::new(&text_to);
        let covered = to_lines.offset(mapped.start)..to_lines.offset(mapped.end);
        let context =
            || format!("round {round}: {text_from:?} -> {text_to:?}, {start}..{end} -> {mapped:?}");
        for index in (start..end).filter(|index| from[*index] != '\n') {
            if let Some(place) = to.iter().position(|(_, id)| *id == Some(index)) {
                assert!(covered.contains(&units(&to_chars, place)), "{}", context());
            }
        }
        // Whether each line of `from` is in `to` as it was.
        let to_lines_ids = lines_of(&to);
        let from_ids: Vec<(char, usize)> = from.iter().copied().zip(0..).collect();
        let from_lines: Vec<Vec<usize>> = lines_of(&from_ids)
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect();
        // Where each line of `from` is in `to`, unchanged.
        let found: Vec<Option<usize>> = from_lines
            .iter()
            .map(|line| {
                to_lines_ids.iter().position(|kept| {
                    kept.len() == line.len() && kept.iter().zip(line).all(|(k, l)| *k == Some(*l))
                })
            })
            .collect();
        let line_of = |index: usize| {
            from_lines
                .iter()
                .position(|line| line.contains(&index))
                .unwrap_or(from_lines.len() - 1)
        };
        let (first, last) = (line_of(start), line_of(end.max(start + 1) - 1));
        // The range's lines unchanged and still together.
        let together = found[first]
            .is_some_and(|at| (first..=last).all(|line| found[line] == Some(at + line - first)));
        if together {
            exact += 1;
            assert_eq!(
                mapped.slice(&text_to),
                range.slice(&text_from),
                "{}",
                context()
            );
            if found[..first].contains(&None) && found[last + 1..].contains(&None) {
                between += 1;
            }
        }
    }
    assert!(
        exact > 2_000 && between > 300,
        "{exact} exact, {between} between edits"
    );
}

/// A store's copy of the document (LF for CRLF or a lone CR, no byte order
/// mark) reads as the document; a text that differs in anything else does
/// not (QA 33, HO33-3: ranges given again with it stay where they are).
#[test]
fn a_text_written_another_way_reads_as_the_document() {
    let document = "\u{feff}a: 1\r\nb: 2\r\n";
    for copy in [
        document,
        "a: 1\nb: 2\n",
        "\u{feff}a: 1\nb: 2\n",
        "a: 1\rb: 2\r",
    ] {
        assert!(reads_as(copy, document), "{copy:?}");
        assert!(reads_as(document, copy), "{copy:?}");
    }
    for other in ["a: 1\nb: 3\n", "a: 1\n\nb: 2\n", "a: 1\nb: 2", ""] {
        assert!(!reads_as(other, document), "{other:?}");
    }
}
