//! P2 and P3 in an XML view where a host scalar is written as several
//! elements (its element typed again, or pasted from a copy of it): the
//! host has no list there, so every item is new. Each saves in the
//! host's type (a copy keeps its source's form), except where the schema
//! takes the view's reading of it and refuses the host's type (a list of
//! strings over the host's `0`): the save then reads as the view does.

use std::sync::Arc;

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::bridge::links::{Clipboard, Inserted, Links, Piece};
use crate::editor::schema::build_schema;
use crate::editor::text::{TextRange, text_size};
use serde_json::{Value, json};

/// The host's text once the XML view of `host` (in `language`, under
/// `schema`) has its `d` element written `count` times, each one after
/// the first as `source` says (typed, or pasted from a copy of the first),
/// saved as the editor saves it, told by the links the edits kept; with
/// `through`, the unsaved edits switched to that view and back first, as
/// a switch carries them (see `sync::switch_to`).
fn saved_with_copies(
    language: Language,
    host: &str,
    schema: Option<&Value>,
    count: usize,
    source: &Inserted,
    through: Option<Language>,
) -> Result<String, String> {
    let xml = Language::XML.behavior();
    let bundle =
        schema.map(|schema| Arc::new(build_schema(&schema.to_string()).expect("a schema")));
    let bridge = Bridge::new(language, Language::XML, bundle.clone()).unwrap();
    let view = bridge.to_edit(host).unwrap().text;
    let start = view.find("<d>").expect("the element");
    let end = start + view[start..].find('\n').unwrap() + 1;
    let line_start = view[..start].rfind('\n').map_or(0, |at| at + 1);
    let written = view[line_start..end].to_string();
    let mut links = Links::fresh(host, &view, Some(bridge.reading()));
    let mut clipboard = Clipboard::default();
    if matches!(source, Inserted::Pasted) {
        let copied = links.copied(
            (xml, &view),
            TextRange::new(text_size(line_start), text_size(end)),
        );
        clipboard.copied(&written, copied);
    }
    let mut next = view.clone();
    for _ in 1..count {
        let piece = Piece {
            range: TextRange::new(text_size(end), text_size(end)),
            inserted: written.clone(),
            source: source.clone(),
        };
        let edited = format!("{}{written}{}", &next[..end], &next[end..]);
        links.edited(xml, &next, &[piece], &mut clipboard);
        next = edited;
    }
    if let Some(other) = through {
        let untouched = links.untouched(xml, &next);
        let shown = bridge
            .to_other_view(&next, other, untouched.as_deref())
            .map_err(|error| format!("{error:?}: {next}"))?
            .text;
        links = links.converted(xml, &next, other.behavior(), &shown);
        let untouched = links.untouched(other.behavior(), &shown);
        let back = Bridge::new(language, other, bundle)
            .unwrap()
            .to_other_view(&shown, Language::XML, untouched.as_deref())
            .map_err(|error| format!("{error:?}: {shown}"))?
            .text;
        links = links.converted(other.behavior(), &shown, xml, &back);
        next = back;
    }
    let identity = language.behavior().items_hold_unshown(host);
    let told = links.told(xml, &next, host, identity);
    let untouched = links.untouched(xml, &next);
    bridge
        .to_host_linked(&next, host, host, told.as_ref(), untouched.as_deref())
        .map_err(|error| format!("{error:?}: {next}"))
}

