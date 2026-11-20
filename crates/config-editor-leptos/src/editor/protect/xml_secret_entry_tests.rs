//! A new element typed or pasted beside an XML host's secret element: a
//! list of secrets with attributes (`<key id="a">«secret»</key>`) or
//! without, a secret attribute, or a lone secret element. Above, between
//! and below the secrets, a key at a time with auto-closing on and off,
//! or pasted: beside a list of secrets the new item is taken and is no
//! problem; beside a lone secret, whose placeholder is the field's whole
//! value (a bare element the schema does not make a list), a second
//! element of its name would make the field a list with other text beside
//! the placeholder: refused where it writes at the placeholder, as YAML
//! refuses `password: [«secret», x]`, else taken and held (flagged, the
//! change event and the save waiting). Text glued to a placeholder is
//! still refused.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::host_editing::SaveRequest;
use super::super::save_matrix::report;
use super::super::schema::build_schema;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::problems;
use super::stepped_closer_tests::type_at;
use super::xml_list_secret_tests::write;

const FIRST: &str = "«secret 0b1c»";
const SECOND: &str = "«secret 3f2a»";

/// A host text, its schema, the new entries, and whether the schema makes
/// the secret's field a list (a new element is an item, not flagged).
struct Case {
    name: &'static str,
    text: String,
    schema: String,
    entries: &'static [&'static str],
    list: bool,
}

fn wrap(inner: &str) -> String {
    format!(
        r#"{{"type": "object", "properties": {{"config": {{"type": "object", "properties": {inner}}}}}}}"#
    )
}

fn keys_schema(item: &str) -> String {
    wrap(&format!(
        r#"{{"keys": {{"type": "object", "properties": {{"key": {{"anyOf": [{item}, {{"type": "array", "items": {item}}}]}}}}}}}}"#
    ))
}

fn keys(items: &[&str]) -> String {
    let written: String = items.iter().map(|item| format!("    {item}\n")).collect();
    format!("<config>\n  <keys>\n{written}  </keys>\n</config>\n")
}

