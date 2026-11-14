//! The walk up by parents (a batch) finds the line the walk over every
//! line above finds, and the paths of many places asked at once are the
//! ones each asked alone, explicit keys and block scalar keys included.

use super::indent_above::{Counting, above_less};
use crate::editor::Language;
use crate::editor::paths::{path_at, paths_at, with_spans};

/// Lines of every kind a walk up passes: entries at several depths,
/// items, comments, blank lines, explicit keys and their values, a block
/// scalar key's text.
const PIECES: [&str; 16] = [
    "a:",
    "  b: 1",
    "    c: 2",
    "  - d: 3",
    "  - - e",
    "  # note",
    "",
    "   ",
    "? k",
    ": v",
    "  : w",
    "  ? |",
    "    text",
    "      more",
    "\tf: 4",
    "- x",
];

/// Texts of `PIECES` picked in a fixed pseudo-random order.
fn texts() -> Vec<String> {
    let mut seed: u64 = 7;
    (0..60)
        .map(|_| {
            (0..24)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    PIECES[(seed >> 33) as usize % PIECES.len()]
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

#[test]
fn the_walk_by_parents_finds_the_line_the_walk_over_every_line_finds() {
    let behavior = Language::YAML.behavior();
    for text in texts() {
        let lines: Vec<&str> = text.split('\n').collect();
        let line = |at: usize| lines[at];
        for counting in [Counting::Entries, Counting::NonBlank] {
            for index in 0..lines.len() {
                for column in 0..8 {
                    let alone = above_less(&text, &line, lines.len(), counting, index, column);
                    let batched = with_spans(behavior, &text, &[], || {
                        above_less(&text, &line, lines.len(), counting, index, column)
                    });
                    assert_eq!(batched, alone, "line {index}, column {column} of:\n{text}");
                }
            }
        }
    }
}

#[test]
fn places_asked_at_once_have_the_paths_each_has_alone() {
    let behavior = Language::YAML.behavior();
    for text in texts() {
        let cursors: Vec<usize> = (0..=text.len()).filter(|at| text.is_char_boundary(*at)).collect();
        let alone: Vec<_> = cursors.iter().map(|cursor| path_at(behavior, &text, *cursor)).collect();
        assert_eq!(paths_at(behavior, &text, &cursors), alone, "{text}");
    }
}
