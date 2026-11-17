//! P1 through every line command of a live editor: a host range over a
//! line break keeps its text, whichever line the command runs on, alone or
//! with the line after it selected. Every edit passes the one gate, and the
//! test builds check after each that the spans it judged keep their text
//! where the followers put them (`kept`), so a command the gate judged
//! otherwise than the range then moves fails here.

use leptos::prelude::*;

use super::super::Language;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run_units as run};
use super::super::paths::{TextPosition, TextRange};
use super::super::test_state::TestEditor;

const TEXT: &str = "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n";

/// The range: from `l` of `Tunnel` to the end of `listen:`.
const LOCKED: &str = "l server configuration\nlisten:";

const COMMANDS: [Command; 7] = [
    Command::Lines(LineCommand::Move { down: false }),
    Command::Lines(LineCommand::Move { down: true }),
    Command::Lines(LineCommand::Copy { down: false }),
    Command::Lines(LineCommand::Copy { down: true }),
    Command::Lines(LineCommand::Delete),
    Command::Indent { outdent: false },
    Command::Indent { outdent: true },
];

#[test]
fn every_line_command_keeps_a_range_over_a_line_break() {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(TEXT.match_indices('\n').map(|(at, _)| at + 1))
        .filter(|at| *at < TEXT.len())
        .collect();
    let mut failures = Vec::new();
    for command in COMMANDS {
        for (index, start) in starts.iter().enumerate() {
            let selections = [
                (*start, *start),
                (
                    *start,
                    starts.get(index + 1).map_or(TEXT.len(), |next| next + 1),
                ),
            ];
            for selection in selections {
                let mut editor = TestEditor::new(Language::YAML, TEXT);
                editor.read_only_ranges = vec![TextRange::new(
                    TextPosition::new(1, 8),
                    TextPosition::new(2, 8),
                )];
                let (_owner, state) = editor.mount();
                let selection = (selection.0, selection.1.min(TEXT.len()));
                run(state, command, selection, false);
                let text = state.doc().text.get_untracked();
                let ranges = state.protect().ranges.get_untracked();
                let kept = ranges
                    .iter()
                    .any(|range| text.get(std::ops::Range::<usize>::from(*range)) == Some(LOCKED));
                if !kept {
                    failures.push(format!(
                        "{command:?} on {selection:?}: {ranges:?} in {text:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