fn cases() -> Vec<Case> {
    let text_secret = r##"{"type": "object", "properties": {"id": {"type": "string"},
      "#text": {"type": "string", "x-secret": true}}}"##;
    let id_secret = r##"{"type": "object", "properties": {"id": {"type": "string", "x-secret": true},
      "#text": {"type": "string"}}}"##;
    let string_secret = r#"{"type": "string", "x-secret": true}"#;
    let with_ids = [
        format!(r#"<key id="a">{FIRST}</key>"#),
        format!(r#"<key id="b">{SECOND}</key>"#),
    ];
    let bare = [
        format!("<key>{FIRST}</key>"),
        format!("<key>{SECOND}</key>"),
    ];
    let id_tokens = [
        format!(r#"<key id="{FIRST}">a</key>"#),
        format!(r#"<key id="{SECOND}">b</key>"#),
    ];
    let mut cases = Vec::new();
    for items in [1, 2] {
        let pick =
            |all: &[String; 2]| keys(&all[..items].iter().map(String::as_str).collect::<Vec<_>>());
        cases.push(Case {
            name: if items == 1 {
                "#text secret with id, 1 item"
            } else {
                "#text secret with id, 2 items"
            },
            text: pick(&with_ids),
            schema: keys_schema(text_secret),
            entries: &[r#"<key id="c">new</key>"#, "<key>new</key>"],
            list: true,
        });
        cases.push(Case {
            name: if items == 1 {
                "bare secret items, 1 item"
            } else {
                "bare secret items, 2 items"
            },
            text: pick(&bare),
            schema: keys_schema(string_secret),
            entries: &["<key>new</key>", r#"<key id="c">new</key>"#],
            // One bare element reads as the field's whole value (the schema
            // allows a string there): a second one would make it a list.
            list: items == 2,
        });
        cases.push(Case {
            name: if items == 1 {
                "secret attribute, 1 item"
            } else {
                "secret attribute, 2 items"
            },
            text: pick(&id_tokens),
            schema: keys_schema(id_secret),
            entries: &[r#"<key id="c">new</key>"#],
            list: true,
        });
    }
    let db = format!(
        "<config>\n  <db>\n    <user>app</user>\n    <password>{FIRST}</password>\n  </db>\n</config>\n"
    );
    for (name, password) in [
        ("lone secret, no type", r#"{"x-secret": true}"#),
        (
            "lone secret string",
            r#"{"type": "string", "x-secret": true}"#,
        ),
    ] {
        cases.push(Case {
            name,
            text: db.clone(),
            schema: wrap(&format!(
                r#"{{"db": {{"type": "object", "properties": {{"user": {{"type": "string"}}, "password": {password}}}}}}}"#
            )),
            entries: &["<password>x</password>", r#"<password a="1">x</password>"#],
            list: false,
        });
    }
    cases
}

fn mount(case: &Case) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(Language::XML, &case.text);
    editor.schema = Some(std::sync::Arc::new(build_schema(&case.schema).unwrap()));
    editor.secrets = Some(super::super::secret_options::SecretOptions::default());
    editor.on_save = Some(Callback::new(|_: SaveRequest| {}));
    editor.mount()
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// The ends of the lines a new element may follow: the list's (or
/// block's) opening line and each line inside it.
fn line_ends(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut ends = Vec::new();
    let mut at = 0;
    for (index, line) in lines.iter().enumerate() {
        at += line.len();
        // From the second line through the last inner one.
        if index >= 1 && index + 3 < lines.len() {
            ends.push(at);
        }
        at += 1;
    }
    ends
}

#[derive(Clone, Copy, Debug)]
enum How {
    Typed { pairs: bool },
    Pasted,
}

#[test]
fn a_new_element_is_taken_beside_xml_secrets_in_a_list_and_held_beside_a_lone_one() {
    let mut failures = Vec::new();
    for case in cases() {
        let schema: serde_json::Value = serde_json::from_str(&case.schema).unwrap();
        for entry in case.entries {
            for end in line_ends(&case.text) {
                for how in [
                    How::Typed { pairs: false },
                    How::Typed { pairs: true },
                    How::Pasted,
                ] {
                    let label = format!("{}: {entry} after byte {end} ({how:?})", case.name);
                    let (_owner, state) = mount(&case);
                    let indent = "    ";
                    let typed = format!("\n{indent}{entry}");
                    match how {
                        How::Typed { pairs } => type_at(state, end, &typed, pairs),
                        How::Pasted => {
                            write(state, end..end, &typed, false);
                        }
                    }
                    let expected = format!("{}{typed}{}", &case.text[..end], &case.text[end..]);
                    let now = shown(state);
                    // Beside a lone secret: refused, or taken and held.
                    if !case.list {
                        if now == expected {
                            state.recheck_tokens();
                            let found = problems(state, Language::XML, Some(&schema));
                            if found.is_empty() || state.save() || !untrack(|| state.changes_held())
                            {
                                failures.push(format!(
                                    "{label}: a lone secret made a list, not held (problems {found:?})"
                                ));
                            }
                        }
                        continue;
                    }
                    if now != expected {
                        failures.push(format!("{label}: not taken, left\n{now}"));
                        continue;
                    }
                    // After the typing pause, as the problems are read.
                    state.recheck_tokens();
                    let found = problems(state, Language::XML, Some(&schema));
                    let saved = state.save();
                    if !found.is_empty() || !saved {
                        failures.push(format!(
                            "{label}: a new item flagged {found:?} (saved: {saved})"
                        ));
                    }
                }
            }
        }
    }
    report(failures);
}

#[test]
fn text_glued_to_an_xml_secret_is_still_refused() {
    let mut failures = Vec::new();
    for case in cases() {
        for token in [FIRST, SECOND] {
            let text = case.text.clone();
            let Some(at) = text.find(token) else {
                continue;
            };
            for (side, place) in [("before", at), ("after", at + token.len())] {
                for how in [How::Typed { pairs: false }, How::Pasted] {
                    let (_owner, state) = mount(&case);
                    match how {
                        How::Typed { pairs } => type_at(state, place, "Z", pairs),
                        How::Pasted => {
                            write(state, place..place, "Z", false);
                        }
                    }
                    if shown(state) != text {
                        failures.push(format!("{}: Z {side} {token} ({how:?}) taken", case.name));
                    }
                }
            }
        }
    }
    report(failures);
}
