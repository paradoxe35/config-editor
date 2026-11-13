//! Every line command and `Ctrl+/`, from every line and over every two
//! lines, next to a field locked each way a host can lock one (a read-only
//! path, the schema's `readOnly`, a read-only range over the value, over
//! its line, over a whole block or a block's header line), in the five
//! languages: the text it makes reads the locked value at its path as
//! before, or does not parse (a save then waits); and the locked value's
//! text refuses typing afterwards, wherever it went.
//!
//! A host range used to lock its text alone: `Ctrl+/` commented its field
//! out and the save dropped it, a line moved past it lost the range (the
//! value then took typing), and a copy of its line was written over it. A
//! YAML parent's key moved above the line before it left its lines under
//! that line, and the locks with them.

use leptos::prelude::*;

use super::super::Language;
use super::super::codec;
use super::super::edit::Edit;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run};
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::test_state::TestEditor;

/// The locked value's text, once in each sample.
pub(super) const VALUE: &str = "512";

/// A document with a block (`parent`) holding the field `leaf`, whose
/// value is [`VALUE`]; the lines are given whole.
pub(super) struct Sample {
    pub(super) text: &'static str,
    pub(super) leaf: &'static str,
    pub(super) parent: &'static str,
    pub(super) leaf_line: &'static str,
    pub(super) header_line: &'static str,
    end_line: &'static str,
}

pub(super) fn sample(language: Language) -> Sample {
    match language.label() {
        "YAML" => Sample {
            text: "name: demo\nserver:\n  listen: 7835\n  limits:\n    burst: 4\n    max_pending: 512\n    idle: 30\n  tags:\n    - a\ntail: x\n",
            leaf: "/server/limits/max_pending",
            parent: "/server/limits",
            leaf_line: "    max_pending: 512",
            header_line: "  limits:",
            end_line: "    idle: 30",
        },
        "JSON" => Sample {
            text: "{\n  \"name\": \"demo\",\n  \"server\": {\n    \"listen\": 7835,\n    \"limits\": {\n      \"burst\": 4,\n      \"max_pending\": 512,\n      \"idle\": 30\n    },\n    \"tags\": [\"a\"]\n  },\n  \"tail\": \"x\"\n}\n",
            leaf: "/server/limits/max_pending",
            parent: "/server/limits",
            leaf_line: "      \"max_pending\": 512,",
            header_line: "    \"limits\": {",
            end_line: "    },",
        },
        "TOML" => Sample {
            text: "name = \"demo\"\ntail = \"x\"\n\n[server]\nlisten = 7835\ntags = [\"a\"]\n\n[server.limits]\nburst = 4\nmax_pending = 512\nidle = 30\n",
            leaf: "/server/limits/max_pending",
            parent: "/server/limits",
            leaf_line: "max_pending = 512",
            header_line: "[server.limits]",
            end_line: "idle = 30",
        },
        "HCL" => Sample {
            text: "name = \"demo\"\nserver {\n  listen = 7835\n  limits {\n    burst = 4\n    max_pending = 512\n    idle = 30\n  }\n  tags = [\"a\"]\n}\ntail = \"x\"\n",
            leaf: "/server/limits/max_pending",
            parent: "/server/limits",
            leaf_line: "    max_pending = 512",
            header_line: "  limits {",
            end_line: "  }",
        },
        _ => Sample {
            text: "<config>\n  <name>demo</name>\n  <server>\n    <listen>7835</listen>\n    <limits>\n      <burst>4</burst>\n      <max_pending>512</max_pending>\n      <idle>30</idle>\n    </limits>\n    <tag>a</tag>\n  </server>\n  <tail>x</tail>\n</config>\n",
            leaf: "/config/server/limits/max_pending",
            parent: "/config/server/limits",
            leaf_line: "      <max_pending>512</max_pending>",
            header_line: "    <limits>",
            end_line: "    </limits>",
        },
    }
}

/// How the field is locked.
#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    LeafPath,
    LeafSchema,
    RangeOverValue,
    RangeOverLine,
    ParentPath,
    ParentSchema,
    RangeOverBlock,
    RangeOverHeader,
}

/// The ways the leaf alone is locked.
pub(super) const KINDS_OF_LEAF: [Kind; 4] = [
    Kind::LeafPath,
    Kind::LeafSchema,
    Kind::RangeOverValue,
    Kind::RangeOverLine,
];

