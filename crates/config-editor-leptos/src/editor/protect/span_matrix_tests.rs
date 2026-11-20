//! P1 over a matrix of shapes and edits: a placeholder the host gave as a
//! field's whole value is never read with other text in what the editor
//! takes as the user's (unless the save waits, saying why), never reaches
//! the host glued, and no message shows text typed beside it. Every
//! language (the built-in ones and the custom ones), every shape XML makes
//! of a field (an element, an attribute, a repeated field, a repeated
//! parent, a list in a list); text typed before, after and inside the
//! placeholder, at its line's start, key and end; its line and the lines
//! around it cut, pasted, dragged and moved.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;
use serde_json::Value;

use super::super::Language;
use super::super::declared_syntax::{DOT, DOT_BARE};
use super::super::edit::Edit;
use super::super::host_editing::SaveRequest;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run_units};
use super::super::odd_syntax::ODD;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;

const TOKEN: &str = "«secret 0b1c»";
/// Text a user types beside the placeholder: a secret's value once glued.
const TYPED_SECRET: &str = "s3cr3tZ";
/// What is typed, a key at a time.
const PIECES: [&str; 9] = [TYPED_SECRET, "\"", ":", "<", "}", "\n", "- ", "=", " "];

/// The shapes: a language and a host text, the placeholder written `{v}`.
fn shapes() -> Vec<(&'static str, Language, &'static str)> {
    vec![
        (
            "YAML block",
            Language::YAML,
            "db:\n  user: app\n  password: {v}\nport: 1\n",
        ),
        (
            "YAML list",
            Language::YAML,
            "keys:\n  - {v}\n  - other\nport: 1\n",
        ),
        (
            "JSON object",
            Language::JSON,
            "{\n  \"db\": {\n    \"user\": \"app\",\n    \"password\": \"{v}\"\n  },\n  \"port\": 1\n}\n",
        ),
        (
            "JSON list",
            Language::JSON,
            "{\n  \"keys\": [\n    \"{v}\",\n    \"other\"\n  ],\n  \"port\": 1\n}\n",
        ),
        (
            "TOML table",
            Language::TOML,
            "port = 1\n\n[db]\nuser = \"app\"\npassword = \"{v}\"\n",
        ),
        (
            "HCL block",
            Language::HCL,
            "db \"main\" {\n  user = \"app\"\n  password = \"{v}\"\n}\nport = 1\n",
        ),
        (
            "XML element",
            Language::XML,
            "<config>\n  <db>\n    <user>app</user>\n    <password>{v}</password>\n  </db>\n  <port>1</port>\n</config>\n",
        ),
        (
            "XML attribute",
            Language::XML,
            "<config>\n  <db user=\"app\" password=\"{v}\"/>\n  <port>1</port>\n</config>\n",
        ),
        (
            "XML repeated field",
            Language::XML,
            "<config>\n  <db>\n    <password>{v}</password>\n    <password>x</password>\n  </db>\n</config>\n",
        ),
        (
            "XML repeated parent",
            Language::XML,
            "<config>\n  <db>\n    <password>{v}</password>\n  </db>\n  <db>\n    <password>y</password>\n  </db>\n</config>\n",
        ),
        (
            "XML list in list",
            Language::XML,
            "<config>\n  <db>\n    <key>{v}</key>\n    <key>a</key>\n  </db>\n  <db>\n    <key>b</key>\n    <key>c</key>\n  </db>\n</config>\n",
        ),
        (
            "Odd block",
            Language::new(&ODD),
            "db :=\n  user := app\n  password := \"{v}\"\nport := 1\n",
        ),
        (
            "Odd list",
            Language::new(&ODD),
            "keys :=\n  @ \"{v}\"\n  @ other\nport := 1\n",
        ),
        (
            "Dot map",
            Language::new(&DOT),
            "db: <user: 'app'; password: '{v}'>\nport: 1\n",
        ),
        (
            "Dot list",
            Language::new(&DOT),
            "keys:\n  * '{v}'\n  * 'other'\nport: 1\n",
        ),
        (
            "DotBare flat",
            Language::new(&DOT_BARE),
            "user: 'app'\npassword: '{v}'\nport: 1\n",
        ),
    ]
}

/// An edit of the matrix.
#[derive(Clone, Debug)]
enum Step {
    /// `PIECES[piece]` typed a key at a time at a byte.
    Type { at: usize, piece: usize },
    /// Bytes cut.
    Cut { range: std::ops::Range<usize> },
    /// Text pasted at a byte.
    Paste { at: usize, with: String },
    /// Bytes dragged to a byte of the text they leave.
    Drag {
        range: std::ops::Range<usize>,
        to: usize,
    },
    /// A line moved up or down from a byte on it.
    Move { at: usize, down: bool },
}

