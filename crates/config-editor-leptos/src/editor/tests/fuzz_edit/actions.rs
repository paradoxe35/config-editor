//! The edits a random sequence is made of, each made as the editor's own
//! key, command or widget makes it (natively: the parts that read a
//! keyboard event are given what the event would say). Positions are raw
//! numbers read against the text at the time (see `docs::offset`), so a
//! sequence shrunk by leaving edits out still runs.

use leptos::prelude::*;

use super::docs::{LANGUAGES, offset, selection, units};
use super::runner::Rng;
use crate::editor::commands::{indent_lines, tab_insert, toggled_selection};
use crate::editor::edit::Edit;
use crate::editor::keyboard::{paired_text, replace_selection, utf16_to_byte_index};
use crate::editor::keys::enter::{Pressed, enter_at};
use crate::editor::keys::line_edit::LineCommand;
use crate::editor::keys::lines::{Command, run};
use crate::editor::state::EditorState;
use crate::editor::sync::{convert_now, switch_to};

/// Text typed, as the browser's `insertText` sends it.
pub(super) const TYPED: [&str; 22] = [
    "a", "x", "1", " ", "key", ": ", " = ", "\"", "{", "}", "[", "]", "<", ">", "#", "é", "😀",
    "-", ",", "port", "\t", "/",
];

/// Find queries and replacements.
pub(super) const QUERIES: [&str; 12] = [
    "port", "a", "e", "1", " ", "\"", "web", "(\\w+)", "[0-9]+", "^ +", "o.", "x_1",
];
pub(super) const REPLACEMENTS: [&str; 9] = ["", "p", "port", "$1$1", "9", " ", "\"q\"", "$0-", "Z"];

/// Openers typed with auto-closing on.
pub(super) const OPENERS: [char; 5] = ['{', '[', '(', '"', '\''];

/// One edit (or read) of a sequence.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Action {
    Type {
        at: u32,
        text: &'static str,
    },
    Backspace {
        at: u32,
        count: u8,
    },
    Enter {
        at: u32,
    },
    LineBelow {
        at: u32,
    },
    Tab {
        at: u32,
        to: u32,
        shift: bool,
    },
    /// 0 delete, 1 move up, 2 move down, 3 copy up, 4 copy down.
    Line {
        at: u32,
        to: u32,
        command: u8,
    },
    Indent {
        at: u32,
        to: u32,
        outdent: bool,
    },
    Comment {
        at: u32,
        to: u32,
    },
    Cut {
        at: u32,
        to: u32,
    },
    Copy {
        at: u32,
        to: u32,
    },
    Paste {
        at: u32,
        to: u32,
    },
    Pair {
        at: u32,
        to: u32,
        opening: char,
    },
    /// Options: bit 0 match case, bit 1 whole word, bit 2 regex.
    ReplaceAll {
        query: &'static str,
        with: &'static str,
        options: u8,
    },
    ReplaceOne {
        query: &'static str,
        with: &'static str,
        options: u8,
        index: u8,
    },
    Complete {
        at: u32,
        pick: u8,
    },
    Fold {
        line: u32,
    },
    Unfold {
        line: u32,
    },
    FoldAll,
    UnfoldAll,
    /// To `LANGUAGES[to % 5]`.
    Switch {
        to: u8,
    },
    /// The typing pause: placeholders followed, the view converted.
    Pause,
    /// A second without input: the next typing starts a new undo step.
    Wait,
    Undo,
    Redo,
}

/// What a sequence carries from edit to edit.
#[derive(Default)]
pub(super) struct Context {
    pub(super) clipboard: String,
    /// Milliseconds, for the undo grouping of typing.
    pub(super) time: f64,
}

