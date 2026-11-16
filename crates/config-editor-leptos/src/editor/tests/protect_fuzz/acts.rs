//! The edits of a random protect run, made as the editor's keys, paste,
//! line commands, Replace all, undo and redo make them; an ordinary one
//! (see `checks::ordinary`) must be taken whole.

use leptos::prelude::*;

use super::cases::{NAMES, NEW_SECRET, TOKEN, value};
use super::checks::{Check, ordinary};
use super::mount::{Mounted, Setup};
use super::rng::Rng;
use crate::editor::edit::Edit;
use crate::editor::keyboard::{paired_text, replace_selection};
use crate::editor::keys::line_edit::LineCommand;
use crate::editor::keys::lines::{Command, run_units as run};
use crate::editor::state::EditorState;
use crate::editor::sync::convert_now;
use crate::editor::text::text_size;

/// Short texts typed anywhere (checked only for safety).
const TYPED: &[&str] = &[
    "x", "1", " ", "\n", ":", "\"", "<", "=", "{", "}", ",", "- ",
];

/// Replace all: the query and its replacement.
const REPLACED: &[(&str, &str)] = &[
    ("app", "svc"),
    ("1", "7"),
    ("x", "y"),
    ("hunter2", "pwned"),
    ("secret", "s"),
    ("e", "E"),
    ("db", "dx"),
    ("key", "kei"),
];

#[derive(Clone, Debug)]
pub(in crate::editor) enum Act {
    /// A new entry named `NAMES[name]` after the case's anchor `anchor`,
    /// typed a key at a time or pasted.
    Entry {
        anchor: u8,
        name: u8,
        paste: bool,
    },
    /// `TYPED[text]` typed at `at` (a byte, rounded down to a character).
    Type {
        at: u16,
        text: u8,
    },
    /// Line `line` cut (unless it holds the protected value).
    Cut {
        line: u16,
    },
    /// A line command on line `line`: move down or up, copy down, delete.
    Line {
        line: u16,
        command: u8,
    },
    /// The protected value's line pasted (a copy) at the start of line
    /// `line`.
    PasteCopy {
        line: u16,
    },
    /// Replace all `REPLACED[pair]`.
    ReplaceAll {
        pair: u8,
    },
    /// A new value pasted over the protected value (a secret takes it).
    NewValue,
    Undo,
    Redo,
    /// The typing pause (an Edit-as view writes back).
    Pause,
    Save,
}

pub(super) fn draw(rng: &mut Rng) -> Act {
    match rng.below(100) {
        0..=34 => Act::Entry {
            anchor: rng.below(4) as u8,
            name: rng.below(NAMES.len()) as u8,
            paste: rng.chance(30),
        },
        35..=46 => Act::Type {
            at: rng.small(),
            text: rng.below(TYPED.len()) as u8,
        },
        47..=54 => Act::Cut { line: rng.small() },
        55..=64 => Act::Line {
            line: rng.small(),
            command: rng.below(4) as u8,
        },
        65..=68 => Act::PasteCopy { line: rng.small() },
        69..=75 => Act::ReplaceAll {
            pair: rng.below(REPLACED.len()) as u8,
        },
        76..=79 => Act::NewValue,
        80..=86 => Act::Undo,
        87..=90 => Act::Redo,
        91..=95 => Act::Pause,
        _ => Act::Save,
    }
}

