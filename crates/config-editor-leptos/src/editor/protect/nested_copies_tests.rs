//! P1: in XML, where an element written twice reads as a list, a secret's
//! placeholder stays the secret's alone however its element and the
//! elements holding it are repeated around it: another element of the
//! field's name beside it, its parent written again (`<db>2</db>`), or
//! both, so the field reads as a list inside a list. Text typed at either
//! edge of the placeholder never reads beside it, in the host's own XML and
//! in an XML view of a JSON host.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use super::super::Language;
use super::super::secret_options::SecretOptions;
use super::super::test_state::TestEditor;
use super::glued_tests::{paste, type_at};
use super::many_copies_tests::shown;

const MASK: &str = "«secret 0b1c»";

/// The secret as an element and as an attribute, and the line after which
/// another element of the field's name goes.
fn forms() -> [(String, &'static str, &'static str); 2] {
    [
        (
            format!(
                "<config>\n  <db>\n    <user>app</user>\n    <password>{MASK}</password>\n  </db>\n  <port>1</port>\n</config>\n"
            ),
            "</password>",
            "\n    <password>}</password>",
        ),
        (
            format!(
                "<config>\n  <db user=\"app\" password=\"{MASK}\"/>\n  <port>1</port>\n</config>\n"
            ),
            "/>",
            "\n  <db password=\"}\"/>",
        ),
    ]
}

#[test]
fn text_typed_at_a_secret_never_reads_beside_it_however_its_elements_repeat() {
    let mut failures = Vec::new();
    for (xml, field_end, sibling) in forms() {
        for host in [Language::XML, Language::JSON] {
            for (field_copy, parent_copy) in [(true, false), (false, true), (true, true)] {
                for typed in ["\"", "x", "}"] {
                    for at_end in [false, true] {
                        let text = if host == Language::XML {
                            xml.clone()
                        } else {
                            let value = Language::XML.parse(&xml).expect("XML parses");
                            host.serialize(&value).expect("written")
                        };
                        let mut editor = TestEditor::new(host, &text);
                        if host != Language::XML {
                            editor.edit_as = Some(Language::XML);
                        }
                        editor.secrets = Some(SecretOptions::default());
                        let (_owner, state) = editor.mount();
                        let case = format!(
                            "{host} {field_end} field copy {field_copy} parent copy {parent_copy} {typed:?} at end {at_end}"
                        );
                        if field_copy {
                            // An XML view of the JSON host writes the
                            // attribute as an element.
                            let (field_end, sibling) = if shown(state).contains("</password>") {
                                ("</password>", "\n    <password>}</password>")
                            } else {
                                (field_end, sibling)
                            };
                            let at = shown(state).find(field_end).expect("field") + field_end.len();
                            if !paste(state, at..at, sibling) {
                                failures.push(format!("{case}: the sibling refused"));
                                continue;
                            }
                        }
                        if parent_copy {
                            let at = shown(state).find("<port>1</port>").expect("port")
                                + "<port>1</port>".len();
                            if !paste(state, at..at, "\n  <db>2</db>") {
                                failures.push(format!("{case}: the parent's copy refused"));
                                continue;
                            }
                        }
                        let before = shown(state);
                        let token = before.find(MASK).expect("the placeholder");
                        let at = if at_end { token + MASK.len() } else { token };
                        type_at(state, at, typed);
                        // Taken only where the text no longer parses (a
                        // quote closing an attribute's value early): the
                        // value is judged once it reads again.
                        let after = shown(state);
                        if after != before
                            && Language::XML.parse(&after).is_ok_and(|value| glued(&value))
                        {
                            failures.push(format!("{case}: taken: {after:?}"));
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Whether a string of `value` holds the placeholder with other text.
fn glued(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => text.contains(MASK) && text.trim() != MASK,
        serde_json::Value::Array(items) => items.iter().any(glued),
        serde_json::Value::Object(map) => map.values().any(glued),
        _ => false,
    }
}
