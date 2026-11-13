//! Prefixes edited from another view: a save that leaves a prefix no
//! declaration binds waits, at the name that needs it (hard-bar QA
//! `D25.editing-2`: removing `xmlns:x` saved `<s x:attr="3">` unbound,
//! which namespace-aware readers refuse, with no problem shown). So does a
//! save the host could not show in the view again (text beside a child
//! element added under it). The declarations themselves are markup no view
//! shows (`xml_declarations`).

use crate::editor::Language;
use crate::editor::save_matrix::{report, save_value};
use serde_json::Value;

const HOST: &str = "<r xmlns:x=\"urn:a\" a=\"1\">\n  <s x:attr=\"3\">t</s>\n  <u>v</u>\n</r>\n";

/// `value` without the key `key` of the map at `pointer`.
fn removed(value: &mut Value, pointer: &str, key: &str) {
    if let Some(map) = value.pointer_mut(pointer).and_then(Value::as_object_mut) {
        map.shift_remove(key);
    }
}

/// A view shows no namespace declaration, so it cannot remove one: an
/// attribute a view gives a prefix that no declaration binds waits, at
/// the name that needs one (hard-bar QA `D25.editing-2`).
#[test]
fn a_prefix_no_declaration_binds_waits_on_the_prefixed_name() {
    let mut failures = Vec::new();
    for edit in [Language::YAML, Language::JSON, Language::TOML] {
        let saved = save_value(Language::XML, edit, HOST, &|value| {
            removed(value, "/r", "a");
            if let Some(map) = value.pointer_mut("/r").and_then(Value::as_object_mut) {
                map.insert("y:a".into(), 1.into());
            }
        });
        match saved {
            Err(error)
                if error.contains("bound to no namespace") && error.contains("(at /r/y:a)") => {}
            other => failures.push(format!("via {edit}: {other:?}")),
        }
    }
    report(failures);
}

/// A child element added beside an element's text: XML would write text
/// beside a child, which no view shows (`/r/s` could not be switched
/// back to): the save waits.
#[test]
fn a_child_added_beside_an_elements_text_waits() {
    let mut failures = Vec::new();
    for edit in [Language::YAML, Language::JSON, Language::TOML] {
        let saved = save_value(Language::XML, edit, HOST, &|value| {
            if let Some(map) = value.pointer_mut("/r/s").and_then(Value::as_object_mut) {
                map.insert("d".into(), 4.into());
            }
        });
        match saved {
            Err(error) if error.contains("MixedContent") && error.contains("/r/s") => {}
            other => failures.push(format!("via {edit}: {other:?}")),
        }
    }
    report(failures);
}

/// A namespace declaration is markup, not data: one a view adds as a key
/// waits, saying to write it in the XML text.
#[test]
fn a_namespace_declaration_added_in_a_view_waits() {
    let mut failures = Vec::new();
    let host = "<r a=\"1\">\n  <s>t</s>\n</r>\n";
    // HCL names no attribute `xmlns:y`.
    for edit in [Language::YAML, Language::JSON, Language::TOML] {
        let saved = save_value(Language::XML, edit, host, &|value| {
            if let Some(map) = value.pointer_mut("/r").and_then(Value::as_object_mut) {
                map.insert("xmlns:y".into(), "urn:b".into());
            }
        });
        match saved {
            Err(error)
                if error.contains("xmlns:y is XML markup") && error.contains("(at /r/xmlns:y)") => {
            }
            other => failures.push(format!("via {edit}: {other:?}")),
        }
    }
    report(failures);
}