#[test]
fn a_host_scalar_written_as_several_elements_saves_as_the_view_reads_it() {
    let strings = json!({"properties": {"d": {"anyOf": [{"items": {"type": "string"}}]}}});
    let hosts = [
        (Language::JSON, "{\"d\": 0}\n"),
        (Language::YAML, "d: 0\n"),
        (Language::TOML, "d = 0\n"),
        (Language::HCL, "d = 0\n"),
    ];
    let mut failed = Vec::new();
    for (language, host) in hosts {
        for count in 1..=3 {
            for schema in [None, Some(&strings)] {
                for source in [Inserted::Typed, Inserted::Pasted] {
                    let saved =
                        saved_with_copies(language, host, schema, count, &source, None).unwrap();
                    let read = language.parse(&saved).expect(&saved);
                    let expected = match (count, schema) {
                        // Unchanged: nothing is saved.
                        (1, _) => json!({"d": 0}),
                        // The schema takes the view's strings and refuses
                        // the host's number.
                        (_, Some(_)) => json!({"d": vec!["0"; count]}),
                        // Without one, each item keeps the host's type.
                        (_, None) => json!({"d": vec![0; count]}),
                    };
                    if read != expected {
                        failed.push(format!(
                            "{language} {count} {} {source:?}: {read} (want {expected})",
                            if schema.is_some() {
                                "schema"
                            } else {
                                "no schema"
                            }
                        ));
                    }
                }
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// P3 with a schema: a host list grown by a copy of one of its items
/// (pasted in an XML view) holds the copy as new, the item copied as the
/// host holds it. The copy keeps its source's type where the schema
/// takes it, and saves as the view reads it where the schema takes only
/// that (the record tells the copy from the item in its place).
#[test]
fn a_copy_a_host_list_grows_by_saves_as_the_schema_reads_it() {
    // A number first, then a string: the copy lands where only text is
    // taken.
    let strings = json!({"properties": {"r": {"properties": {"d": {
        "prefixItems": [{"type": "integer"}, {"type": "string"}]
    }}}}});
    let hosts = [
        (Language::JSON, "{\"r\": {\"d\": [0, \"a\"]}}\n"),
        (Language::YAML, "r:\n  d:\n    - 0\n    - a\n"),
        (Language::TOML, "[r]\nd = [0, \"a\"]\n"),
        (Language::HCL, "r {\n  d = [0, \"a\"]\n}\n"),
    ];
    let mut failed = Vec::new();
    for (language, host) in hosts {
        for schema in [None, Some(&strings)] {
            let read = saved_with_copies(language, host, schema, 2, &Inserted::Pasted, None)
                .map(|saved| language.parse(&saved).expect(&saved));
            let Ok(read) = read else {
                failed.push(format!("{language}: {read:?}"));
                continue;
            };
            let expected = match schema {
                Some(_) => json!([0, "0", "a"]),
                None => json!([0, 0, "a"]),
            };
            let read = read.pointer("/r/d").cloned().unwrap_or_default();
            if read != expected {
                let schema = if schema.is_some() {
                    "schema"
                } else {
                    "no schema"
                };
                failed.push(format!("{language} {schema}: {read} (want {expected})"));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// P3 across a switch: a copy of a list item pasted in an XML view, the
/// edits switched unsaved to another view and back, is still a copy (its
/// mark carried by the scalars a switch writes as the editor wrote them,
/// in a view that writes the list on one line too): under a schema taking
/// only text there it saves as the view reads it, as it does with no
/// switch.
#[test]
fn a_copy_switched_to_another_view_and_back_saves_as_the_schema_reads_it() {
    let strings = json!({"properties": {"r": {"properties": {"d": {
        "prefixItems": [{"type": "integer"}, {"type": "string"}]
    }}}}});
    let hosts = [
        (Language::JSON, "{\"r\": {\"d\": [0, \"a\"]}}\n"),
        (Language::YAML, "r:\n  d:\n    - 0\n    - a\n"),
        (Language::TOML, "[r]\nd = [0, \"a\"]\n"),
        (Language::HCL, "r {\n  d = [0, \"a\"]\n}\n"),
    ];
    let views = [
        Language::JSON,
        Language::YAML,
        Language::TOML,
        Language::HCL,
    ];
    let mut failed = Vec::new();
    for (language, host) in hosts {
        // A switch to the host's language saves the edits: no view.
        for through in views.into_iter().filter(|view| *view != language) {
            for schema in [None, Some(&strings)] {
                let read =
                    saved_with_copies(language, host, schema, 2, &Inserted::Pasted, Some(through))
                        .map(|saved| language.parse(&saved).expect(&saved));
                let label = format!(
                    "{language} through {through}, {}",
                    if schema.is_some() {
                        "schema"
                    } else {
                        "no schema"
                    }
                );
                let Ok(read) = read else {
                    failed.push(format!("{label}: {read:?}"));
                    continue;
                };
                let expected = match schema {
                    Some(_) => json!([0, "0", "a"]),
                    None => json!([0, 0, "a"]),
                };
                let read = read.pointer("/r/d").cloned().unwrap_or_default();
                if read != expected {
                    failed.push(format!("{label}: {read} (want {expected})"));
                }
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
