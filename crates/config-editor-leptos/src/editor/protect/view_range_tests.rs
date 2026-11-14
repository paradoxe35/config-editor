//! A host's read-only range in an Edit-as view locks the field it covers
//! however the host's text moves under it: a key added above or below it,
//! or one deleted above it, then any second edit (a key typed and taken
//! back, an undo, typing in the new key), each converted back after a
//! pause or not. The range was read again at its first positions in each
//! new host text, so after the second edit it locked the field above.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::sync::{convert_now, switch_to};
use super::super::test_state::TestEditor;
use super::guarantee_tests::utf16;

/// A host text with `name`, `port` and `host`, and where `port` is.
fn host_text(language: Language) -> (&'static str, &'static str, &'static str) {
    if language == Language::YAML {
        ("name: a\nport: 80\nhost: h\n", "port: 80", "/port")
    } else if language == Language::JSON {
        (
            "{\n  \"name\": \"a\",\n  \"port\": 80,\n  \"host\": \"h\"\n}\n",
            "\"port\": 80",
            "/port",
        )
    } else if language == Language::XML {
        (
            "<c>\n  <name>a</name>\n  <port>80</port>\n  <host>h</host>\n</c>\n",
            "<port>80</port>",
            "/c/port",
        )
    } else {
        (
            "name = \"a\"\nport = 80\nhost = \"h\"\n",
            "port = 80",
            "/port",
        )
    }
}

/// A new key's line in the view's language, written after the end of the
/// line holding `beside`, indented as it is.
fn new_line(state: EditorState, view: Language, beside: &str) -> String {
    let text = shown(state);
    let at = text.find(beside).expect("the line");
    let start = text[..at].rfind('\n').map_or(0, |at| at + 1);
    let indent = &text[start..start + text[start..].len() - text[start..].trim_start().len()];
    let key = if view == Language::JSON {
        "\"extra\": 1,"
    } else if view == Language::TOML {
        "extra = 1"
    } else {
        "extra: 1"
    };
    format!("\n{indent}{key}")
}

#[derive(Clone, Copy, Debug)]
enum Change {
    AddAbove,
    AddBelow,
    DeleteAbove,
}

#[derive(Clone, Copy, Debug)]
enum Second {
    TypedAndTakenBack,
    Undo,
    TypedInNewKey,
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

fn host(state: EditorState) -> String {
    state.sync().host_text.get_value()
}

fn pause(state: EditorState) {
    let language = state.sync().edit_language.get_untracked();
    convert_now(state, shown(state), language);
}

/// Replaces bytes `range` of the text on screen with `with`.
fn input(state: EditorState, range: std::ops::Range<usize>, with: &str, kind: &str) -> bool {
    let before = shown(state);
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let (start, end) = (utf16(&before, range.start), utf16(&before, range.end));
    let caret = start + with.encode_utf16().count();
    state.commit(Edit::input(
        before,
        (start, end),
        after,
        (caret, caret),
        kind.into(),
        0.0,
    ))
}

/// Types `typed` at byte `at`, a key at a time; whether all was typed.
fn type_at(state: EditorState, at: usize, typed: &str) -> bool {
    let mut at = at;
    for character in typed.chars() {
        let kind = if character == '\n' {
            "insertLineBreak"
        } else {
            "insertText"
        };
        if !input(state, at..at, &character.to_string(), kind) {
            return false;
        }
        at += character.len_utf8();
    }
    true
}

/// The end of the line of the text on screen holding `needle`.
fn line_end(state: EditorState, needle: &str) -> usize {
    let text = shown(state);
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("{needle} in {text}"));
    at + text[at..].find('\n').unwrap_or(text.len() - at)
}

/// The byte just after the value written after `key` on screen (before a
/// closing quote or comma).
fn value_end(state: EditorState, key: &str) -> usize {
    let text = shown(state);
    let end = line_end(state, key);
    let line = &text[..end];
    end - (line.len() - line.trim_end_matches([',', '"', ';']).len())
}

fn locked(state: EditorState) -> Vec<String> {
    state.protect().protected.with_untracked(|protected| {
        protected
            .locks
            .iter()
            .filter_map(|lock| lock.pointer.clone())
            .collect()
    })
}

fn mount(host_language: Language, view: Language) -> (Owner, EditorState) {
    let (text, port, _) = host_text(host_language);
    let at = text.find(port).expect("port");
    let line = text[..at].matches('\n').count() + 1;
    let column = at - text[..at].rfind('\n').map_or(0, |at| at + 1) + 1;
    TestEditor {
        edit_as: Some(view),
        read_only_ranges: vec![TextRange::new(
            TextPosition::new(line, column),
            TextPosition::new(line, column + port.len()),
        )],
        ..TestEditor::new(host_language, text)
    }
    .mount()
}

