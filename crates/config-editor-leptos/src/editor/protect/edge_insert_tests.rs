//! Text inserted exactly at a host range's edge goes outside it: before it
//! at its start, after it at its end. A line added above a locked one
//! (Enter at its start, Ctrl+Shift+Enter, a line pasted there) is free,
//! takes typing and saves; the locked text stays as it was, and undo takes
//! the user's own text back. In every language, for a range over a whole
//! line, over a line's text, and over the field (`range_of`).
//!
//! The range used to take in what was inserted at its start: the new line
//! joined the lock, the next key went in, and every key after it (Backspace
//! and undo included) was refused, the text left broken (QA 32, HO32-1).

use leptos::prelude::*;
use serde_json::Value;

use super::super::Language;
use super::super::codec;
use super::super::edit::Edit;
use super::super::keys::enter::{Pressed, enter_at};
use super::super::paths::{TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::test_state::TestEditor;

/// A document whose `host` field is locked, its path, the locked line, and
/// a key line the user adds (without indentation).
fn document(language: Language) -> (&'static str, &'static str, &'static str, &'static str) {
    match language.label() {
        "YAML" => (
            "meta:\n  name: a\n  host: h\n  port: 80\n",
            "/meta/host",
            "  host: h",
            "x: 1",
        ),
        "JSON" => (
            "{\n  \"meta\": {\n    \"name\": \"a\",\n    \"host\": \"h\",\n    \"port\": 80\n  }\n}\n",
            "/meta/host",
            "    \"host\": \"h\",",
            "\"x\": 1,",
        ),
        "TOML" => (
            "[meta]\nname = \"a\"\nhost = \"h\"\nport = 80\n",
            "/meta/host",
            "host = \"h\"",
            "x = 1",
        ),
        "HCL" => (
            "meta {\n  name = \"a\"\n  host = \"h\"\n  port = 80\n}\n",
            "/meta/host",
            "  host = \"h\"",
            "x = 1",
        ),
        _ => (
            "<c>\n  <name>a</name>\n  <host>h</host>\n  <port>80</port>\n</c>\n",
            "/c/host",
            "  <host>h</host>",
            "<x>1</x>",
        ),
    }
}

/// The ways a host locks the `host` field.
#[derive(Clone, Copy, Debug)]
enum Style {
    /// The whole line, its line break included (line N to line N+1).
    WholeLine,
    /// The line's text, from its first column to its end.
    LineText,
    /// The field, as `range_of` gives it.
    Field,
}

const STYLES: [Style; 3] = [Style::WholeLine, Style::LineText, Style::Field];

/// How the line above the locked one is opened, at the range's start.
#[derive(Clone, Copy, Debug)]
enum Open {
    Enter,
    LineAbove,
    /// A line break pasted at the range's start.
    Paste,
}

const OPENS: [Open; 3] = [Open::Enter, Open::LineAbove, Open::Paste];

/// How the key is written on the new line.
#[derive(Clone, Copy, Debug)]
enum Write {
    Typed,
    Pasted,
}

fn units(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

fn range(language: Language, text: &str, path: &str, line: &str, style: Style) -> TextRange {
    let index = text.split('\n').position(|l| l == line).expect("the line") + 1;
    match style {
        Style::WholeLine => {
            TextRange::new(TextPosition::new(index, 1), TextPosition::new(index + 1, 1))
        }
        Style::LineText => TextRange::new(
            TextPosition::new(index, 1),
            TextPosition::new(index, line.chars().count() + 1),
        ),
        Style::Field => language.range_of(text, path).expect("the field's range"),
    }
}

/// The host's ranges on screen, as text.
fn locked(state: EditorState) -> Vec<String> {
    let text = state.doc().text.get_untracked();
    state.protect().ranges.with_untracked(|ranges| {
        ranges
            .iter()
            .map(|range| {
                String::from_utf16_lossy(&text.encode_utf16().collect::<Vec<_>>()[range.clone()])
            })
            .collect()
    })
}

fn insert(state: EditorState, at: usize, inserted: &str, kind: &str) -> bool {
    let text = state.doc().text.get_untracked();
    let byte = text
        .char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
        .find(|byte| units(&text, *byte) == at)
        .expect("a char boundary");
    let mut after = text.clone();
    after.insert_str(byte, inserted);
    let caret = at + inserted.encode_utf16().count();
    state.commit(Edit::input(
        text,
        (at, at),
        after,
        (caret, caret),
        kind.into(),
        0.0,
    ))
}

/// `pointer`'s value in `text`, when it parses.
fn value_at(language: Language, text: &str, pointer: &str) -> Option<Value> {
    let document = codec::read(language.behavior(), text, None).ok()?;
    document.pointer(pointer).cloned()
}

/// One case: the line above the lock opened by `open` at the range's
/// start, the key written there by `write`; then every step undone. The
/// failures it finds.
fn case(language: Language, style: Style, open: Open, write: Write) -> Vec<String> {
    let (text, path, line, key) = document(language);
    let range = range(language, text, path, line, style);
    let mut failures = Vec::new();
    let name = format!("{language} {style:?} {open:?} {write:?}");
    let mut editor = TestEditor::new(language, text);
    editor.read_only_ranges = vec![range];
    let (_owner, state) = editor.mount();
    let before_locks = locked(state);
    let pointer = path.to_string();
    let value = value_at(language, text, &pointer);
    let start = state
        .protect()
        .ranges
        .with_untracked(|ranges| ranges[0].start);
    let accepted = match open {
        Open::Enter => {
            enter_at(state, (start, start), Pressed::Enter, None);
            state.doc().text.get_untracked() != text
        }
        Open::LineAbove => {
            enter_at(state, (start, start), Pressed::LineAbove, None);
            state.doc().text.get_untracked() != text
        }
        // After the line's indentation, the break pasted with it.
        Open::Paste => {
            let column = start - units(text, text.rfind('\n').map_or(0, |_| 0));
            let line_start = units(text, text[..text.find(line).expect("the line")].len());
            let indent = &line[..(start - line_start).min(line.len())];
            let _ = column;
            insert(state, start, &format!("\n{indent}"), "insertFromPaste")
        }
    };
    if !accepted {
        failures.push(format!("{name}: the line break was refused"));
        return failures;
    }
    if locked(state) != before_locks {
        failures.push(format!(
            "{name}: the lock took in the line break: {:?}",
            locked(state)
        ));
    }
    // The key on the line above the locked one, indented as it is.
    let indent = &line[..line.len() - line.trim_start().len()];
    let shown = state.doc().text.get_untracked();
    let lines: Vec<&str> = shown.split('\n').collect();
    let Some(locked_index) = lines.iter().position(|l| *l == line) else {
        failures.push(format!("{name}: the locked line changed: {shown:?}"));
        return failures;
    };
    let above = lines[locked_index - 1];
    let wanted = format!("{indent}{key}");
    let Some(rest) = wanted.strip_prefix(above) else {
        failures.push(format!("{name}: the line above is {above:?}"));
        return failures;
    };
    let above_end = units(&shown, lines[..locked_index].join("\n").len());
    match write {
        Write::Typed => {
            for (offset, character) in rest.chars().enumerate() {
                if !insert(
                    state,
                    above_end + offset,
                    &character.to_string(),
                    "insertText",
                ) {
                    failures.push(format!("{name}: {character:?} refused on the new line"));
                    return failures;
                }
            }
        }
        Write::Pasted => {
            if !insert(state, above_end, rest, "insertFromPaste") {
                failures.push(format!(
                    "{name}: the key pasted on the new line was refused"
                ));
                return failures;
            }
        }
    }
    let written = state.doc().text.get_untracked();
    if locked(state) != before_locks {
        failures.push(format!("{name}: the lock moved: {:?}", locked(state)));
    }
    if value_at(language, &written, &pointer) != value {
        failures.push(format!(
            "{name}: the locked value changed or does not parse: {written:?}"
        ));
    }
    if !written.contains(&wanted) {
        failures.push(format!("{name}: the key is not there: {written:?}"));
    }
    // The user's own text taken back, a step at a time.
    for _ in 0..rest.len() + 2 {
        if state.doc().text.get_untracked() == text {
            break;
        }
        match state.history_edit(false) {
            Some(undo) => {
                if !state.commit(undo) {
                    failures.push(format!("{name}: undo refused"));
                    return failures;
                }
            }
            None => break,
        }
    }
    if state.doc().text.get_untracked() != text {
        failures.push(format!(
            "{name}: undo did not take the text back: {:?}",
            state.doc().text.get_untracked()
        ));
    }
    failures
}

#[test]
fn a_line_opened_at_a_ranges_start_is_free_in_every_language() {
    let mut failures = Vec::new();
    for language in Language::ALL.iter().copied() {
        for style in STYLES {
            for open in OPENS {
                for write in [Write::Typed, Write::Pasted] {
                    failures.extend(case(language, style, open, write));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Enter at a range's end, then Backspace: the line break is outside the
/// range, the locked text stays, and the user takes the break back.
#[test]
fn a_line_break_at_a_ranges_end_stays_outside_it_in_every_language() {
    let mut failures = Vec::new();
    for language in Language::ALL.iter().copied() {
        let (text, path, line, _) = document(language);
        for style in STYLES {
            let name = format!("{language} {style:?}");
            let mut editor = TestEditor::new(language, text);
            editor.read_only_ranges = vec![range(language, text, path, line, style)];
            let (_owner, state) = editor.mount();
            let before_locks = locked(state);
            let end = state
                .protect()
                .ranges
                .with_untracked(|ranges| ranges[0].end);
            if !insert(state, end, "\n", "insertLineBreak") {
                failures.push(format!("{name}: the line break was refused"));
                continue;
            }
            if locked(state) != before_locks {
                failures.push(format!(
                    "{name}: the lock took in the line break: {:?}",
                    locked(state)
                ));
            }
            let shown = state.doc().text.get_untracked();
            let mut back = shown.clone();
            back.remove(end);
            let edit = Edit::input(
                shown,
                (end + 1, end + 1),
                back,
                (end, end),
                "deleteContentBackward".into(),
                1.0,
            );
            if !state.commit(edit) {
                failures.push(format!("{name}: Backspace of the line break refused"));
            } else if state.doc().text.get_untracked() != text || locked(state) != before_locks {
                failures.push(format!("{name}: Backspace left {:?}", locked(state)));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