/// Whether a string in `value` holds `token` with other text.
fn beside_text(value: &serde_json::Value, token: &str) -> bool {
    match value {
        serde_json::Value::String(text) => text.contains(token) && text.trim() != token,
        serde_json::Value::Array(items) => items.iter().any(|item| beside_text(item, token)),
        serde_json::Value::Object(map) => map.values().any(|item| beside_text(item, token)),
        _ => false,
    }
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// Whether bytes `range` of the text on screen hold text a lock keeps.
fn cuts_locked(state: EditorState, range: std::ops::Range<usize>) -> bool {
    let range = text_size(range.start)..text_size(range.end);
    state.protect().protected.with_untracked(|protected| {
        protected.locks.iter().any(|lock| {
            lock.ranges
                .iter()
                .any(|locked| locked.start() < range.end && range.start < locked.end())
        })
    })
}

/// Whether an edit replacing `regions` (bytes of the text on
/// screen; an empty one inserts) writes inside text a host range covers,
/// where the editor holds it: the range's own lock or a field it covers.
/// Of two alike lines only the editor knows which one the range followed
/// (a copy pasted above it, or typed into), so the model asks it.
fn writes_in_range(state: EditorState, regions: &[std::ops::Range<usize>]) -> bool {
    state.protect().protected.with_untracked(|protected| {
        protected
            .locks
            .iter()
            .filter(|lock| lock.by_range())
            .flat_map(|lock| &lock.ranges)
            .any(|locked| {
                regions.iter().any(|region| {
                    let (start, end) = (text_size(region.start), text_size(region.end));
                    if start == end {
                        locked.start() < start && start < locked.end()
                    } else {
                        start < locked.end() && locked.start() < end
                    }
                })
            })
    })
}

fn utf16(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// A click elsewhere: typing starts afresh (what `input::click` does).
fn click(state: EditorState) {
    state.forget_typing_aids();
}

/// Replaces bytes `start..end` with `with`, as a paste: whether taken.
fn paste(state: EditorState, start: usize, end: usize, with: &str) -> bool {
    click(state);
    let before = shown(state);
    let after = format!("{}{with}{}", &before[..start], &before[end..]);
    let (from, to) = (utf16(&before, start), utf16(&before, end));
    let caret = from + with.encode_utf16().count();
    state.commit(Edit::new_units(before, (from, to), after, caret))
}

/// Types `typed` at byte `at` a key at a time (with `pairs`, an opening
/// bracket or quote writes its closer, and typing the closer steps over
/// it): whether every key was taken.
fn type_at(state: EditorState, at: usize, typed: &str, pairs: bool) -> bool {
    click(state);
    let behavior = state.behavior();
    let mut caret = utf16(&shown(state), at);
    let mut closers: Vec<usize> = Vec::new();
    for character in typed.chars() {
        let before = shown(state);
        let next = before.encode_utf16().nth(caret);
        if pairs && closers.contains(&caret) && next == Some(character as u16) {
            closers.retain(|at| *at != caret);
            caret += 1;
            continue;
        }
        let paired = pairs
            .then(|| paired_text(&before, caret, caret, character, behavior))
            .flatten();
        let (after, new_caret, closed) = match paired {
            Some((after, at, _)) => (after, at, true),
            None => {
                let (after, at) = replace_selection(&before, caret, caret, &character.to_string());
                (after, at, false)
            }
        };
        let kind = if character == '\n' {
            "insertLineBreak"
        } else {
            "insertText"
        };
        let edit = Edit::input_units(
            before,
            (caret, caret),
            after,
            (new_caret, new_caret),
            kind.into(),
            0.0,
        );
        if !state.commit(edit) {
            return false;
        }
        for at in &mut closers {
            if *at >= caret {
                *at += if closed { 2 } else { 1 };
            }
        }
        if closed {
            closers.push(new_caret);
        }
        caret = new_caret;
    }
    true
}

/// A byte of `text` at a character boundary, from `at`.
fn boundary(text: &str, at: u16) -> usize {
    let mut at = at as usize % (text.len() + 1);
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The byte span of line `index` with its line break.
fn line_span(text: &str, index: usize) -> (usize, usize) {
    let start: usize = text.split_inclusive('\n').take(index).map(str::len).sum();
    let length = text.split_inclusive('\n').nth(index).map_or(0, str::len);
    (start, start + length)
}

/// Makes `act`; an ordinary edit not taken whole is a failure.
pub(super) fn make(setup: &Setup, mounted: &Mounted, act: &Act) -> Result<(), (Check, String)> {
    let state = mounted.state;
    let view = setup.view_language();
    let protected = value(setup.lock);
    let before = shown(state);
    let lines = before.split_inclusive('\n').count().max(1);
    // An ordinary edit must end on `expected`.
    let expect = |taken: bool, expected: &str, what: &str| {
        let now = shown(state);
        if !taken {
            Err((
                Check::OverRefusal,
                format!("{what} refused in {before:?} (ends {now:?})"),
            ))
        } else if now != expected {
            Err((Check::Differs, format!("{what}: {now:?}, not {expected:?}")))
        } else {
            Ok(())
        }
    };
    match act {
        Act::Entry {
            anchor,
            name,
            paste: pasted,
        } => {
            let anchors = setup.case().anchors;
            let anchor = &anchors[*anchor as usize % anchors.len()];
            let after = anchor.after.replace("{v}", protected);
            let Some(found) = before.find(&after) else {
                return Ok(());
            };
            let at = found + after.len();
            let entry = (anchor.entry)(NAMES[*name as usize % NAMES.len()]);
            let expected = format!("{}{entry}{}", &before[..at], &before[at..]);
            let taken = if *pasted {
                paste(state, at, at, &entry)
            } else {
                type_at(state, at, &entry, setup.pairs)
            };
            if ordinary(setup, view, protected, &before, &expected, false)
                && !writes_in_range(state, std::slice::from_ref(&(at..at)))
            {
                return expect(taken, &expected, &format!("entry {entry:?}"));
            }
        }
        Act::Type { at, text } => {
            let at = boundary(&before, *at);
            type_at(state, at, TYPED[*text as usize % TYPED.len()], setup.pairs);
        }
        Act::Cut { line } => {
            let (start, end) = line_span(&before, *line as usize % lines);
            if before[start..end].contains(protected) || start == end {
                return Ok(());
            }
            // Locked text stays locked wherever it went (a read-only item's
            // line moved out of its list): cutting it is refused.
            if cuts_locked(state, start..end) {
                return Ok(());
            }
            let expected = format!("{}{}", &before[..start], &before[end..]);
            let taken = paste(state, start, end, "");
            if ordinary(setup, view, protected, &before, &expected, true)
                && !writes_in_range(state, std::slice::from_ref(&(start..end)))
            {
                return expect(taken, &expected, &format!("cut {:?}", &before[start..end]));
            }
        }
        Act::Line { line, command } => {
            click(state);
            let (start, _) = line_span(&before, *line as usize % lines);
            let units = utf16(&before, start);
            let command = match command % 4 {
                0 => LineCommand::Move { down: true },
                1 => LineCommand::Move { down: false },
                2 => LineCommand::Copy { down: true },
                _ => LineCommand::Delete,
            };
            run(state, Command::Lines(command), (units, units), false);
        }
        Act::PasteCopy { line } => {
            let Some(copied) = before
                .split_inclusive('\n')
                .find(|line| line.contains(protected))
                .map(str::to_string)
            else {
                return Ok(());
            };
            let (start, _) = line_span(&before, *line as usize % lines);
            paste(state, start, start, &copied);
        }
        Act::ReplaceAll { pair } => {
            let (query, with) = REPLACED[*pair as usize % REPLACED.len()];
            click(state);
            state.search().query.set(query.to_string());
            state.search().replacement.set(with.to_string());
            crate::editor::search::replace_all(state);
            // Every match off the protected value's line is ordinary.
            let line = before
                .lines()
                .find(|line| line.contains(protected))
                .unwrap_or_default();
            let expected = before.replace(query, with);
            let matches: Vec<std::ops::Range<usize>> = before
                .match_indices(query)
                .map(|(at, found)| at..at + found.len())
                .collect();
            if !line.contains(query)
                && before.contains(query)
                && ordinary(setup, view, protected, &before, &expected, false)
                && !writes_in_range(state, &matches)
            {
                return expect(true, &expected, &format!("Replace all {query:?}"));
            }
        }
        Act::NewValue => {
            let Some(start) = before.find(protected) else {
                return Ok(());
            };
            let end = start + protected.len();
            // Over a placeholder written with other text in its string, a
            // secret would be read beside that text (and shown with it):
            // refusing it is no over-refusal.
            let glued = view
                .parse(&before)
                .is_ok_and(|document| beside_text(&document, TOKEN));
            let taken = paste(state, start, end, NEW_SECRET);
            if protected == TOKEN && !glued {
                let expected = format!("{}{NEW_SECRET}{}", &before[..start], &before[end..]);
                return expect(taken, &expected, "a new secret value");
            }
        }
        Act::Undo | Act::Redo => {
            click(state);
            if let Some(edit) = state.history_edit(matches!(act, Act::Redo)) {
                state.commit(edit);
            }
        }
        Act::Pause => pause(state),
        Act::Save => {
            state.save();
        }
    }
    Ok(())
}

/// The typing pause: the text on screen written back to the host.
pub(super) fn pause(state: EditorState) {
    convert_now(
        state,
        shown(state),
        state.sync().edit_language.get_untracked(),
    );
}
