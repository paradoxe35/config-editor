//! The edits the matrix makes, each as the editor's own key, command or
//! pause would make it, on whatever text the editor shows (a host's or a
//! view's): places are found by the items' keys (`name`, `port`, `host`,
//! `mode`), however the language writes them.

use leptos::prelude::*;

use crate::editor::edit::Edit;
use crate::editor::keys::enter::{Pressed, enter_at};
use crate::editor::keys::line_edit::LineCommand;
use crate::editor::keys::lines::{Command, run};
use crate::editor::state::EditorState;
use crate::editor::sync::convert_now;

/// A kind of edit (or read) the matrix measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EditKind {
    /// The editor mounted on the document, its view drawn and its first
    /// typing pause run.
    Open,
    /// A digit typed into the middle item's port, then the typing pause
    /// and a save.
    Type,
    /// Enter at the end of the middle item's port line.
    Enter,
    /// The middle item's port line deleted (Ctrl+Shift+K).
    DeleteLine,
    /// The middle item cut, a typing pause, then pasted before the second.
    CutPausePaste,
    /// The middle item cut and pasted before the second, no pause between.
    CutPaste,
    /// The middle item's port line moved down (Alt+Down).
    MoveLine,
    /// The middle item's port line copied down (Shift+Alt+Down).
    CopyLine,
    /// Replace all of every item's `port` key.
    ReplaceAll,
    /// Select all, then the whole text pasted back with two values changed.
    PasteWhole,
    /// A Replace all undone, then redone, a pause after each.
    UndoRedo,
    /// The suggestions opened on a new line in the middle item, then
    /// filtered by a second letter typed.
    Suggest,
    /// The docs under the pointer on the middle item's port key and value.
    Hover,
    /// The typing pause's check of the document against the schema.
    Validate,
}

impl EditKind {
    #[allow(
        dead_code,
        reason = "used by an open finding's ignored test (none open now)"
    )]
    pub(super) const ALL: [Self; 14] = [
        Self::Open,
        Self::Type,
        Self::Enter,
        Self::DeleteLine,
        Self::CutPausePaste,
        Self::CutPaste,
        Self::MoveLine,
        Self::CopyLine,
        Self::ReplaceAll,
        Self::PasteWhole,
        Self::UndoRedo,
        Self::Suggest,
        Self::Hover,
        Self::Validate,
    ];
}

pub(super) fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The UTF-16 offset of the start of line `line` (from 0) of `text`.
fn line_start(text: &str, line: usize) -> usize {
    text.split_inclusive('\n').take(line).map(units).sum()
}

/// The UTF-16 offset of the end of line `line` (before its newline).
fn line_end(text: &str, line: usize) -> usize {
    line_start(text, line) + units(text.split('\n').nth(line).unwrap_or_default())
}

/// The lines (from 0) of `text` writing the key `name`, in order.
pub(super) fn key_lines(text: &str, name: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let line = line.trim_start();
            let line = line.strip_prefix("- ").unwrap_or(line);
            let line = line.trim_start_matches(['"', '<', '{', ' ']);
            line.strip_prefix(name)
                .is_some_and(|rest| rest.starts_with(['"', '>', ' ', ':', '=']))
        })
        .map(|(index, _)| index)
        .collect()
}

/// The line of the middle item's `name` key.
fn middle(text: &str, name: &str) -> usize {
    let lines = key_lines(text, name);
    assert!(lines.len() > 2, "no items' {name} in\n{text}");
    lines[lines.len() / 2]
}

/// Everything the mounted view reads after an edit: the lines, the code
/// layer's constructs over lines, the fold markers, the suggestions and
/// the change bars.
pub(super) fn read_view(state: EditorState) {
    let _ = state.view().lines.get_untracked();
    let _ = state.view().visible_lines.get_untracked();
    let _ = state.view().spanning.get_untracked();
    let _ = state.view().string_bodies.get_untracked();
    let _ = state.fold().fold_ranges.get_untracked();
    let _ = state.completion().suggestions.get_untracked();
    state.changes().diff.with_untracked(|_| ());
}

/// The typing pause (`sync::debounce_validation`, then
/// `sync::convert_edits`): the placeholders followed, the problems
/// placed, an Edit-as view's text converted and sent to the host.
pub(super) fn pause(state: EditorState) {
    state.recheck_tokens();
    let text = shown(state);
    state.problems().diagnosed_text.set(text.clone());
    let _ = state.problems().diagnostics.get_untracked();
    convert_now(state, text, state.sync().edit_language.get_untracked());
}

/// `typed` input at UTF-16 `at`, as the browser's input sends it.
fn type_at(state: EditorState, at: usize, typed: &str) {
    let before = shown(state);
    let byte = crate::editor::keyboard::utf16_to_byte_index(&before, at);
    let mut text = before.clone();
    text.insert_str(byte, typed);
    let caret = at + units(typed);
    state.commit(Edit::input(
        before,
        (at, at),
        text,
        (caret, caret),
        "insertText".into(),
        0.0,
    ));
}

/// The UTF-16 offset just past the first digit of the middle item's port.
fn port_digit_end(text: &str) -> usize {
    let line = middle(text, "port");
    let written = text.split('\n').nth(line).unwrap_or_default();
    let digit = written
        .find(|character: char| character.is_ascii_digit())
        .unwrap_or_else(|| panic!("no digit in {written:?}"));
    line_start(text, line) + units(&written[..=digit])
}

