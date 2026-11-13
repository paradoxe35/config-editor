//! An XML element whose children, comments and processing instructions
//! share one line (a whole document on one line, or one element of a
//! document over lines), edited structurally from every other view: only
//! what the edit touches changes, and the comments, processing
//! instructions and CDATA sections around it stay byte for byte (hard-bar
//! QA `D24.editing-1`: deleting an element or attribute rewrote the host
//! from the data, dropping every comment and processing instruction and
//! escaping the CDATA kept, and the save was accepted).

use crate::editor::Language;
use crate::editor::save_matrix::{from_every_view, report};
use serde_json::Value;

const ONE_LINE: &str = "<r><!-- c --><?pi x?><a><![CDATA[<x>]]></a><b>t</b></r>\n";
const ATTRIBUTES: &str =
    "<r xmlns:x=\"urn:a\" a=\"1\" b=\"2\"><!-- c --><?pi x?><s x:attr=\"3\">t</s><u>v</u></r>\n";
const NESTED: &str =
    "<r>\n  <s n=\"1\"><!-- c --><p>1</p> <q>2</q></s> <!-- s -->\n  <z>0</z>\n</r>\n";

/// `value` without the key `key` of the map at `pointer`.
fn removed(value: &mut Value, pointer: &str, key: &str) {
    if let Some(map) = value.pointer_mut(pointer).and_then(Value::as_object_mut) {
        map.shift_remove(key);
    }
}

/// `value` with `key` set to `held` in the map at `pointer`.
fn added(value: &mut Value, pointer: &str, key: &str, held: Value) {
    if let Some(map) = value.pointer_mut(pointer).and_then(Value::as_object_mut) {
        map.insert(key.to_string(), held);
    }
}

#[test]
fn a_structural_edit_of_an_element_on_one_line_keeps_its_comments() {
    let mut failures = Vec::new();
    type Case = (&'static str, &'static str, fn(&mut Value), &'static str);
    let cases: [Case; 8] = [
        (
            "the element after the comments removed",
            ONE_LINE,
            |value| removed(value, "/r", "a"),
            "<r><!-- c --><?pi x?><b>t</b></r>\n",
        ),
        (
            "the element after the CDATA removed",
            ONE_LINE,
            |value| removed(value, "/r", "b"),
            "<r><!-- c --><?pi x?><a><![CDATA[<x>]]></a></r>\n",
        ),
        (
            "an element added",
            ONE_LINE,
            |value| added(value, "/r", "d", "four".into()),
            "<r><!-- c --><?pi x?><a><![CDATA[<x>]]></a><b>t</b><d>four</d></r>\n",
        ),
        (
            "an element removed beside attributes",
            ATTRIBUTES,
            |value| removed(value, "/r", "u"),
            "<r xmlns:x=\"urn:a\" a=\"1\" b=\"2\"><!-- c --><?pi x?><s x:attr=\"3\">t</s></r>\n",
        ),
        (
            "an attribute removed",
            ATTRIBUTES,
            |value| removed(value, "/r", "b"),
            "<r xmlns:x=\"urn:a\" a=\"1\"><!-- c --><?pi x?><s x:attr=\"3\">t</s><u>v</u></r>\n",
        ),
        (
            "an attribute added",
            ATTRIBUTES,
            |value| added(value, "/r", "x:c", "z".into()),
            "<r xmlns:x=\"urn:a\" a=\"1\" b=\"2\" x:c=\"z\"><!-- c --><?pi x?><s x:attr=\"3\">t</s><u>v</u></r>\n",
        ),
        (
            "a child of an element on one line removed",
            NESTED,
            |value| removed(value, "/r/s", "q"),
            "<r>\n  <s n=\"1\"><!-- c --><p>1</p> </s> <!-- s -->\n  <z>0</z>\n</r>\n",
        ),
        (
            "a child added to an element on one line",
            NESTED,
            |value| added(value, "/r/s", "w", "five".into()),
            "<r>\n  <s n=\"1\"><!-- c --><p>1</p> <q>2</q><w>five</w></s> <!-- s -->\n  <z>0</z>\n</r>\n",
        ),
    ];
    for (label, host, change, expected) in cases {
        from_every_view(&mut failures, label, Language::XML, host, &change, expected);
    }
    report(failures);
}
