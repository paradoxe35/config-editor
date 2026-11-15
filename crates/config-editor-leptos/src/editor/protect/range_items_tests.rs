//! A host range protects its text: a new item or block added above, between
//! or below a list item holding a range (its value, its line, or the whole
//! item), pasted at once or typed a character at a time, and a second block
//! turning a lone block into a list, leave the range's text as it was, so
//! they are let through in every language; the range follows its text and
//! still refuses typing there. Moving the range's text under another
//! parent, or changing the item it is in, is still refused.
//!
//! The fields a range covers are locked by path too (`locks::fields::
//! covered`), and the edit used to be compared at the old path: an item
//! added above put another value at `/servers/0/host`, and a second block
//! turned `/server/host` into `/server/0/host`, so it was refused ("This
//! text is read-only", or "would no longer read as the host gave it").

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::test_state::TestEditor;

/// A document holding a list, the locked value, and the lines of the item
/// holding it.
struct Case {
    name: &'static str,
    language: Language,
    text: &'static str,
    /// The locked value, written once in `text`.
    value: &'static str,
    /// The item's first and last lines (whole, as written).
    item: (&'static str, &'static str),
    /// Each edit adding an item: where (a byte offset of `text`) and what.
    adds: Vec<(usize, &'static str)>,
}

/// The byte offset where line `line` (one-based) of `text` starts.
fn line_start(text: &str, line: usize) -> usize {
    text.split_inclusive('\n')
        .take(line - 1)
        .map(str::len)
        .sum()
}

/// The byte offset where line `line` of `text` ends (before its `\n`).
fn line_end(text: &str, line: usize) -> usize {
    line_start(text, line + 1) - 1
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let yaml =
        "servers:\n  - host: a.example.com\n    port: 80\n  - host: b.example.com\n    port: 81\n";
    let item = "  - host: new.example.com\n    port: 1\n";
    cases.push(Case {
        name: "YAML list",
        language: Language::YAML,
        text: yaml,
        value: "a.example.com",
        item: ("  - host: a.example.com", "    port: 80"),
        adds: vec![
            (line_start(yaml, 2), item),
            (line_start(yaml, 4), item),
            (yaml.len(), item),
        ],
    });
    let json = "{\n  \"servers\": [\n    {\n      \"host\": \"a.example.com\"\n    },\n    {\n      \"host\": \"b.example.com\"\n    }\n  ]\n}\n";
    cases.push(Case {
        name: "JSON list",
        language: Language::JSON,
        text: json,
        value: "\"a.example.com\"",
        item: ("    {", "    },"),
        adds: vec![
            (line_start(json, 3), "    {\"host\": \"new\"},\n"),
            (line_start(json, 6), "    {\"host\": \"new\"},\n"),
            (line_end(json, 8), ",\n    {\"host\": \"new\"}"),
        ],
    });
    let toml = "[[servers]]\nhost = \"a.example.com\"\n\n[[servers]]\nhost = \"b.example.com\"\n";
    cases.push(Case {
        name: "TOML array of tables",
        language: Language::TOML,
        text: toml,
        value: "\"a.example.com\"",
        item: ("[[servers]]", "host = \"a.example.com\""),
        adds: vec![
            (0, "[[servers]]\nhost = \"new\"\n\n"),
            (line_start(toml, 3), "\n[[servers]]\nhost = \"new\"\n"),
            (toml.len(), "\n[[servers]]\nhost = \"new\"\n"),
        ],
    });
    let hcl = "server {\n  host = \"a.example.com\"\n}\nserver {\n  host = \"b.example.com\"\n}\n";
    let block = "server {\n  host = \"new\"\n}\n";
    cases.push(Case {
        name: "HCL repeated blocks",
        language: Language::HCL,
        text: hcl,
        value: "\"a.example.com\"",
        item: ("server {", "}"),
        adds: vec![(0, block), (line_start(hcl, 4), block), (hcl.len(), block)],
    });
    let lone = "resource \"group\" \"sg\" {\n  name = \"sg\"\n\n  ingress {\n    from_port   = 22\n    description = \"10.0.0.0/8\"\n  }\n}\n";
    let ingress = "  ingress {\n    from_port = 443\n  }\n";
    cases.push(Case {
        name: "HCL lone block made a list",
        language: Language::HCL,
        text: lone,
        value: "\"10.0.0.0/8\"",
        item: ("  ingress {", "  }"),
        adds: vec![
            (line_start(lone, 4), ingress),
            (line_start(lone, 8), ingress),
        ],
    });
    let provider = "provider \"aws\" {\n  access_key = \"eu-west-1\"\n}\n";
    let alias = "provider \"aws\" {\n  alias = \"us\"\n}\n";
    cases.push(Case {
        name: "HCL lone labelled block made a list",
        language: Language::HCL,
        text: provider,
        value: "\"eu-west-1\"",
        item: ("provider \"aws\" {", "}"),
        adds: vec![(0, alias), (provider.len(), alias)],
    });
    let attrs = "<config>\n  <servers>\n    <server host=\"a.example.com\" port=\"80\"/>\n    <server host=\"b.example.com\" port=\"81\"/>\n  </servers>\n</config>\n";
    let server = "    <server host=\"new\" port=\"1\"/>\n";
    cases.push(Case {
        name: "XML attributes",
        language: Language::XML,
        text: attrs,
        value: "a.example.com",
        item: (
            "    <server host=\"a.example.com\" port=\"80\"/>",
            "    <server host=\"a.example.com\" port=\"80\"/>",
        ),
        adds: vec![
            (line_start(attrs, 3), server),
            (line_start(attrs, 4), server),
            (line_start(attrs, 5), server),
        ],
    });
    let texts = "<config>\n  <keys>\n    <key id=\"a\">k1</key>\n    <key id=\"b\">k2</key>\n  </keys>\n</config>\n";
    let key = "    <key id=\"c\">new</key>\n";
    cases.push(Case {
        name: "XML text",
        language: Language::XML,
        text: texts,
        value: "k1",
        item: ("    <key id=\"a\">k1</key>", "    <key id=\"a\">k1</key>"),
        adds: vec![
            (line_start(texts, 3), key),
            (line_start(texts, 4), key),
            (line_start(texts, 5), key),
        ],
    });
    let single = "<config>\n  <keys>\n    <key id=\"a\">k1</key>\n  </keys>\n</config>\n";
    cases.push(Case {
        name: "XML lone element made a list",
        language: Language::XML,
        text: single,
        value: "k1",
        item: ("    <key id=\"a\">k1</key>", "    <key id=\"a\">k1</key>"),
        adds: vec![(line_start(single, 3), key), (line_start(single, 4), key)],
    });
    cases
}

/// How the range is placed.
#[derive(Clone, Copy, Debug)]
enum Form {
    Value,
    Line,
    Item,
}

const FORMS: [Form; 3] = [Form::Value, Form::Line, Form::Item];

/// How the item is added.
#[derive(Clone, Copy, Debug)]
enum Way {
    Paste,
    Type,
}

/// The one-based position of byte `at` of `text`.
fn position(text: &str, at: usize) -> TextPosition {
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    TextPosition::new(line, column)
}

fn units(text: &str, at: usize) -> usize {
    text[..at].encode_utf16().count()
}

/// The byte range `form` covers in `case`'s text.
fn covered(case: &Case, form: Form) -> (usize, usize) {
    let text = case.text;
    let line_of = |line: &str| {
        text.match_indices(line)
            .map(|(at, _)| at)
            .find(|at| *at == 0 || text.as_bytes()[at - 1] == b'\n')
            .unwrap_or_else(|| panic!("no line {line:?}"))
    };
    let first = line_of(case.item.0);
    let code = first + case.item.0.len() - case.item.0.trim_start().len();
    match form {
        Form::Value => {
            let at = text.find(case.value).expect("the value");
            (at, at + case.value.len())
        }
        Form::Line => {
            let at = text.find(case.value).expect("the value");
            let start = text[..at].rfind('\n').map_or(0, |end| end + 1);
            let end = at + text[at..].find('\n').unwrap_or(text.len() - at);
            let code = start + text[start..end].len() - text[start..end].trim_start().len();
            (code, end)
        }
        Form::Item => {
            let last = text[first..]
                .match_indices(case.item.1)
                .map(|(at, _)| first + at)
                .find(|at| *at == 0 || text.as_bytes()[at - 1] == b'\n')
                .expect("the item's last line");
            (code, last + case.item.1.len())
        }
    }
}

fn mount(case: &Case, form: Form) -> (Owner, EditorState) {
    let (start, end) = covered(case, form);
    let text = case.text;
    TestEditor {
        read_only_ranges: vec![TextRange::new(position(text, start), position(text, end))],
        ..TestEditor::new(case.language, text)
    }
    .mount()
}

/// Adds `add` at byte `at` of the editor's text `way`; whether every step
/// was let through.
fn add(state: EditorState, at: usize, add: &str, way: Way) -> bool {
    let text = state.doc().text.get_untracked();
    match way {
        Way::Paste => {
            let mut after = text.clone();
            after.insert_str(at, add);
            let start = units(&text, at);
            let caret = start + add.encode_utf16().count();
            let edit = Edit::input(
                text,
                (start, start),
                after,
                (caret, caret),
                "insertFromPaste".into(),
                0.0,
            );
            state.commit(edit)
        }
        Way::Type => {
            let mut at = at;
            // A line typed above another starts with Enter, then the line
            // (a key typed against a range's first word glues onto it).
            let order: Vec<(char, bool)> = match add.strip_suffix('\n') {
                Some(line) => std::iter::once(('\n', false))
                    .chain(line.chars().map(|typed| (typed, true)))
                    .collect(),
                None => add.chars().map(|typed| (typed, true)).collect(),
            };
            for (step, (typed, advance)) in order.into_iter().enumerate() {
                let text = state.doc().text.get_untracked();
                let mut after = text.clone();
                after.insert(at, typed);
                let start = units(&text, at);
                let caret = start + 1;
                let kind = if typed == '\n' {
                    "insertLineBreak"
                } else {
                    "insertText"
                };
                let edit = Edit::input(
                    text,
                    (start, start),
                    after,
                    (caret, caret),
                    kind.into(),
                    step as f64 * 2000.0,
                );
                if !state.commit(edit) {
                    return false;
                }
                if advance {
                    at += typed.len_utf8();
                }
            }
            true
        }
    }
}

/// Whether a character typed into the locked value is refused.
fn value_refuses_typing(state: EditorState, value: &str) -> bool {
    let text = state.doc().text.get_untracked();
    let Some(found) = text.find(value) else {
        return false;
    };
    let inside = found + value.len() / 2;
    let at = units(&text, inside);
    let mut typed = text.clone();
    typed.insert(inside, 'x');
    let edit = Edit::input(
        text,
        (at, at),
        typed,
        (at + 1, at + 1),
        "insertText".into(),
        0.0,
    );
    !state.commit(edit)
}

#[test]
fn an_item_added_beside_a_range_locked_item_is_let_through_and_the_range_follows() {
    let mut failures = Vec::new();
    for case in cases() {
        for form in FORMS {
            for (at, added) in &case.adds {
                for way in [Way::Paste, Way::Type] {
                    let (_owner, state) = mount(&case, form);
                    let mut expected = case.text.to_string();
                    expected.insert_str(*at, added);
                    let taken = add(state, *at, added, way);
                    let now = state.doc().text.get_untracked();
                    if !taken || now != expected {
                        failures.push(format!(
                            "{} {form:?} {way:?} at {at}: refused\n{now}",
                            case.name
                        ));
                        continue;
                    }
                    if !value_refuses_typing(state, case.value) {
                        failures.push(format!(
                            "{} {form:?} {way:?} at {at}: the range did not follow\n{now}",
                            case.name
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

/// The range's line moved into the other item (a cut and paste at once)
/// changes the field it covers: still refused.
#[test]
fn a_range_locked_line_moved_into_another_item_is_still_refused() {
    let text = "servers:\n  - host: a.example.com\n    port: 80\n  - name: b\n    port: 81\n";
    let moved = "servers:\n  - port: 80\n  - name: b\n    host: a.example.com\n    port: 81\n";
    for form in [Form::Value, Form::Line] {
        let case = Case {
            name: "YAML",
            language: Language::YAML,
            text,
            value: "a.example.com",
            item: ("  - host: a.example.com", "    port: 80"),
            adds: Vec::new(),
        };
        let (_owner, state) = mount(&case, form);
        let edit = Edit::new(
            text.to_string(),
            (0, units(text, text.len())),
            moved.to_string(),
            0,
        );
        assert!(!state.commit(edit), "{form:?}");
        assert_eq!(state.doc().text.get_untracked(), text, "{form:?}");
    }
}