/// The lines of the middle item and of the one before it, as the text
/// writes them: from the line after the one before's last key through the
/// middle one's last key (`last`): the closer of the one before and the
/// opener of the middle one included, so the lines put back before
/// another item's opener make a whole item again, in every language.
fn middle_block(text: &str, last: &str) -> (usize, usize) {
    let lasts = key_lines(text, last);
    let middle = lasts.len() / 2;
    (lasts[middle - 1] + 1, lasts[middle] + 1)
}

/// The middle item cut (its lines, see [`middle_block`]); the text cut.
fn cut_middle(state: EditorState, last: &str) -> String {
    let before = shown(state);
    let (first, end) = middle_block(&before, last);
    let (from, to) = (line_start(&before, first), line_start(&before, end));
    let start = crate::editor::keyboard::utf16_to_byte_index(&before, from);
    let stop = crate::editor::keyboard::utf16_to_byte_index(&before, to);
    let lines = before[start..stop].to_string();
    let text = format!("{}{}", &before[..start], &before[stop..]);
    state.commit(Edit::new(before, (from, to), text, from));
    lines
}

/// `lines` pasted after the first item's last key line.
fn paste_after_first(state: EditorState, lines: &str, last: &str) {
    let before = shown(state);
    let line = key_lines(&before, last)[0] + 1;
    let at = line_start(&before, line);
    let byte = crate::editor::keyboard::utf16_to_byte_index(&before, at);
    let text = format!("{}{lines}{}", &before[..byte], &before[byte..]);
    state.commit(Edit::new(before, (at, at), text, at + units(lines)));
}

/// Every item's `port` key replaced with `listen` by the find widget's
/// Replace all.
fn replace_ports(state: EditorState) {
    state.search().query.set("port".into());
    state.search().replacement.set("listen".into());
    crate::editor::search::replace_all(state);
    assert!(
        key_lines(&shown(state), "port").is_empty(),
        "Replace all left a port:\n{}",
        shown(state)
    );
}

/// The whole text selected and pasted back with the first and the last
/// port's first digit changed.
fn paste_whole(state: EditorState) {
    let before = shown(state);
    let lines = key_lines(&before, "port");
    let mut written: Vec<String> = before.split('\n').map(str::to_string).collect();
    for line in [lines[0], lines[lines.len() - 1]] {
        if let Some(digit) = written[line].find(|character: char| character.is_ascii_digit()) {
            written[line].replace_range(digit..=digit, "7");
        }
    }
    let text = written.join("\n");
    let caret = units(&text);
    state.commit(Edit::new(before.clone(), (0, units(&before)), text, caret));
}

/// The setup an edit needs before it is counted (the edit to undo).
pub(super) fn prepare(state: EditorState, kind: EditKind) {
    if kind == EditKind::UndoRedo {
        replace_ports(state);
        read_view(state);
        pause(state);
    }
}

/// Makes `kind`'s edit (not [`EditKind::Open`]), `last` being the key
/// each item's lines end with.
pub(super) fn make(state: EditorState, kind: EditKind, last: &str) {
    match kind {
        EditKind::Open => unreachable!("the mount itself"),
        EditKind::Type => {
            type_at(state, port_digit_end(&shown(state)), "2");
            read_view(state);
            pause(state);
            let _ = state.save();
        }
        EditKind::Enter => {
            let text = shown(state);
            let at = line_end(&text, middle(&text, "port"));
            enter_at(state, (at, at), Pressed::Enter, None);
            read_view(state);
            pause(state);
        }
        EditKind::DeleteLine | EditKind::MoveLine | EditKind::CopyLine => {
            let text = shown(state);
            let at = line_start(&text, middle(&text, "port"));
            let command = match kind {
                EditKind::DeleteLine => LineCommand::Delete,
                EditKind::MoveLine => LineCommand::Move { down: true },
                _ => LineCommand::Copy { down: true },
            };
            run(state, Command::Lines(command), (at, at), false);
            read_view(state);
            pause(state);
        }
        EditKind::CutPausePaste | EditKind::CutPaste => {
            let lines = cut_middle(state, last);
            read_view(state);
            if kind == EditKind::CutPausePaste {
                pause(state);
            }
            paste_after_first(state, &lines, last);
            read_view(state);
            pause(state);
        }
        EditKind::ReplaceAll => {
            replace_ports(state);
            read_view(state);
            pause(state);
        }
        EditKind::PasteWhole => {
            paste_whole(state);
            read_view(state);
            pause(state);
        }
        EditKind::UndoRedo => {
            for redo in [false, true] {
                let step = state.history_edit(redo).expect("a step to take");
                state.commit(step);
                read_view(state);
                pause(state);
            }
        }
        EditKind::Suggest => {
            let text = shown(state);
            let at = line_end(&text, middle(&text, "port"));
            enter_at(state, (at, at), Pressed::Enter, None);
            state.completion().requested.set(true);
            let at = state.doc().caret.get_untracked();
            type_at(state, at, "h");
            let _ = state.completion().suggestions.get_untracked();
            type_at(state, at + 1, "o");
            let _ = state.completion().suggestions.get_untracked();
            read_view(state);
        }
        EditKind::Hover => {
            let text = shown(state);
            let line = middle(&text, "port");
            let written = text.split('\n').nth(line).unwrap_or_default();
            let key = written.find("port").expect("the key");
            let key = written[..key].chars().count();
            // The value's first digit: on the value, at any size.
            let value = written
                .find(|character: char| character.is_ascii_digit())
                .map_or(0, |digit| written[..digit].chars().count());
            for column in [key + 1, value] {
                let _ = state.docs_under_pointer(line, column);
            }
        }
        EditKind::Validate => pause(state),
    }
}
