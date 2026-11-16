//! The edits a random session makes, each as the editor's own key,
//! command or pause makes it, on whatever text is on screen. Places are
//! relative (a line number taken modulo the lines there are), so a
//! session still runs once the shrinker drops some of its edits.

use leptos::prelude::*;

use crate::editor::edit::Edit;
use crate::editor::keys::enter::{Pressed, enter_units};
use crate::editor::keys::line_edit::LineCommand;
use crate::editor::keys::lines::{Command, run_units as run};
use crate::editor::languages::Language;
use crate::editor::save_properties::Random;
use crate::editor::state::EditorState;
use crate::editor::sync::{convert_now, switch_to};
use crate::editor::text::text_size;

/// A column at or past this is counted back from the line's end (`END`
/// is the end itself, `END + 1` one character before it).
pub(super) const END: usize = 1000;

/// How lines are taken for the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Take {
    /// The line and the lines indented deeper below it (an entry).
    Entry,
    /// The line alone, its line break included.
    Line,
    /// The line's text alone (End, Shift+Home): its indent and line break
    /// stay.
    Text,
}

/// How the clipboard is pasted on a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Put {
    /// Before the line.
    Before,
    /// Over a new blank line typed before it, selected whole.
    OverBlank,
    /// Over the line, selected whole (its line break too).
    OverLine,
}

/// One step of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Op {
    /// `text` typed a character at a time at `line`, `column`.
    Type {
        line: usize,
        column: usize,
        text: &'static str,
    },
    /// Backspace pressed `count` times at `line`, `column`.
    Backspace {
        line: usize,
        column: usize,
        count: usize,
    },
    /// Enter at `line`, `column`.
    Enter {
        line: usize,
        column: usize,
    },
    /// Ctrl+Shift+K on `line`.
    DeleteLine {
        line: usize,
    },
    /// Alt+Up / Alt+Down on `line`.
    MoveLine {
        line: usize,
        down: bool,
    },
    /// Shift+Alt+Up / Shift+Alt+Down on `line`.
    CopyLine {
        line: usize,
        down: bool,
    },
    /// The lines taken as `how` says, cut.
    Cut {
        line: usize,
        how: Take,
    },
    /// The lines taken as `how` says, copied.
    Copy {
        line: usize,
        how: Take,
    },
    /// The clipboard pasted on `line` (nothing when it is empty).
    Paste {
        line: usize,
        how: Put,
    },
    /// Replace all of the text's `pick`th word with the word and `suffix`.
    ReplaceAll {
        pick: usize,
        suffix: &'static str,
    },
    /// Select all, then the text pasted back with `line` changed a little.
    PasteWhole {
        line: usize,
    },
    Undo,
    Redo,
    /// A typing pause: the view saved.
    Pause,
    /// A switch to `Language::ALL[to % 5]`.
    Switch {
        to: usize,
    },
}

impl Op {
    /// Whether the editor saves at this step (a pause or a switch).
    pub(super) fn saves(&self) -> bool {
        matches!(self, Op::Pause | Op::Switch { .. })
    }
}

/// What is typed: mostly a value's letters and digits, now and then a
/// character of the syntax.
const TYPED: &[&str] = &[
    "x", "1", "0", "a", "y2", "7", "b", "x", "1", " ", "_", "-", ".", "\"", ":", "=", "#",
];
const SUFFIXES: &[&str] = &["2", "_x", "Y"];

