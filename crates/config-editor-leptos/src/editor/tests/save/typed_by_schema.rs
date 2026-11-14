//! Saves under a schema that types every value, where a language that
//! writes scalars as text (XML) reads each by the schema: numbers and
//! booleans typed in a spelling of their own (`2.50`, `007`, `+6`) save as
//! the schema's type from every view to every host, the host keeping the
//! spelling where it reads it the same; a string spelled like a number,
//! boolean or `null` stays a string; and the text around an XML value is
//! kept.

use std::sync::Arc;

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::codec;
use crate::editor::diagnostics::{SchemaBundle, build_schema};
use crate::editor::save_matrix::{ALL, report};
use serde_json::{Value, json};

/// Types `f` (number), `i` (integer), `b` (boolean) and `n`, `s` (strings)
/// at the root and under `c` (an XML host's root element).
fn schema() -> Arc<SchemaBundle> {
    let schema = json!({
        "properties": {
            "f": {"type": "number"},
            "i": {"type": "integer"},
            "b": {"type": "boolean"},
            "n": {"type": "string"},
            "s": {"type": "string"},
            "c": {"$ref": "#"},
        },
    });
    Arc::new(build_schema(&schema.to_string()).expect("a schema"))
}

fn hosts() -> [(Language, &'static str); 5] {
    [
        (Language::YAML, "f: 1.50 # f\ni: 5\nb: true\nn: x\n"),
        (
            Language::JSON,
            "{\n  \"f\": 1.50,\n  \"i\": 5,\n  \"b\": true,\n  \"n\": \"x\"\n}\n",
        ),
        (Language::TOML, "f = 1.50 # f\ni = 5\nb = true\nn = \"x\"\n"),
        (Language::HCL, "f = 1.50 # f\ni = 5\nb = true\nn = \"x\"\n"),
        (
            Language::XML,
            "<c>\n  <f>1.50</f> <!-- f -->\n  <i>5</i>\n  <b>true</b>\n  <n>x</n>\n</c>\n",
        ),
    ]
}

fn in_host(host: Language, key: &str) -> String {
    if host == Language::XML {
        format!("/c/{key}")
    } else {
        format!("/{key}")
    }
}

/// `text` with the value at `pointer` replaced by `typed`.
fn typed_over(language: Language, text: &str, pointer: &str, typed: &str) -> Option<String> {
    let span = language.behavior().value_span(text, pointer)?;
    let range = codec::byte_range(text, span)?;
    let mut typed_text = text.to_string();
    typed_text.replace_range(range, typed);
    Some(typed_text)
}

/// XML read by a schema reads every decimal form as the number (an XML
/// host's `1.50` is the number 1.5, E21-3); every view's spelling of a
/// number or boolean it reads as one saves as the schema's type, and the
/// host keeps the spelling where it reads it the same.
#[test]
fn numbers_and_booleans_in_any_spelling_save_as_the_schema_types_them() {
    let schema = schema();
    let cases: [(&str, &str, Value); 10] = [
        ("f", "2.50", json!(2.5)),
        ("f", "0.10", json!(0.1)),
        ("f", "2e3", json!(2000.0)),
        ("f", "2.0e3", json!(2000.0)),
        ("i", "007", json!(7)),
        ("i", "+6", json!(6)),
        ("i", "6", json!(6)),
        ("b", "false", json!(false)),
        ("f", "3", json!(3)),
        ("f", "-0.5", json!(-0.5)),
    ];
    let mut failures = Vec::new();
    let mut tried = 0;
    for (host, host_text) in hosts() {
        let host_value = codec::read(host.behavior(), host_text, Some(&schema.schema)).unwrap();
        if host == Language::XML {
            assert_eq!(
                host_value["c"]["f"],
                json!(1.5),
                "XML reads 1.50 as the number"
            );
        }
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            let bridge = Bridge::new(host, edit, Some(schema.clone())).unwrap();
            let shown = bridge.to_edit(host_text).unwrap().text;
            for (key, typed, expected) in &cases {
                let pointer = in_host(host, key);
                let Some(edited) = typed_over(edit, &shown, &pointer, typed) else {
                    failures.push(format!("{host} via {edit}: no {pointer} in\n{shown}"));
                    continue;
                };
                // A spelling the view itself does not read as a number or
                // boolean (YAML's `007`) is not typed there.
                let read = codec::read(edit.behavior(), &edited, Some(&schema.schema));
                if !read.is_ok_and(|read| {
                    read.pointer(&pointer)
                        .is_some_and(|value| value.is_number() || value.is_boolean())
                }) {
                    continue;
                }
                tried += 1;
                let case = format!("{host} host via {edit}: {key} typed {typed}");
                let saved = match bridge.to_host(&edited, host_text) {
                    Ok(saved) => saved,
                    Err(error) => {
                        failures.push(format!("{case}: waits: {error:?}"));
                        continue;
                    }
                };
                let value = codec::read(host.behavior(), &saved, Some(&schema.schema))
                    .ok()
                    .and_then(|value| value.pointer(&pointer).cloned());
                let same = match (&value, expected) {
                    (Some(Value::Number(saved)), Value::Number(expected)) => {
                        saved.as_f64() == expected.as_f64()
                    }
                    (Some(saved), expected) => saved == expected,
                    _ => false,
                };
                if !same {
                    failures.push(format!("{case}: saved {value:?}, not {expected}:\n{saved}"));
                    continue;
                }
                // An XML host writes the number as typed: it is text there.
                let spelled = host
                    .behavior()
                    .value_span(&saved, &pointer)
                    .and_then(|span| codec::byte_range(&saved, span))
                    .map(|range| saved[range].trim().to_string());
                // (HCL has one number type: a whole number typed over a
                // float stays a float, `3.0`.)
                let hcl_float = edit == Language::HCL && !typed.contains(['.', 'e']);
                if host == Language::XML && !hcl_float && spelled.as_deref() != Some(*typed) {
                    failures.push(format!("{case}: written {spelled:?}:\n{saved}"));
                }
                // Nothing else changes.
                let comment = if host == Language::XML {
                    "<!-- f -->"
                } else {
                    "# f"
                };
                if host != Language::JSON && !saved.contains(comment) {
                    failures.push(format!("{case}: lost the comment:\n{saved}"));
                }
            }
        }
    }
    assert!(tried > 100, "only {tried} cases tried");
    report(failures);
}