const KINDS: [Kind; 8] = [
    Kind::LeafPath,
    Kind::LeafSchema,
    Kind::RangeOverValue,
    Kind::RangeOverLine,
    Kind::ParentPath,
    Kind::ParentSchema,
    Kind::RangeOverBlock,
    Kind::RangeOverHeader,
];

/// What `kind` locks of `sample`: the fields whose values must read as
/// before.
fn locked(sample: &Sample, kind: Kind) -> Vec<&'static str> {
    match kind {
        Kind::LeafPath | Kind::LeafSchema | Kind::RangeOverValue | Kind::RangeOverLine => {
            vec![sample.leaf]
        }
        Kind::ParentPath | Kind::ParentSchema | Kind::RangeOverBlock => {
            vec![sample.parent, sample.leaf]
        }
        // The header's text, the block's place: the block's key holds it.
        Kind::RangeOverHeader => vec![sample.parent],
    }
}

/// The one-based position of byte `at` of `text`.
fn position(text: &str, at: usize) -> TextPosition {
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    TextPosition::new(line, column)
}

/// The byte offsets of the code on the whole line `line` of `text`, and of
/// its end.
fn line_code(text: &str, line: &str) -> (usize, usize) {
    let at = text
        .find(&format!("\n{line}\n"))
        .unwrap_or_else(|| panic!("no line {line:?}"))
        + 1;
    (at + line.len() - line.trim_start().len(), at + line.len())
}

/// A schema marking `pointer` read-only.
fn schema(pointer: &str) -> std::sync::Arc<super::super::diagnostics::SchemaBundle> {
    let mut schema = serde_json::json!({ "readOnly": true });
    for key in pointer.trim_start_matches('/').split('/').rev() {
        schema = serde_json::json!({ "type": "object", "properties": { key: schema } });
    }
    let bundle = super::super::diagnostics::build_schema(&schema.to_string()).expect("a schema");
    std::sync::Arc::new(bundle)
}

pub(super) fn mount(language: Language, kind: Kind) -> (Owner, EditorState) {
    let sample = sample(language);
    let text = sample.text;
    let range =
        |start: usize, end: usize| vec![TextRange::new(position(text, start), position(text, end))];
    let mut editor = TestEditor::new(language, text);
    let (line_start, line_end) = line_code(text, sample.leaf_line);
    let (header_start, header_end) = line_code(text, sample.header_line);
    match kind {
        Kind::LeafPath => editor.read_only_paths = vec![FieldPath::from(sample.leaf)],
        Kind::ParentPath => editor.read_only_paths = vec![FieldPath::from(sample.parent)],
        Kind::LeafSchema => editor.schema = Some(schema(sample.leaf)),
        Kind::ParentSchema => editor.schema = Some(schema(sample.parent)),
        Kind::RangeOverValue => {
            let at = line_start + text[line_start..].find(VALUE).expect("the value");
            editor.read_only_ranges = range(at, at + VALUE.len());
        }
        Kind::RangeOverLine => editor.read_only_ranges = range(line_start, line_end),
        Kind::RangeOverBlock => {
            let (_, end) = line_code(text, sample.end_line);
            editor.read_only_ranges = range(header_start, end);
        }
        Kind::RangeOverHeader => editor.read_only_ranges = range(header_start, header_end),
    }
    editor.mount()
}

pub(super) fn units(text: &str, at: usize) -> usize {
    text[..at].encode_utf16().count()
}

/// Every line command, and `Ctrl+/` (`None`).
const COMMANDS: [(&str, Option<Command>); 8] = [
    ("Ctrl+/", None),
    (
        "Alt+Up",
        Some(Command::Lines(LineCommand::Move { down: false })),
    ),
    (
        "Alt+Down",
        Some(Command::Lines(LineCommand::Move { down: true })),
    ),
    (
        "Shift+Alt+Up",
        Some(Command::Lines(LineCommand::Copy { down: false })),
    ),
    (
        "Shift+Alt+Down",
        Some(Command::Lines(LineCommand::Copy { down: true })),
    ),
    ("Ctrl+Shift+K", Some(Command::Lines(LineCommand::Delete))),
    ("Ctrl+]", Some(Command::Indent { outdent: false })),
    ("Ctrl+[", Some(Command::Indent { outdent: true })),
];

