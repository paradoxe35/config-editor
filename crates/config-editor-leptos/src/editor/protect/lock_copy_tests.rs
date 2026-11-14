//! A copy of locked text pasted where it locks nothing is accepted, in
//! every language: a host range's lines copied and pasted far from it, right
//! beside another lock (above it, where the pasted text starts as the
//! locked line does, and below it), a block of several locked lines, and
//! one closing their block, which leaves the text broken, with the locked
//! text written once or repeated in 12 blocks. The locks
//! stay where they were, on the same text; an edit into one is still
//! refused, and the copy takes typing (QA 33: such a paste was refused,
//! the edit read as starting inside the lock its text began like).

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::test_state::TestEditor;

/// `blocks` services, each with the same locked `target` and `port` lines,
/// then a spare block holding none of their keys, its `limit` locked.
fn document(language: Language, blocks: usize) -> String {
    let each = |index: usize| match language.label() {
        "YAML" => format!("svc{index}:\n  name: s{index}\n  target: 22\n  port: 80\n  tail: 0\n"),
        "JSON" => format!(
            "  \"svc{index}\": {{\n    \"name\": \"s{index}\",\n    \"target\": 22,\n    \"port\": 80,\n    \"tail\": 0\n  }},\n"
        ),
        "TOML" => {
            format!("[svc{index}]\nname = \"s{index}\"\ntarget = 22\nport = 80\ntail = 0\n\n")
        }
        "HCL" => format!(
            "svc{index} {{\n  name = \"s{index}\"\n  target = 22\n  port = 80\n  tail = 0\n}}\n"
        ),
        _ => format!(
            "  <svc{index}>\n    <name>s{index}</name>\n    <target>22</target>\n    <port>80</port>\n    <tail>0</tail>\n  </svc{index}>\n"
        ),
    };
    let body: String = (0..blocks).map(each).collect();
    match language.label() {
        "YAML" => format!("{body}spare:\n  mode: x\n  limit: 5\n"),
        "JSON" => {
            format!("{{\n{body}  \"spare\": {{\n    \"mode\": \"x\",\n    \"limit\": 5\n  }}\n}}\n")
        }
        "TOML" => format!("{body}[spare]\nmode = \"x\"\nlimit = 5\n"),
        "HCL" => format!("{body}spare {{\n  mode = \"x\"\n  limit = 5\n}}\n"),
        _ => format!(
            "<root>\n{body}  <spare>\n    <mode>x</mode>\n    <limit>5</limit>\n  </spare>\n</root>\n"
        ),
    }
}

/// The index of the first line of `lines` holding `part` from `from` on.
fn line_with(lines: &[&str], part: &str, from: usize) -> usize {
    from + lines[from..]
        .iter()
        .position(|line| line.contains(part))
        .unwrap_or_else(|| panic!("a line with {part:?}"))
}

/// The byte (and UTF-16: ASCII) offset where line `line` starts.
fn line_start(lines: &[&str], line: usize) -> usize {
    lines[..line].iter().map(|line| line.len() + 1).sum()
}

/// A host range over each whole line in `locked` (from its first column).
fn ranges(lines: &[&str], locked: &[usize]) -> Vec<TextRange> {
    locked
        .iter()
        .map(|line| {
            TextRange::new(
                TextPosition::new(line + 1, 1),
                TextPosition::new(line + 1, lines[*line].len() + 1),
            )
        })
        .collect()
}

/// Replaces `range` of the text on screen with `with`, as typed or pasted
/// (the caret ends the new text). Returns whether the edit was made.
fn replace(state: EditorState, range: std::ops::Range<usize>, with: &str, kind: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let caret = range.start + with.len();
    state.commit(Edit::input(
        before,
        (range.start, range.end),
        after,
        (caret, caret),
        kind.to_string(),
        0.0,
    ))
}

/// One paste: the lines `copied` (indices) pasted at the start of line
/// `at`.
struct Paste {
    what: &'static str,
    copied: std::ops::Range<usize>,
    at: usize,
}

#[test]
fn a_copy_of_locked_text_pasted_where_it_locks_nothing_is_accepted() {
    let mut failures = Vec::new();
    for language in Language::ALL.iter().copied() {
        for repeated in [1, 12] {
            let text = document(language, repeated.max(2));
            let lines: Vec<&str> = text.split('\n').collect();
            let target = line_with(&lines, "target", 0);
            let limit = line_with(&lines, "limit", 0);
            let mode = line_with(&lines, "mode", 0);
            let mut locked: Vec<usize> = Vec::new();
            let mut from = 0;
            for _ in 0..repeated {
                let at = line_with(&lines, "target", from);
                locked.extend([at, at + 1]);
                from = at + 2;
            }
            locked.push(limit);
            let pastes = [
                Paste {
                    what: "a locked line far from it",
                    copied: target..target + 1,
                    at: mode,
                },
                Paste {
                    what: "a locked line right above another lock",
                    copied: target..target + 1,
                    at: limit,
                },
                Paste {
                    what: "a locked line right below another lock",
                    copied: target..target + 1,
                    at: limit + 1,
                },
                Paste {
                    what: "a block of several locked lines",
                    copied: target..target + 2,
                    at: mode,
                },
                // Where blocks close: the text left broken, the paste
                // starting as the lock below it does.
                Paste {
                    what: "locked lines with their block's closer right above another lock",
                    copied: target..target + 4,
                    at: limit,
                },
            ];
            let closes = matches!(language.label(), "JSON" | "HCL" | "XML");
            for paste in pastes
                .into_iter()
                .filter(|paste| closes || paste.copied.len() < 4)
            {
                let case = format!(
                    "{language}, locked text written {repeated}×, {}",
                    paste.what
                );
                let mut editor = TestEditor::new(language, &text);
                editor.read_only_ranges = ranges(&lines, &locked);
                let (_owner, state) = editor.mount();
                let placed = state.protect().ranges.get_untracked();
                let copy: String = lines[paste.copied.clone()]
                    .iter()
                    .map(|line| format!("{line}\n"))
                    .collect();
                let at = line_start(&lines, paste.at);
                if !replace(state, at..at, &copy, "insertFromPaste") {
                    failures.push(format!("{case}: the paste was refused"));
                    continue;
                }
                let shown = state.doc().text.get_untracked();
                // The locks where they were, on the same text.
                let moved: Vec<std::ops::Range<usize>> = placed
                    .iter()
                    .map(|range| {
                        if range.start >= at {
                            range.start + copy.len()..range.end + copy.len()
                        } else {
                            range.clone()
                        }
                    })
                    .collect();
                let followed = state.protect().ranges.get_untracked();
                if followed != moved {
                    failures.push(format!(
                        "{case}: the locks moved to {followed:?}, expected {moved:?}"
                    ));
                    continue;
                }
                // An edit into the first lock is still refused.
                let lock = &moved[0];
                let value = lock.start + shown[lock.clone()].find("22").expect("its value");
                if replace(state, value..value + 2, "23", "insertText") {
                    failures.push(format!("{case}: an edit into the lock was taken"));
                    continue;
                }
                // The copy is not locked: it takes typing.
                let copied = at + copy.find("22").expect("the copy's value");
                if !replace(state, copied..copied + 2, "23", "insertText") {
                    failures.push(format!("{case}: the pasted copy refused typing"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