/// Where the schema wants a string, XML text spelled like another value
/// (`true`, `null`, `1.50`, `""`, spaces around it) is that string: edited
/// in any view and saved into an XML host, or into another host from an
/// XML view, it stays the string, and the save does not wait (M23-E1).
#[test]
fn strings_spelled_like_other_values_stay_strings_from_and_to_xml() {
    let schema = schema();
    let mut failures = Vec::new();
    for text in ["true", "null", "1.50", "007", " 80 ", ""] {
        for host in ALL {
            let host_text = if host == Language::XML {
                "<c>\n  <s>old</s>\n  <i>5</i>\n</c>\n"
            } else if host == Language::JSON {
                "{\n  \"s\": \"old\",\n  \"i\": 5\n}\n"
            } else if host == Language::YAML {
                "s: old\ni: 5\n"
            } else {
                "s = \"old\"\ni = 5\n"
            }
            .to_string();
            let pointer = in_host(host, "s");
            for edit in ALL.into_iter().filter(|edit| *edit != host) {
                if host != Language::XML && edit != Language::XML {
                    continue;
                }
                let case = format!("{text:?}: {host} host via {edit}");
                let Some(bridge) = Bridge::new(host, edit, Some(schema.clone())) else {
                    continue;
                };
                let shown = bridge.to_edit(&host_text).unwrap().text;
                let mut value = codec::read(edit.behavior(), &shown, Some(&schema.schema)).unwrap();
                *value.pointer_mut(&pointer).unwrap() = json!(text);
                let Ok(edited) = codec::serialize(edit.behavior(), &value, Some(&shown)) else {
                    failures.push(format!("{case}: the view cannot write it"));
                    continue;
                };
                match bridge.to_host(&edited, &host_text) {
                    Ok(saved) => {
                        let read = codec::read(host.behavior(), &saved, Some(&schema.schema))
                            .ok()
                            .and_then(|read| read.pointer(&pointer).cloned());
                        // XML keeps no whitespace-only or empty text apart
                        // from an empty element, read as the empty string.
                        let expected = if host == Language::XML && text.trim().is_empty() {
                            json!("")
                        } else {
                            json!(text)
                        };
                        if read.as_ref() != Some(&expected) {
                            failures.push(format!("{case}: reads {read:?}:\n{saved}"));
                        }
                    }
                    Err(error) => failures.push(format!("{case}: waits: {error:?}")),
                }
            }
        }
    }
    report(failures);
}

/// An XML value with spaces around it (`<port> 80 </port>`) reads as the
/// number where the schema wants one and as the exact text where it wants
/// a string; a save from another view that leaves it alone keeps it byte
/// for byte, and one that changes the value next to it too.
#[test]
fn xml_text_with_spaces_reads_by_the_schema_and_saves_as_written() {
    let schema = schema();
    let host_text = "<c>\n  <i> 80 </i>\n  <s> padded </s>\n  <n>x</n>\n</c>\n";
    let value = codec::read(Language::XML.behavior(), host_text, Some(&schema.schema)).unwrap();
    assert_eq!(value, json!({"c": {"i": 80, "s": " padded ", "n": "x"}}));
    let mut failures = Vec::new();
    for edit in ALL.into_iter().filter(|edit| *edit != Language::XML) {
        let bridge = Bridge::new(Language::XML, edit, Some(schema.clone())).unwrap();
        let shown = bridge.to_edit(host_text).unwrap().text;
        let mut value = codec::read(edit.behavior(), &shown, Some(&schema.schema)).unwrap();
        value["c"]["n"] = json!("y");
        let edited = codec::serialize(edit.behavior(), &value, Some(&shown)).unwrap();
        match bridge.to_host(&edited, host_text) {
            Ok(saved) if saved == host_text.replace("<n>x</n>", "<n>y</n>") => {}
            Ok(saved) => failures.push(format!("via {edit}:\n{saved}")),
            Err(error) => failures.push(format!("via {edit}: waits: {error:?}")),
        }
    }
    report(failures);
}