/// `command` on `selection` (UTF-16) of the editor's text, as its key does.
fn press(
    state: EditorState,
    language: Language,
    command: Option<Command>,
    selection: (usize, usize),
) {
    let text = state.doc().text.get_untracked();
    match command {
        Some(command) => run(state, command, selection, false),
        None => {
            if let Some((updated, start, end)) =
                language
                    .behavior()
                    .toggle_comments(&text, selection.0, selection.1)
                && updated != text
            {
                let edit = Edit::new(text, selection, updated, end).selecting_from(start);
                let _ = state.commit(edit);
            }
        }
    }
}

/// Where `VALUE` is written in `text` and a digit typed into it is let
/// through.
pub(super) fn typed_into(state: EditorState) -> Vec<usize> {
    let text = state.doc().text.get_untracked();
    let mut taken = Vec::new();
    for (found, _) in text.match_indices(VALUE) {
        let at = units(&text, found + 1);
        let mut typed = text.clone();
        typed.insert(found + 1, '9');
        let edit = Edit::input(
            text.clone(),
            (at, at),
            typed,
            (at + 1, at + 1),
            "insertText".into(),
            0.0,
        );
        if state.commit(edit) {
            taken.push(found);
            let undo = state.history_edit(false).expect("the digit typed");
            assert!(state.commit(undo));
        }
    }
    taken
}

#[test]
fn a_line_command_or_comment_toggle_never_takes_a_locked_field() {
    let mut failures = Vec::new();
    for language in Language::ALL.iter().copied() {
        let sample = sample(language);
        let text = sample.text;
        let behavior = language.behavior();
        let document = codec::read(behavior, text, None).expect("the sample parses");
        let lines: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
        let starts: Vec<usize> = lines
            .iter()
            .scan(0, |at, line| {
                let start = *at;
                *at += line.len() + 1;
                Some(units(text, start + line.len() - line.trim_start().len()))
            })
            .collect();
        // A caret on each line, and each two lines selected.
        let selections = starts
            .iter()
            .map(|at| (*at, *at))
            .chain(starts.windows(2).map(|pair| (pair[0], pair[1] + 1)));
        let selections: Vec<(usize, usize)> = selections.collect();
        for kind in KINDS {
            for (name, command) in COMMANDS {
                for selection in &selections {
                    let (_owner, state) = mount(language, kind);
                    press(state, language, command, *selection);
                    let after = state.doc().text.get_untracked();
                    if after == text {
                        continue;
                    }
                    let changed: Vec<&str> = match codec::read(behavior, &after, None) {
                        Ok(now) => locked(&sample, kind)
                            .into_iter()
                            .filter(|pointer| now.pointer(pointer) != document.pointer(pointer))
                            .collect(),
                        Err(_) => Vec::new(),
                    };
                    let taken = typed_into(state);
                    if !changed.is_empty() || !taken.is_empty() {
                        failures.push(format!(
                            "{} {kind:?} {name} at {selection:?}: changed {changed:?}, typing taken at {taken:?}\n{after}",
                            language.label()
                        ));
                    }
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

/// With CRLF line ends, `Ctrl+/` on a line a host range covers (its value
/// or the whole line) is refused as well.
#[test]
fn a_comment_toggle_on_a_crlf_line_a_range_covers_is_refused() {
    for language in [Language::YAML, Language::TOML, Language::HCL] {
        let sample = sample(language);
        let text = sample.text.replace('\n', "\r\n");
        let (line_start, line_end) = line_code(sample.text, sample.leaf_line);
        let value = line_start + sample.text[line_start..].find(VALUE).expect("the value");
        // CRLF adds one byte per line above.
        let shift = |at: usize| at + sample.text[..at].matches('\n').count();
        for (start, end) in [(value, value + VALUE.len()), (line_start, line_end)] {
            let (start, end) = (shift(start), shift(end));
            let position = |at: usize| {
                let before = &text[..at];
                let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                TextPosition::new(before.matches('\n').count() + 1, column)
            };
            let (_owner, state) = TestEditor {
                read_only_ranges: vec![TextRange::new(position(start), position(end))],
                ..TestEditor::new(language, &text)
            }
            .mount();
            let shown = state.doc().text.get_untracked();
            let at = units(&shown, shown.find(VALUE).expect("the value"));
            press(state, language, None, (at, at));
            let now = codec::read(language.behavior(), &state.doc().text.get_untracked(), None)
                .expect("parses");
            assert_eq!(
                now.pointer(sample.leaf).and_then(serde_json::Value::as_i64),
                Some(512),
                "{} {start}..{end}",
                language.label()
            );
        }
    }
}