/// A random session of `length` steps, a pause after some.
pub(super) fn session(random: &mut Random, length: usize) -> Vec<Op> {
    let mut ops = Vec::new();
    let line = |random: &mut Random| random.below(64);
    // Mostly at a line's end (a value's), or just before its last
    // character (inside a closing quote).
    let column = |random: &mut Random| match random.below(8) {
        0..=3 => END,
        4 | 5 => END + 1,
        6 => random.below(12),
        _ => random.below(40),
    };
    for _ in 0..length {
        let op = match random.below(100) {
            0..=19 => Op::Type {
                line: line(random),
                column: column(random),
                text: TYPED[random.below(TYPED.len())],
            },
            20..=29 => Op::Backspace {
                line: line(random),
                column: column(random),
                count: 1 + random.below(3),
            },
            30..=35 => Op::Enter {
                line: line(random),
                column: column(random),
            },
            36..=40 => Op::DeleteLine { line: line(random) },
            41..=45 => Op::MoveLine {
                line: line(random),
                down: random.below(2) == 0,
            },
            46..=49 => Op::CopyLine {
                line: line(random),
                down: random.below(2) == 0,
            },
            50..=56 => Op::Cut {
                line: line(random),
                how: [Take::Entry, Take::Line, Take::Text][random.below(3)],
            },
            57..=60 => Op::Copy {
                line: line(random),
                how: [Take::Entry, Take::Line, Take::Text][random.below(3)],
            },
            61..=72 => Op::Paste {
                line: line(random),
                how: [Put::Before, Put::OverBlank, Put::OverLine][random.below(3)],
            },
            73..=76 => Op::ReplaceAll {
                pick: random.below(64),
                suffix: SUFFIXES[random.below(SUFFIXES.len())],
            },
            77..=80 => Op::PasteWhole { line: line(random) },
            81..=86 => Op::Undo,
            87..=90 => Op::Redo,
            _ => Op::Switch {
                to: random.below(5),
            },
        };
        let saves = op.saves();
        ops.push(op);
        if !saves && random.below(5) < 2 {
            ops.push(Op::Pause);
        }
    }
    ops
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The lines of `text` with their breaks.
fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// `line` taken modulo the lines of `text` (an empty text has one).
fn line_of(text: &str, line: usize) -> usize {
    line % lines(text).len().max(1)
}

/// The byte offset of the start of line `line` (from 0).
fn line_start(text: &str, line: usize) -> usize {
    lines(text).iter().take(line).map(|line| line.len()).sum()
}

/// The byte offset of `line`, `column` (see [`END`]).
fn offset(text: &str, line: usize, column: usize) -> usize {
    let line = line_of(text, line);
    let start = line_start(text, line);
    let written = lines(text)
        .get(line)
        .map_or("", |line| line.trim_end_matches(['\n', '\r']));
    let characters: Vec<(usize, char)> = written.char_indices().collect();
    let at = if column >= END {
        characters.len().saturating_sub(column - END)
    } else {
        column.min(characters.len())
    };
    start + characters.get(at).map_or(written.len(), |(byte, _)| *byte)
}

/// The leading spaces of the line holding byte `at`.
fn indent_at(text: &str, at: usize) -> String {
    let start = text[..at].rfind('\n').map_or(0, |break_at| break_at + 1);
    text[start..]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

/// The edit replacing the bytes `from..to` with `typed`, as a command.
fn replace(state: EditorState, (from, to): (usize, usize), typed: &str) {
    let before = state.doc().text.get_untracked();
    let text = format!("{}{typed}{}", &before[..from], &before[to..]);
    let start = units(&before[..from]);
    state.commit(Edit::new_units(
        before.clone(),
        (start, units(&before[..to])),
        text,
        start + units(typed),
    ));
}

/// The edit replacing the bytes `from..to` with `typed`, as input of
/// `kind` the browser applied at `time`.
fn input(state: EditorState, (from, to): (usize, usize), typed: &str, kind: &str, time: f64) {
    let before = state.doc().text.get_untracked();
    let text = format!("{}{typed}{}", &before[..from], &before[to..]);
    let start = units(&before[..from]);
    let caret = start + units(typed);
    state.commit(Edit::input_units(
        before.clone(),
        (start, units(&before[..to])),
        text,
        (caret, caret),
        kind.into(),
        time,
    ));
}

/// The typing pause (`sync::debounce_validation`, then
/// `sync::convert_edits`).
pub(super) fn pause(state: EditorState) {
    state.recheck_tokens();
    let text = state.doc().text.get_untracked();
    state.problems().diagnosed_text.set(text.clone());
    let _ = state.problems().diagnostics.get_untracked();
    convert_now(state, text, state.sync().edit_language.get_untracked());
}

/// The byte range of the lines `how` takes at `line`.
fn taken(text: &str, line: usize, how: Take) -> (usize, usize) {
    let all = lines(text);
    let first = line_of(text, line);
    let start = line_start(text, first);
    let Some(own) = all.get(first) else {
        return (0, 0);
    };
    match how {
        Take::Line => (start, start + own.len()),
        Take::Text => {
            let indent = own.len() - own.trim_start().len();
            (start + indent, start + own.trim_end().len().max(indent))
        }
        Take::Entry => {
            let depth = |line: &str| line.len() - line.trim_start().len();
            let mut last = first + 1;
            while last < all.len() && !all[last].trim().is_empty() && depth(all[last]) > depth(own)
            {
                last += 1;
            }
            // The line closing it at its indent.
            if last > first + 1
                && last < all.len()
                && depth(all[last]) == depth(own)
                && all[last].trim_start().starts_with(['}', ']', '<'])
                && all[last].trim_start().starts_with("</") | !all[last].contains('<')
            {
                last += 1;
            }
            (start, line_start(text, last))
        }
    }
}

/// Makes `op` in the session, `clipboard` holding what was cut or copied;
/// `time` is the input's clock, moved on by each keystroke.
pub(super) fn apply(
    state: EditorState,
    host: Language,
    op: &Op,
    clipboard: &mut Option<String>,
    time: &mut f64,
) {
    let text = state.doc().text.get_untracked();
    *time += 50.0;
    match *op {
        Op::Type {
            line,
            column,
            text: typed,
        } => {
            let mut at = offset(&text, line, column);
            for character in typed.chars() {
                let typed = character.to_string();
                input(state, (at, at), &typed, "insertText", *time);
                at += typed.len();
                *time += 50.0;
            }
        }
        Op::Backspace {
            line,
            column,
            count,
        } => {
            for _ in 0..count {
                let text = state.doc().text.get_untracked();
                let at = offset(&text, line, column);
                let Some(previous) = text[..at].chars().next_back() else {
                    break;
                };
                input(
                    state,
                    (at - previous.len_utf8(), at),
                    "",
                    "deleteContentBackward",
                    *time,
                );
                *time += 50.0;
            }
        }
        Op::Enter { line, column } => {
            let at = units(&text[..offset(&text, line, column)]);
            enter_units(state, (at, at), Pressed::Enter, Some(*time));
        }
        Op::DeleteLine { line } | Op::MoveLine { line, .. } | Op::CopyLine { line, .. } => {
            let at = units(&text[..line_start(&text, line_of(&text, line))]);
            let command = match *op {
                Op::DeleteLine { .. } => LineCommand::Delete,
                Op::MoveLine { down, .. } => LineCommand::Move { down },
                Op::CopyLine { down, .. } => LineCommand::Copy { down },
                _ => unreachable!(),
            };
            run(state, Command::Lines(command), (at, at), false);
        }
        Op::Cut { line, how } | Op::Copy { line, how } => {
            let (from, to) = taken(&text, line, how);
            if from == to {
                return;
            }
            *clipboard = Some(text[from..to].to_string());
            // As Ctrl+C and Ctrl+X tell the editor what they copy.
            state.note_copied(&text[from..to], (text_size(from), text_size(to)));
            if matches!(op, Op::Cut { .. }) {
                input(state, (from, to), "", "deleteByCut", *time);
                // A cut line's text leaves its line blank: deleted too.
                if how == Take::Text {
                    let text = state.doc().text.get_untracked();
                    let start = text[..from].rfind('\n').map_or(0, |at| at + 1);
                    let end = text[from..]
                        .find('\n')
                        .map_or(text.len(), |at| from + at + 1);
                    if text[start..end].trim().is_empty() && end > start {
                        run(
                            state,
                            Command::Lines(LineCommand::Delete),
                            (units(&text[..start]), units(&text[..start])),
                            false,
                        );
                    }
                }
            }
        }
        Op::Paste { line, how } => {
            let Some(clip) = clipboard.clone() else {
                return;
            };
            let at = line_start(&text, line_of(&text, line));
            let indent = indent_at(&text, at.min(text.len()));
            // A line's text is pasted at the line's indent, as a line.
            let pasted = if clip.ends_with('\n') {
                clip.clone()
            } else {
                format!("{indent}{clip}\n")
            };
            match how {
                Put::Before => input(state, (at, at), &pasted, "insertFromPaste", *time),
                Put::OverBlank => {
                    replace(state, (at, at), &format!("{indent}\n"));
                    let end = at + indent.len() + 1;
                    input(state, (at, end), &pasted, "insertFromPaste", *time);
                }
                Put::OverLine => {
                    let end = text[at..]
                        .find('\n')
                        .map_or(text.len(), |break_at| at + break_at + 1);
                    input(state, (at, end), &pasted, "insertFromPaste", *time);
                }
            }
        }
        Op::ReplaceAll { pick, suffix } => {
            let words = words(&text);
            if words.is_empty() {
                return;
            }
            let word = &words[pick % words.len()];
            let (replaced, pieces) = replace_words(&text, word, &format!("{word}{suffix}"));
            let caret = units(&replaced);
            // As the Replace all command says what it replaced.
            let splices = Some(crate::editor::bridge::links::Splice::each(pieces));
            state.commit(Edit::new_units(text, (0, 0), replaced, caret).splicing(splices));
        }
        Op::PasteWhole { line } => {
            let changed = changed_a_little(&text, line_of(&text, line));
            input(state, (0, text.len()), &changed, "insertFromPaste", *time);
        }
        Op::Undo | Op::Redo => {
            if let Some(step) = state.history_edit(matches!(op, Op::Redo)) {
                state.commit(step);
            }
        }
        Op::Pause => pause(state),
        Op::Switch { to } => {
            let all = [
                Language::YAML,
                Language::JSON,
                Language::TOML,
                Language::HCL,
                Language::XML,
            ];
            let _ = host;
            // The language on screen is not offered: the user waits
            // instead.
            let next = all[to % all.len()];
            if next == state.sync().edit_language.get_untracked() {
                pause(state);
            } else {
                switch_to(state, next);
            }
        }
    }
}

/// The words of `text` (letters, digits, `_`; two characters at least),
/// each once, sorted.
pub(super) fn words(text: &str) -> Vec<String> {
    let mut words: Vec<String> = text
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|word| word.chars().count() >= 2)
        .map(str::to_string)
        .collect();
    words.sort();
    words.dedup();
    words
}

/// `text` with every whole word `word` replaced by `with`.
fn replace_words(
    text: &str,
    word: &str,
    with: &str,
) -> (String, Vec<(std::ops::Range<usize>, String)>) {
    let mut out = String::new();
    let mut pieces = Vec::new();
    let mut rest = text;
    let part = |character: char| character.is_alphanumeric() || character == '_';
    while let Some(at) = rest.find(word) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + word.len()..].chars().next();
        out.push_str(&rest[..at]);
        if before.is_some_and(part) || after.is_some_and(part) {
            out.push_str(word);
        } else {
            let start = text.len() - rest.len() + at;
            pieces.push((start..start + word.len(), with.to_string()));
            out.push_str(with);
        }
        rest = &rest[at + word.len()..];
    }
    out.push_str(rest);
    (out, pieces)
}

/// `text` with line `line` changed a little: its last digit made a 7, or
/// else an `x` added to its last word.
fn changed_a_little(text: &str, line: usize) -> String {
    let start = line_start(text, line);
    let end = text[start..].find('\n').map_or(text.len(), |at| start + at);
    let written = &text[start..end];
    let changed = if let Some(digit) = written.rfind(|character: char| character.is_ascii_digit()) {
        format!("{}7{}", &written[..digit], &written[digit + 1..])
    } else if let Some(letter) = written.rfind(|character: char| character.is_alphanumeric()) {
        let next = letter + written[letter..].chars().next().map_or(1, char::len_utf8);
        format!("{}x{}", &written[..next], &written[next..])
    } else {
        written.to_string()
    };
    format!("{}{changed}{}", &text[..start], &text[end..])
}