/// A random edit; `with_history` adds undo and redo, `with_views` the
/// Edit-as switches.
pub(super) fn random_action(rng: &mut Rng, with_history: bool, with_views: bool) -> Action {
    let at = rng.position();
    let to = if rng.chance(50) { at } else { rng.position() };
    loop {
        let action = match rng.below(26) {
            0..=4 => Action::Type {
                at,
                text: rng.pick(&TYPED),
            },
            5 => Action::Backspace {
                at,
                count: 1 + rng.below(4) as u8,
            },
            6 | 7 => Action::Enter { at },
            8 => Action::LineBelow { at },
            9 => Action::Tab {
                at,
                to,
                shift: rng.chance(40),
            },
            10 | 11 => Action::Line {
                at,
                to,
                command: rng.below(5) as u8,
            },
            12 => Action::Indent {
                at,
                to,
                outdent: rng.chance(50),
            },
            13 => Action::Comment { at, to },
            14 => Action::Cut { at, to },
            15 => Action::Copy { at, to },
            16 => Action::Paste { at, to },
            17 => Action::Pair {
                at,
                to,
                opening: *rng.pick(&OPENERS),
            },
            18 => Action::ReplaceAll {
                query: rng.pick(&QUERIES),
                with: rng.pick(&REPLACEMENTS),
                options: rng.below(8) as u8,
            },
            19 => Action::ReplaceOne {
                query: rng.pick(&QUERIES),
                with: rng.pick(&REPLACEMENTS),
                options: rng.below(8) as u8,
                index: rng.below(6) as u8,
            },
            20 => Action::Complete {
                at,
                pick: rng.below(8) as u8,
            },
            21 => match rng.below(4) {
                0 => Action::Fold { line: at },
                1 => Action::Unfold { line: at },
                2 => Action::FoldAll,
                _ => Action::UnfoldAll,
            },
            22 if with_views => Action::Switch {
                to: rng.below(5) as u8,
            },
            23 => {
                if rng.chance(50) {
                    Action::Pause
                } else {
                    Action::Wait
                }
            }
            24 | 25 if with_history => {
                if rng.chance(60) {
                    Action::Undo
                } else {
                    Action::Redo
                }
            }
            _ => continue,
        };
        return action;
    }
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// Puts the caret (and the textarea's selection, natively the caret) at
/// `at`.
fn place(state: EditorState, at: usize) {
    state.doc().caret.set(at);
}

fn typed(state: EditorState, (start, end): (usize, usize), text: &str, kind: &str, time: f64) {
    let before = shown(state);
    let (updated, caret) = replace_selection(&before, start, end, text);
    if updated == before {
        return;
    }
    state.commit(Edit::input(
        before,
        (start, end),
        updated,
        (caret, caret),
        kind.into(),
        time,
    ));
}

/// The text of UTF-16 `start..end`.
fn slice(text: &str, (start, end): (usize, usize)) -> String {
    let from = utf16_to_byte_index(text, start);
    let to = utf16_to_byte_index(text, end);
    text[from..to].to_string()
}

/// Tab and Shift+Tab as `keys::tab` makes them (no snippet slots open).
fn tab(state: EditorState, (start, end): (usize, usize), shift: bool) {
    let text = shown(state);
    let unit = state.doc().indent.get_value();
    let spans = start != end && slice(&text, (start, end)).contains('\n');
    if shift || spans {
        if let Some((updated, from, to)) = indent_lines(&text, start, end, shift, &unit)
            && updated != text
        {
            state.commit(Edit::new(text, (start, end), updated, to).selecting_from(from));
        }
        return;
    }
    let (updated, caret) = replace_selection(&text, start, end, &tab_insert(&text, start, &unit));
    state.commit(Edit::new(text, (start, end), updated, caret));
}

/// Ctrl+/ as `keys::typing::toggle_comments` makes it.
fn comment(state: EditorState, (start, end): (usize, usize)) {
    let before = shown(state);
    let Some((updated, from, to)) = state.behavior().toggle_comments(&before, start, end) else {
        return;
    };
    let (from, to) = toggled_selection(&before, (start, end), &updated, (from, to));
    if updated != before {
        state.commit(Edit::new(before, (start, end), updated, to).selecting_from(from));
    }
}

/// An opener typed with auto-closing on, over `selection`.
fn pair(state: EditorState, (start, end): (usize, usize), opening: char, time: f64) {
    let before = shown(state);
    match paired_text(&before, start, end, opening, state.behavior()) {
        Some((updated, from, to)) => {
            let edit = Edit::new(before, (start, end), updated, to)
                .selecting_from(from)
                .typed(
                    if start == end {
                        "insertText"
                    } else {
                        crate::editor::keys::WRAP
                    },
                    time,
                );
            state.commit(edit);
        }
        None => typed(
            state,
            (start, end),
            &opening.to_string(),
            "insertText",
            time,
        ),
    }
}

fn search(state: EditorState, query: &str, with: &str, bits: u8) {
    state.search().query.set(query.to_string());
    state.search().replacement.set(with.to_string());
    state.search().options.update(|options| {
        options.case_sensitive = bits & 1 != 0;
        options.whole_word = bits & 2 != 0;
        options.regex = bits & 4 != 0;
    });
}

/// Makes `action`; whether it was an edit the history may have recorded
/// (undo and redo say whether there was a step to take).
pub(super) fn apply(state: EditorState, action: &Action, context: &mut Context) -> bool {
    context.time += 100.0;
    let time = context.time;
    let text = shown(state);
    let lines = text.split('\n').count().max(1);
    match *action {
        Action::Type {
            at,
            text: typed_text,
        } => {
            let at = offset(&text, at);
            place(state, at);
            typed(state, (at, at), typed_text, "insertText", time);
        }
        Action::Backspace { at, count } => {
            let at = offset(&text, at);
            // Never half a surrogate pair.
            let from = snap(&text, at.saturating_sub(count as usize));
            place(state, at);
            if from < at {
                let before = shown(state);
                let (updated, _) = replace_selection(&before, from, at, "");
                state.commit(Edit::input(
                    before,
                    (at, at),
                    updated,
                    (from, from),
                    "deleteContentBackward".into(),
                    time,
                ));
            }
        }
        Action::Enter { at } => {
            let at = offset(&text, at);
            place(state, at);
            enter_at(state, (at, at), Pressed::Enter, Some(time));
        }
        Action::LineBelow { at } => {
            let at = offset(&text, at);
            place(state, at);
            enter_at(state, (at, at), Pressed::LineBelow, None);
        }
        Action::Tab { at, to, shift } => {
            let range = selection(&text, at, to);
            place(state, range.1);
            tab(state, range, shift);
        }
        Action::Line { at, to, command } => {
            let range = selection(&text, at, to);
            place(state, range.1);
            let command = match command % 5 {
                0 => LineCommand::Delete,
                1 => LineCommand::Move { down: false },
                2 => LineCommand::Move { down: true },
                3 => LineCommand::Copy { down: false },
                _ => LineCommand::Copy { down: true },
            };
            run(state, Command::Lines(command), range, false);
        }
        Action::Indent { at, to, outdent } => {
            let range = selection(&text, at, to);
            place(state, range.1);
            run(state, Command::Indent { outdent }, range, false);
        }
        Action::Comment { at, to } => {
            let range = selection(&text, at, to);
            place(state, range.1);
            comment(state, range);
        }
        Action::Cut { at, to } | Action::Copy { at, to } => {
            let range = selection(&text, at, to);
            place(state, range.1);
            if range.0 < range.1 {
                context.clipboard = slice(&text, range);
                if matches!(action, Action::Cut { .. }) {
                    typed(state, range, "", "deleteByCut", time);
                }
            }
        }
        Action::Paste { at, to } => {
            let range = selection(&text, at, to);
            place(state, range.1);
            let pasted = context.clipboard.clone();
            if !pasted.is_empty() || range.0 < range.1 {
                typed(state, range, &pasted, "insertFromPaste", time);
            }
        }
        Action::Pair { at, to, opening } => {
            let range = selection(&text, at, to);
            place(state, range.1);
            pair(state, range, opening, time);
        }
        Action::ReplaceAll {
            query,
            with,
            options,
        } => {
            search(state, query, with, options);
            crate::editor::search::replace_all(state);
        }
        Action::ReplaceOne {
            query,
            with,
            options,
            index,
        } => {
            search(state, query, with, options);
            state.search().open.set(true);
            let found = state
                .search()
                .results
                .with_untracked(|results| results.matches.len());
            if found > 0 {
                state.search().current.set(Some(index as usize % found));
                crate::editor::search::replace_one_match(state);
            }
            state.search().open.set(false);
        }
        Action::Complete { at, pick } => {
            let at = offset(&text, at);
            place(state, at);
            state.completion().requested.set(true);
            let options = state.completion().suggestions.get_untracked();
            if !options.is_empty() {
                let option = options[pick as usize % options.len()].clone();
                state.accept(&option, false);
            }
            state.close_suggestions();
        }
        Action::Fold { line } => {
            state.fold_line(line as usize % lines);
        }
        Action::Unfold { line } => {
            state.unfold_line(line as usize % lines);
        }
        Action::FoldAll => state.fold_all(),
        Action::UnfoldAll => state.unfold_all(),
        Action::Switch { to } => {
            let next = LANGUAGES[to as usize % LANGUAGES.len()];
            if next != state.sync().edit_language.get_untracked() {
                switch_to(state, next);
                // The browser keeps the textarea's caret inside its new
                // text (natively nothing does).
                let end = units(&shown(state));
                if state.doc().caret.get_untracked() > end {
                    state.doc().caret.set(end);
                }
            }
        }
        Action::Pause => pause(state),
        Action::Wait => {
            context.time += 1000.0;
            state.doc().history.update(|history| history.seal());
        }
        Action::Undo | Action::Redo => {
            let redo = matches!(action, Action::Redo);
            return match state.history_edit(redo) {
                Some(step) => {
                    state.commit(step);
                    true
                }
                None => false,
            };
        }
    }
    true
}

/// The UTF-16 offset `at` moved back off the middle of a surrogate pair.
fn snap(text: &str, at: usize) -> usize {
    let mut units = 0;
    for character in text.chars() {
        let next = units + character.len_utf16();
        if next > at {
            return units;
        }
        units = next;
    }
    units.min(at)
}

/// The typing pause (`sync::debounce_validation`, then
/// `sync::convert_edits`).
pub(super) fn pause(state: EditorState) {
    state.recheck_tokens();
    let text = shown(state);
    state.problems().diagnosed_text.set(text.clone());
    let _ = state.problems().diagnostics.get_untracked();
    convert_now(state, text, state.sync().edit_language.get_untracked());
}

/// Whether the caret is inside the text.
pub(super) fn caret_in_range(state: EditorState) -> bool {
    state.doc().caret.get_untracked() <= units(&shown(state))
}

/// Writes `actions` as Rust, one per line.
pub(super) fn code(actions: &[Action]) -> String {
    actions
        .iter()
        .map(|action| format!("            {action:?},"))
        .collect::<Vec<_>>()
        .join("\n")
}