impl Step {
    /// Whether text typed beside the placeholder, which is a secret once
    /// glued, is written.
    fn beside(&self, token: &std::ops::Range<usize>) -> bool {
        matches!(self, Step::Type { at, piece: 0 } if token.start <= *at && *at <= token.end)
    }
}

/// The lines of `text`: their byte ranges, each with its line break.
fn lines(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut at = 0;
    text.split_inclusive('\n')
        .map(|line| {
            let range = at..at + line.len();
            at += line.len();
            range
        })
        .collect()
}

/// Every step of the matrix on `text`, whose placeholder is at `token`.
fn steps(text: &str, token: &std::ops::Range<usize>) -> Vec<Step> {
    let lines = lines(text);
    let own = lines
        .iter()
        .position(|line| line.start <= token.start && token.end <= line.end)
        .expect("the token's line");
    let line = lines[own].clone();
    let key = line.start + text[line.clone()].len() - text[line.clone()].trim_start().len();
    let line_end = line.end - usize::from(text[line.clone()].ends_with('\n'));
    let inside = token.start + TOKEN.chars().next().map_or(1, char::len_utf8);
    let mut steps = Vec::new();
    for at in [token.start, token.end, inside, line.start, key, line_end] {
        for piece in 0..PIECES.len() {
            steps.push(Step::Type { at, piece });
        }
    }
    let around: Vec<usize> = [own.checked_sub(1), Some(own), Some(own + 1)]
        .into_iter()
        .flatten()
        .filter(|at| *at < lines.len())
        .collect();
    for at in &around {
        steps.push(Step::Cut {
            range: lines[*at].clone(),
        });
    }
    let copy = text[line.clone()].to_string();
    for other in &lines {
        steps.push(Step::Paste {
            at: other.start,
            with: copy.clone(),
        });
        if other.start != line.start && other.start != line.end {
            steps.push(Step::Drag {
                range: line.clone(),
                to: other.start,
            });
        }
        for down in [false, true] {
            steps.push(Step::Move {
                at: other.start,
                down,
            });
        }
    }
    // The rest of the token's line dragged off it, and text dragged in
    // right after the token.
    if token.end < line_end {
        steps.push(Step::Drag {
            range: token.end..line_end,
            to: lines.last().map_or(text.len(), |last| last.start),
        });
    }
    if let Some(next) = lines.get(own + 1) {
        let end = next.end - usize::from(text[next.clone()].ends_with('\n'));
        steps.push(Step::Drag {
            range: next.start..end,
            to: token.end,
        });
    }
    for at in [token.start, token.end] {
        steps.push(Step::Paste {
            at,
            with: TYPED_SECRET.to_string(),
        });
    }
    steps
}

