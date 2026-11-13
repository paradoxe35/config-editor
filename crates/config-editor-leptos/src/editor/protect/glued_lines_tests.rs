//! A placeholder the host gave as a field's whole value takes nothing
//! beside it over lines either: a line typed inside a multi-line value next
//! to it (YAML block scalars and plain continuation lines, TOML `'''` and
//! `"""`, an HCL heredoc, XML text), its line copied inside the value
//! (`Shift+Alt+Down`), and text joined onto it across a line break (Enter
//! after it, a word typed, the break deleted) are refused, so the host
//! never hears the token glued to other text. In every host language, for
//! two unrelated patterns.
//!
//! The lines typed after the token's own line were not read, and a join
//! making the text parse again was let through and only flagged.

use leptos::prelude::*;

use super::super::Language;
use super::super::codec;
use super::super::edit::Edit;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run};
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::{HOSTS, Tokens, utf16};

/// The field holding the token in every sample.
const FIELD: &str = "/key";

/// The host's samples with `token` alone in `/key`, written over lines, and
/// the indent of a line inside its value.
fn multi_line(token: &str) -> Vec<(Language, String, &'static str)> {
    let yaml = |header: &str| {
        (
            Language::YAML,
            format!("name: a\nkey: {header}\n  {token}\nlast: 1\n"),
            "  ",
        )
    };
    vec![
        yaml("|"),
        yaml("|-"),
        yaml("|+"),
        yaml(">"),
        yaml(">-"),
        (
            Language::TOML,
            format!("name = \"a\"\nkey = '''\n{token}\n'''\nlast = 1\n"),
            "",
        ),
        (
            Language::TOML,
            format!("name = \"a\"\nkey = \"\"\"\n{token}\n\"\"\"\nlast = 1\n"),
            "",
        ),
        (
            Language::HCL,
            format!("name = \"a\"\nkey = <<-EOT\n  {token}\n  EOT\nlast = 1\n"),
            "  ",
        ),
        (
            Language::XML,
            format!("<name>a</name>\n<key>\n  {token}\n</key>\n<last>1</last>\n"),
            "  ",
        ),
    ]
}

/// The host's samples with `token` alone in `/key` on one line, where a
/// line break typed right after it stays inside the value or leaves it
/// unparseable (before a closing quote or tag).
fn one_line(tokens: &Tokens) -> Vec<(Language, String, &'static str)> {
    let token = tokens.same;
    let mut samples = vec![
        (
            Language::JSON,
            format!("{{\n  \"name\": \"a\",\n  \"key\": \"{token}\",\n  \"last\": 1\n}}\n"),
            "",
        ),
        (
            Language::TOML,
            format!("name = \"a\"\nkey = \"{token}\"\nlast = 1\n"),
            "",
        ),
        (
            Language::HCL,
            format!("name = \"a\"\nkey = \"{token}\"\nlast = 1\n"),
            "",
        ),
        (
            Language::XML,
            format!("<name>a</name>\n<key>{token}</key>\n<last>1</last>\n"),
            "",
        ),
        (
            Language::YAML,
            format!("name: a\nkey: \"{token}\"\nlast: 1\n"),
            "",
        ),
    ];
    if !tokens.quoted_in_yaml {
        samples.push((
            Language::YAML,
            format!("name: a\nkey: {token}\nlast: 1\n"),
            "",
        ));
    }
    samples
}

/// A new field in `language`, on a line of its own.
fn new_entry(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "new: 2\n",
        "XML" => "<new>2</new>\n",
        _ => "new = 2\n",
    }
}

/// A live editor on `text` with the host's `tokens`, and what `on_change`
/// heard.
fn mount(
    language: Language,
    text: &str,
    tokens: &Tokens,
) -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(language, text);
    editor.secrets = Some(tokens.options());
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

/// Replaces bytes `range` of the text on screen with `with`, as one input.
fn input(state: EditorState, range: std::ops::Range<usize>, with: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let (start, end) = (utf16(&before, range.start), utf16(&before, range.end));
    let caret = start + with.encode_utf16().count();
    let kind = match with {
        "" if range.start + 1 == range.end => "deleteContentForward",
        "" => "deleteContentBackward",
        _ if with.starts_with('\n') => "insertLineBreak",
        _ => "insertText",
    };
    state.commit(Edit::input(
        before,
        (start, end),
        after,
        (caret, caret),
        kind.into(),
        0.0,
    ))
}

