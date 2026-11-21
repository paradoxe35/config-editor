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
/// saved as the editor saves it, told by the links the edits kept.
fn saved_with_copies(
    language: Language,
    host: &str,
    schema: Option<&Value>,
    count: usize,
    source: &Inserted,
) -> String {
    let xml = Language::XML.behavior();
    let bundle =
        schema.map(|schema| Arc::new(build_schema(&schema.to_string()).expect("a schema")));
    let bridge = Bridge::new(language, Language::XML, bundle).unwrap();
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
    let identity = language.behavior().items_hold_unshown(host);
    let told = links.told(xml, &next, host, identity);
    let untouched = links.untouched(xml, &next);
    bridge
        .to_host_linked(&next, host, host, told.as_ref(), untouched.as_deref())
        .unwrap_or_else(|error| panic!("{error:?}: {next}"))
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
                    let saved = saved_with_copies(language, host, schema, count, &source);
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
