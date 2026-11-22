//! P2 across the ways a schema gives a list's items: a host scalar written
//! as several items of a view (XML elements, typed again or pasted, or an
//! HCL list typed over it) saves, and reads back as the view reads it,
//! where the items are given through `anyOf`, `oneOf`, `allOf`, `$ref` or
//! `prefixItems`, for items of text, numbers and booleans. A host scalar
//! the view now holds as an item is judged by the item's schema: where it
//! takes the view's reading and refuses the host's type (the host's `0`
//! where items are text), the view's reading is the one saved.

use super::{Inserted, saved_with_copies};
use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::codec;
use crate::editor::schema::build_schema;
use serde_json::{Value, json};
use std::sync::Arc;

/// `d`'s items as `item` through each way of giving them, by name.
fn schemas(item: &Value) -> Vec<(&'static str, Value)> {
    let items = json!({"items": item});
    vec![
        ("anyOf", json!({"properties": {"d": {"anyOf": [items]}}})),
        ("oneOf", json!({"properties": {"d": {"oneOf": [items]}}})),
        ("allOf", json!({"properties": {"d": {"allOf": [items]}}})),
        (
            "$ref",
            json!({"$defs": {"l": items}, "properties": {"d": {"$ref": "#/$defs/l"}}}),
        ),
        (
            "prefixItems",
            json!({"properties": {"d": {"prefixItems": [item, item, item]}}}),
        ),
    ]
}

/// Each item type, the host's value of another type whose text reads as
/// one (`0` for text items), and that reading.
fn kinds() -> [(&'static str, Value, Value); 3] {
    [
        ("string", json!(0), json!("0")),
        ("number", json!("0"), json!(0)),
        ("boolean", json!("true"), json!(true)),
    ]
}

/// The host `{d: value}` in `language`.
fn host_text(language: Language, value: &Value) -> String {
    // A JSON scalar is one in each host's syntax (a string quoted).
    if language == Language::JSON {
        format!("{{\"d\": {value}}}\n")
    } else if language == Language::YAML {
        format!("d: {value}\n")
    } else {
        format!("d = {value}\n")
    }
}

fn valid(schema: &Value, read: &Value) -> bool {
    jsonschema::validator_for(schema).is_ok_and(|validator| validator.is_valid(read))
}

#[test]
fn a_host_scalar_written_as_several_items_saves_as_each_way_of_giving_items_reads_it() {
    let hosts = [
        Language::JSON,
        Language::YAML,
        Language::TOML,
        Language::HCL,
    ];
    let mut failed = Vec::new();
    let mut tried = 0;
    for (kind, host_value, read_as) in kinds() {
        for (way, schema) in schemas(&json!({"type": kind})) {
            for language in hosts {
                let host = host_text(language, &host_value);
                for count in 1..=3 {
                    for source in [Inserted::Typed, Inserted::Pasted] {
                        for through in [None, Some(Language::HCL)] {
                            if through == Some(language) {
                                continue;
                            }
                            tried += 1;
                            let case = format!(
                                "{kind} items through {way}, {language} host, {count} {source:?}{}",
                                through.map_or(String::new(), |view| format!(" via {view}"))
                            );
                            let saved = match saved_with_copies(
                                language,
                                &host,
                                Some(&schema),
                                count,
                                &source,
                                through,
                            ) {
                                Ok(saved) => saved,
                                Err(error) => {
                                    failed.push(format!("{case}: waits: {error}"));
                                    continue;
                                }
                            };
                            let read = codec::read(language.behavior(), &saved, Some(&schema))
                                .expect(&saved);
                            // One item: the view is unchanged, nothing saved.
                            let expected = if count == 1 {
                                json!({ "d": host_value })
                            } else {
                                json!({ "d": vec![read_as.clone(); count] })
                            };
                            if read != expected || (count > 1 && !valid(&schema, &read)) {
                                failed.push(format!("{case}: {read} (want {expected}):\n{saved}"));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(tried > 500, "only {tried} cases tried");
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// The same host scalar made a list in an HCL view, its items typed in
/// the schema's type: it saves as the view reads it, in every host.
#[test]
fn a_host_scalar_typed_over_as_an_hcl_list_saves_as_each_way_of_giving_items_reads_it() {
    let hosts = [Language::JSON, Language::YAML, Language::TOML];
    let mut failed = Vec::new();
    for (kind, host_value, read_as) in kinds() {
        let written = read_as.to_string();
        for (way, schema) in schemas(&json!({"type": kind})) {
            let bundle = Arc::new(build_schema(&schema.to_string()).expect("a schema"));
            for language in hosts {
                let host = host_text(language, &host_value);
                let bridge = Bridge::new(language, Language::HCL, Some(bundle.clone())).unwrap();
                let view = bridge.to_edit(&host).unwrap().text;
                for count in 1..=3 {
                    let case = format!("{kind} items through {way}, {language} host, {count}");
                    let list = vec![written.as_str(); count].join(", ");
                    let edited = format!("d = [{list}]\n");
                    let expected = json!({ "d": vec![read_as.clone(); count] });
                    let shown = codec::read(Language::HCL.behavior(), &edited, Some(&schema));
                    if shown.as_ref().ok() != Some(&expected) {
                        failed.push(format!("{case}: the view reads {shown:?} in\n{view}"));
                        continue;
                    }
                    let saved = match bridge.to_host(&edited, &host) {
                        Ok(saved) => saved,
                        Err(error) => {
                            failed.push(format!("{case}: waits: {error:?}"));
                            continue;
                        }
                    };
                    let read =
                        codec::read(language.behavior(), &saved, Some(&schema)).expect(&saved);
                    if read != expected || !valid(&schema, &read) {
                        failed.push(format!("{case}: {read} (want {expected}):\n{saved}"));
                    }
                }
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
