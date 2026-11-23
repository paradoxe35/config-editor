//! P2: a value the user left alone keeps its host text when the user
//! joins the lines around its item in an XML view, which writes a list
//! item's last child ending where the item does: the joined line still
//! tells each item (and so each untouched value) by its own link. Every
//! host, each holding a string that reads as a number, under schemas that
//! let the view read it as text or as the number.

use leptos::prelude::*;

use super::ops::{self, Op};
use crate::editor::bridge::Bridge;
use crate::editor::languages::Language;
use crate::editor::schema::Schema;
use crate::editor::test_state::TestEditor;

/// The second item's value read as the view reads it: as text, or as the
/// number the schema takes where it refuses the host's string (the first
/// item's text, or any number), which a save of a value the user wrote
/// would keep.
pub(super) const SCHEMAS: &[&str] = &[
    r#"{"type": ["integer", "string"]}"#,
    r#"{"anyOf": [{"type": "integer"}, {"enum": ["x"]}]}"#,
];

/// The schema of the documents, `value` the value's.
pub(super) fn schema(value: &str) -> String {
    format!(
        r#"{{"properties": {{"spec": {{"properties": {{"env": {{"type": "array",
        "items": {{"properties": {{"name": {{"type": "string"}}, "value": {value}}}}}}}}}}}}}}}"#
    )
}

/// Each host's document: a list of two maps inside a map, the second's
/// last value the string `3` in a form of the host's.
pub(super) const HOSTS: &[(Language, &str)] = &[
    (
        Language::YAML,
        "spec:\n  env:\n    - name: a\n      value: x\n    - name: b\n      value: !!str 3\n",
    ),
    (
        Language::YAML,
        "spec:\n  env:\n    - name: a\n      value: x\n    - name: b\n      value: '3'\n",
    ),
    (
        Language::JSON,
        "{\n  \"spec\": {\n    \"env\": [\n      {\"name\": \"a\", \"value\": \"x\"},\n      {\n        \"name\": \"b\",\n        \"value\": \"3\"\n      }\n    ]\n  }\n}\n",
    ),
    (
        Language::TOML,
        "[[spec.env]]\nname = \"a\"\nvalue = \"x\"\n\n[[spec.env]]\nname = \"b\"\nvalue = '3'\n",
    ),
    (
        Language::HCL,
        "spec {\n  env {\n    name = \"a\"\n    value = \"x\"\n  }\n  env {\n    name = \"b\"\n    value = \"3\"\n  }\n}\n",
    ),
];

/// The host's text once each line of the view holding one of `joined`
/// (consecutive lines of the second item, the first first, as Backspace
/// pressed again where the caret was joins them) is joined onto the line
/// above it, and a pause saves.
fn saved_after_joining(
    (host, text): (Language, &str),
    value: &str,
    joined: &[&str],
) -> (String, String) {
    let schema = Schema::compile(&schema(value)).expect("the schema compiles");
    let bridge = Bridge::new(host, Language::XML, Some(schema.bundle.clone())).unwrap();
    let shown = bridge.to_edit(text).expect("shown").text;
    let lines: Vec<&str> = shown.lines().collect();
    // The second item's lines: the last line holding each piece of text.
    let mut at: Vec<usize> = joined
        .iter()
        .map(|piece| {
            lines
                .iter()
                .rposition(|line| line.contains(piece))
                .unwrap_or_else(|| panic!("{piece} in\n{shown}"))
        })
        .collect();
    at.sort_unstable();
    let held = std::sync::Arc::new(std::sync::Mutex::new(text.to_string()));
    let mut editor = TestEditor::new(host, text);
    editor.schema = Some(schema.bundle.clone());
    let told = held.clone();
    editor.on_change = Some(Callback::new(move |text: String| {
        *told.lock().unwrap() = text;
    }));
    editor.edit_as = Some(Language::XML);
    let (_owner, state) = editor.mount();
    let (mut clipboard, mut time) = (None, 0.0);
    let op = Op::Backspace {
        line: at[0],
        column: 0,
        count: at.len(),
    };
    ops::apply(state, host, &op, &mut clipboard, &mut time);
    ops::pause(state);
    let view = state.doc().text.get_untracked();
    let saved = held.lock().unwrap().clone();
    (view, saved)
}

#[test]
fn a_value_left_alone_keeps_its_host_text_when_its_items_lines_are_joined_in_xml() {
    let joins: &[&[&str]] = &[
        &["<value>3"],
        &["<name>b", "<value>3"],
        &["<env>", "<name>b", "<value>3"],
        &["<env>", "<name>b"],
    ];
    for (host, text) in HOSTS {
        for value in SCHEMAS {
            for joined in joins {
                let (view, saved) = saved_after_joining((*host, text), value, joined);
                assert_eq!(
                    saved, *text,
                    "{host} host, value {value}, {joined:?} joined in the XML view:\n{view}"
                );
            }
        }
    }
}