/// Types `typed` at byte `at`, one key at a time, until one is refused.
fn type_at(state: EditorState, mut at: usize, typed: &str) {
    for character in typed.chars() {
        if !input(state, at..at, &character.to_string()) {
            return;
        }
        at += character.len_utf8();
    }
}

/// What `/key` holds in `text` when the text parses and it is not the
/// token alone: the token glued to other text.
fn glued(language: Language, text: &str, token: &str) -> Option<String> {
    let document = codec::read(language.behavior(), text, None).ok()?;
    let value = document.pointer(FIELD)?.as_str()?.to_string();
    (value.contains(token) && value.trim() != token).then_some(value)
}

/// Every text the host heard, and the text on screen, hold the token alone.
fn check(
    failures: &mut Vec<String>,
    case: &str,
    language: Language,
    state: EditorState,
    heard: RwSignal<Vec<String>>,
    token: &str,
) {
    let shown = state.doc().text.get_untracked();
    for text in heard.get_untracked().iter().chain([&shown]) {
        if let Some(value) = glued(language, text, token) {
            failures.push(format!("{case}: {value:?} reached the host in {text:?}"));
            return;
        }
    }
}

#[test]
fn a_line_written_beside_a_whole_placeholder_in_its_value_is_refused() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        let token = tokens.same;
        for (language, text, indent) in multi_line(token) {
            let line_start = text.find(token).unwrap() - indent.len();
            let line_end = text.find(token).unwrap() + token.len();
            let header_end = line_start - 1;
            // A line typed after the token's, and before it.
            for (place, at) in [("after", line_end), ("before", header_end)] {
                let (_owner, state, heard) = mount(language, &text, tokens);
                if input(state, at..at, &format!("\n{indent}")) {
                    type_at(state, at + 1 + indent.len(), "MYPASS");
                }
                let case = format!("{language} {text:?}, a line typed {place} it");
                check(&mut failures, &case, language, state, heard, token);
            }
            // Its line copied below.
            let (_owner, state, heard) = mount(language, &text, tokens);
            let caret = utf16(&text, line_end);
            run(
                state,
                Command::Lines(LineCommand::Copy { down: true }),
                (caret, caret),
                false,
            );
            let case = format!("{language} {text:?}, its line copied");
            check(&mut failures, &case, language, state, heard, token);
            // A field typed after the value stays free.
            let (_owner, state, _) = mount(language, &text, tokens);
            let last = text.rfind("last").unwrap() - usize::from(language == Language::XML);
            let entry = new_entry(language);
            type_at(state, last, entry);
            if !state.doc().text.get_untracked().contains(entry) {
                failures.push(format!("{language} {text:?}: {entry:?} after it refused"));
            }
        }
        // A plain scalar's continuation line, typed below the token.
        if !tokens.quoted_in_yaml {
            let text = format!("name: a\nkey: {token}\nlast: 1\n");
            let (_owner, state, heard) = mount(Language::YAML, &text, tokens);
            let end = text.find(token).unwrap() + token.len();
            if input(state, end..end, "\n") {
                type_at(state, end + 1, "  abc");
            }
            let case = format!("{text:?}, a continuation line typed");
            check(&mut failures, &case, Language::YAML, state, heard, token);
        }
        // Joined across a line break: Enter after it, a word, then the
        // break deleted (Delete at the token's line end, Backspace at the
        // word's start).
        let samples = one_line(tokens).into_iter().chain(multi_line(token));
        for (language, text, indent) in samples {
            for delete in [true, false] {
                let (_owner, state, heard) = mount(language, &text, tokens);
                let end = text.find(token).unwrap() + token.len();
                if input(state, end..end, &format!("\n{indent}")) {
                    type_at(state, end + 1 + indent.len(), "abc");
                }
                let now = state.doc().text.get_untracked();
                let range = if delete {
                    end..end + 1 + indent.len()
                } else {
                    end..end + 1
                };
                if now[end..].starts_with('\n') {
                    input(state, range, "");
                }
                let how = if delete { "Delete" } else { "Backspace" };
                let case = format!("{language} {text:?}, a word joined with {how}");
                check(&mut failures, &case, language, state, heard, token);
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
