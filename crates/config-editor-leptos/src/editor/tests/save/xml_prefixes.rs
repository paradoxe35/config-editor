//! XML namespace declarations edited from another view: a save that leaves
//! a prefix no declaration binds waits, at the name that needs it (hard-bar
//! QA `D25.editing-2`: removing `xmlns:x` saved `<s x:attr="3">` unbound,
//! which namespace-aware readers refuse, with no problem shown). So does a
//! save the host could not show in the view again (text beside a child
//! element added under it).

use crate::editor::Language;
use crate::editor::save_matrix::{ALL, report, save_value};
use serde_json::Value;

const HOST: &str = "<r xmlns:x=\"urn:a\" a=\"1\">\n  <s x:attr=\"3\">t</s>\n  <u>v</u>\n</r>\n";

/// `value` without the key `key` of the map at `pointer`.
fn removed(value: &mut Value, pointer: &str, key: &str) {
    if let Some(map) = value.pointer_mut(pointer).and_then(Value::as_object_mut) {
        map.shift_remove(key);
    }
}

fn views() -> impl Iterator<Item = Language> {
    ALL.into_iter().filter(|edit| *edit != Language::XML)
}

#[test]
fn removing_a_declaration_a_prefix_needs_waits_on_the_prefixed_name() {
    let mut failures = Vec::new();
    for edit in views() {
        let saved = save_value(Language::XML, edit, HOST, &|value| {
            removed(value, "/r", "xmlns:x");
        });
        match saved {
            Err(error)
                if error.contains("bound to no namespace")
                    && error.contains("(at /r/s/x:attr)") => {}
            other => failures.push(format!("via {edit}: {other:?}")),
        }
    }
    report(failures);
}

#[test]
fn removing_a_declaration_nothing_needs_is_saved() {
    let mut failures = Vec::new();
    let host = "<r xmlns:x=\"urn:a\" a=\"1\">\n  <s k=\"3\">t</s>\n  <u>v</u>\n</r>\n";
    let expected = "<r a=\"1\">\n  <s k=\"3\">t</s>\n  <u>v</u>\n</r>\n";
    for edit in views() {
        match save_value(Language::XML, edit, host, &|value| {
            removed(value, "/r", "xmlns:x");
        }) {
            Ok(saved) if saved == expected => {}
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

/// A namespace declaration added in a view is a declaration, not an
/// element named `xmlns:y` (which reads back as `y`).
#[test]
fn a_namespace_declaration_added_in_a_view_is_an_attribute() {
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
            Ok(saved) if saved == "<r a=\"1\" xmlns:y=\"urn:b\">\n  <s>t</s>\n</r>\n" => {}
            other => failures.push(format!("via {edit}: {other:?}")),
        }
    }
    report(failures);
}