/// Runs one case; what went wrong, if anything.
fn run_case(
    host_language: Language,
    view: Language,
    change: Change,
    second: Second,
    (first_pause, second_pause): (bool, bool),
) -> Vec<String> {
    let (_owner, state) = mount(host_language, view);
    let (_, port, pointer) = host_text(host_language);
    let mut failures = Vec::new();
    let case = format!(
        "{host_language:?}->{view:?} {change:?} {second:?} pauses {first_pause}/{second_pause}"
    );
    match change {
        Change::AddAbove => assert!(
            type_at(
                state,
                line_end(state, "name"),
                &new_line(state, view, "name")
            ),
            "{case}: {:?}",
            shown(state)
        ),
        Change::AddBelow => assert!(
            type_at(
                state,
                line_end(state, "port"),
                &new_line(state, view, "port")
            ),
            "{case}: {:?}",
            shown(state)
        ),
        Change::DeleteAbove => {
            let text = shown(state);
            let start = text.find("name").expect("name");
            let start = text[..start].rfind('\n').map_or(0, |at| at + 1);
            let end = line_end(state, "name") + 1;
            assert!(input(state, start..end, "", "deleteContentBackward"));
        }
    }
    if first_pause {
        pause(state);
    }
    // The second edit, somewhere free.
    let free = if matches!(change, Change::DeleteAbove) {
        "host"
    } else {
        "name"
    };
    let made = match second {
        Second::TypedAndTakenBack => {
            let at = value_end(state, free);
            type_at(state, at, "9") && input(state, at..at + 1, "", "deleteContentBackward")
        }
        Second::Undo => {
            type_at(state, value_end(state, free), "9")
                && state
                    .history_edit(false)
                    .is_some_and(|edit| state.commit(edit))
        }
        Second::TypedInNewKey => {
            let key = if matches!(change, Change::DeleteAbove) {
                "host"
            } else {
                "extra"
            };
            type_at(state, value_end(state, key), "2")
        }
    };
    if !made {
        failures.push(format!(
            "{case}: the second edit was refused: {:?}",
            shown(state)
        ));
    }
    if second_pause {
        pause(state);
    }
    if locked(state) != [pointer] {
        failures.push(format!("{case}: locks {:?}", locked(state)));
    }
    if type_at(state, value_end(state, "port"), "1") {
        failures.push(format!(
            "{case}: the locked port took typing: {:?}",
            shown(state)
        ));
    }
    // A free field stays free.
    if !type_at(state, value_end(state, "host"), "Z") {
        failures.push(format!("{case}: the free host was refused"));
    }
    pause(state);
    let saved = host(state);
    if !saved.contains(port) || saved.contains("801") || saved.contains("180") {
        failures.push(format!("{case}: the port was saved changed: {saved:?}"));
    }
    if !saved.contains("Z") {
        failures.push(format!("{case}: the free edit was not saved: {saved:?}"));
    }
    // Back in the host's language, the range is on the port still.
    switch_to(state, host_language);
    if state.sync().edit_language.get_untracked() == host_language {
        let text = shown(state);
        let at = text.find("80").expect("port value") + 2;
        if type_at(state, at, "1") {
            failures.push(format!(
                "{case}: back in the host's language the port took typing: {:?}",
                shown(state)
            ));
        }
        let at = text.find('Z').map(|at| at + 1);
        if !at.is_some_and(|at| type_at(state, at, "Y")) {
            failures.push(format!(
                "{case}: back in the host's language the host was refused"
            ));
        }
    }
    failures
}

#[test]
fn a_hosts_range_locks_its_field_in_a_view_however_the_text_moves_under_it() {
    let pairs = [
        (Language::YAML, Language::JSON),
        (Language::YAML, Language::TOML),
        (Language::JSON, Language::YAML),
        (Language::JSON, Language::TOML),
        (Language::TOML, Language::YAML),
        (Language::TOML, Language::JSON),
        (Language::HCL, Language::JSON),
        (Language::HCL, Language::YAML),
        (Language::XML, Language::YAML),
        (Language::XML, Language::JSON),
    ];
    let mut failures = Vec::new();
    for (host_language, view) in pairs {
        for change in [Change::AddAbove, Change::AddBelow, Change::DeleteAbove] {
            for second in [
                Second::TypedAndTakenBack,
                Second::Undo,
                Second::TypedInNewKey,
            ] {
                for pauses in [(true, true), (true, false), (false, true), (false, false)] {
                    failures.extend(run_case(host_language, view, change, second, pauses));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The ranges are followed once per text the host gets, not on each key:
/// typing in a view of a long document reads the locks after every key.
#[test]
fn a_hosts_ranges_are_followed_once_per_host_text() {
    use super::super::work_count::{Work, counted};
    let followed = |fields: usize| {
        let text: String = (0..fields).map(|at| format!("k{at}: v{at}\n")).collect();
        let middle = fields / 2;
        let locked_field = format!("k{middle}: v{middle}");
        let at = text.find(&locked_field).expect("the locked field");
        let line = text[..at].matches('\n').count() + 1;
        let (_owner, state) = TestEditor {
            edit_as: Some(Language::JSON),
            read_only_ranges: vec![TextRange::new(
                TextPosition::new(line, 1),
                TextPosition::new(line, locked_field.len() + 1),
            )],
            ..TestEditor::new(Language::YAML, &text)
        }
        .mount();
        let pointer = format!("/k{middle}");
        let ((), count) = counted(Work::RangesFollowed, || {
            for _ in 0..2 {
                for _ in 0..20 {
                    assert!(type_at(state, value_end(state, "\"k1\""), "x"));
                    assert_eq!(locked(state), std::slice::from_ref(&pointer));
                }
                pause(state);
                assert!(host(state).contains("k1: v1xxxx"), "{}", host(state));
            }
            assert_eq!(locked(state), std::slice::from_ref(&pointer));
        });
        count
    };
    // Two host texts written: at most one following into each, at any size.
    let (few, many) = (followed(50), followed(1_000));
    assert!(few <= 2 && many <= 2, "{few} and {many}");
}