/// The first edits each step also runs after (none, or one leaving the
/// text broken or read otherwise): a quote, a separator or a letter typed
/// at the token's key, the first key renamed, the line above copied over
/// the token's (a new parent key above it).
fn prefixes(text: &str, token: &std::ops::Range<usize>) -> Vec<Option<Step>> {
    let lines = lines(text);
    let own = lines
        .iter()
        .position(|line| line.start <= token.start && token.end <= line.end)
        .expect("the token's line");
    let line = lines[own].clone();
    let key = line.start + text[line.clone()].len() - text[line.clone()].trim_start().len();
    let first = text.find(|c: char| c.is_alphanumeric()).unwrap_or(0) + 1;
    let mut prefixes = vec![
        None,
        Some(Step::Type { at: key, piece: 1 }),
        Some(Step::Type { at: key, piece: 2 }),
        Some(Step::Type { at: key, piece: 4 }),
        Some(Step::Type {
            at: first,
            piece: 8,
        }),
    ];
    if let Some(above) = own.checked_sub(1).map(|at| lines[at].clone()) {
        prefixes.push(Some(Step::Paste {
            at: line.start,
            with: text[above].to_string(),
        }));
    }
    prefixes
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

fn units(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// Browser input of `kind` writing `with` over bytes `range` of the text
/// on screen (the caret after it, a drop's selection over it).
fn input(state: EditorState, range: std::ops::Range<usize>, with: &str, kind: &str) -> bool {
    state.forget_typing_aids();
    let before = shown(state);
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let start = units(&before, range.start);
    let end = start + with.encode_utf16().count();
    let selection = if kind == "insertFromDrop" {
        (start, end)
    } else {
        (end, end)
    };
    state.commit(Edit::input_units(
        before.clone(),
        (start, units(&before, range.end)),
        after,
        selection,
        kind.into(),
        0.0,
    ))
}

/// Runs `step` on the text on screen.
fn run(state: EditorState, step: &Step) {
    match step {
        Step::Type { at, piece } => {
            let mut at = *at;
            for key in PIECES[*piece].chars() {
                let key = key.to_string();
                let kind = if key == "\n" {
                    "insertLineBreak"
                } else {
                    "insertText"
                };
                if !input(state, at..at, &key, kind) {
                    return;
                }
                at += key.len();
            }
        }
        Step::Cut { range } => {
            input(state, range.clone(), "", "deleteByCut");
        }
        Step::Paste { at, with } => {
            input(state, *at..*at, with, "insertFromPaste");
        }
        Step::Drag { range, to } => {
            let moved = shown(state)[range.clone()].to_string();
            if !input(state, range.clone(), "", "deleteByDrag") {
                return;
            }
            let to = if *to > range.start {
                to - range.len()
            } else {
                *to
            };
            input(state, to..to, &moved, "insertFromDrop");
        }
        Step::Move { at, down } => {
            state.forget_typing_aids();
            let at = units(&shown(state), *at);
            run_units(
                state,
                Command::Lines(LineCommand::Move { down: *down }),
                (at, at),
                false,
            );
        }
    }
}

/// The first string of `value` holding the token with other text.
fn glued(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if text.contains(TOKEN) && text.trim() != TOKEN => Some(text.clone()),
        Value::Array(items) => items.iter().find_map(glued),
        Value::Object(map) => map.values().find_map(glued),
        _ => None,
    }
}

/// A mounted editor on `host` (in `language`) with the default
/// placeholder pattern, and every text it sends the host.
fn mount(language: Language, host: &str) -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(language, host);
    editor.secrets = Some(SecretOptions::default());
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|all| all.push(text));
    }));
    editor.on_save = Some(Callback::new(move |request: SaveRequest| {
        heard.update(|all| all.push(request.text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

#[test]
fn no_edit_glues_a_placeholder_or_shows_text_typed_beside_it_in_any_shape_or_language() {
    let mut failures = Vec::new();
    let (mut runs, mut taken, mut held) = (0, 0, 0);
    for (label, language, template) in shapes() {
        let host = template.replace("{v}", TOKEN);
        let start = host.find(TOKEN).expect("the token");
        for first in prefixes(&host, &(start..start + TOKEN.len())) {
            // The text the steps start from, after the first edit.
            let (_owner, state, _) = mount(language, &host);
            if let Some(first) = &first {
                run(state, first);
            }
            let from = shown(state);
            let Some(start) = from.find(TOKEN) else {
                continue;
            };
            let token = start..start + TOKEN.len();
            for step in steps(&from, &token) {
                runs += 1;
                let (_owner, state, heard) = mount(language, &host);
                if let Some(first) = &first {
                    run(state, first);
                }
                run(state, &step);
                let text = shown(state);
                taken += usize::from(text != from);
                let case = || format!("{label}: {first:?} then {step:?} -> {text:?}");
                // On screen: glued only while the save waits, saying why.
                if let Some(value) = language.parse(&text).ok().as_ref().and_then(glued) {
                    held += 1;
                    if !(state.changes_held() && state.save_waits().is_some()) {
                        failures.push(format!("{}: {value:?} on screen, not held", case()));
                    }
                }
                if step.beside(&token) {
                    state.problems().diagnosed_text.set(text.clone());
                    let shown_secret = state
                        .problems()
                        .diagnostics
                        .get_untracked()
                        .into_iter()
                        .map(|problem| problem.message)
                        .chain(state.save_waits())
                        .find(|message| message.contains(TYPED_SECRET));
                    if let Some(message) = shown_secret {
                        failures.push(format!("{}: a message shows it: {message}", case()));
                    }
                }
                convert_now(state, text.clone(), language);
                state.save();
                for sent in heard.get_untracked() {
                    if let Some(value) = language.parse(&sent).ok().as_ref().and_then(glued) {
                        failures.push(format!("{}: the host heard {value:?} in {sent:?}", case()));
                    }
                }
            }
        }
    }
    assert!(
        runs > 5000 && taken > runs / 4 && held > 0,
        "{runs} runs, {taken} taken, {held} held"
    );
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}",
        failures.len()
    );
}
