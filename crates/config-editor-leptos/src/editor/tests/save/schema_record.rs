//! P2 in an XML view read by a schema, as the editor saves it from its
//! record of the view's edits: a scalar the user typed saves as typed (in
//! the host's type, or as the view reads it where the schema takes only
//! that), even where the schema reads the text typed and the text shown
//! alike; and the schema's verdict that picks the reading is the one on
//! the typed value, never one an unrelated part of the document fails.

use std::sync::Arc;

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::bridge::links::{Clipboard, Inserted, Links, Piece};
use crate::editor::schema::build_schema;
use crate::editor::text::{TextRange, text_size};
use serde_json::{Value, json};

/// The host's text once each `(shown, typed)` of `edits` is typed in
/// turn over the first `shown` in the XML view of `host` (in `language`)
/// under `schema`, saved as the editor saves it, told by the links the
/// edits kept.
fn saved_after_typing(
    language: Language,
    host: &str,
    schema: &Value,
    edits: &[(&str, &str)],
) -> String {
    let xml = Language::XML.behavior();
    let bundle = Arc::new(build_schema(&schema.to_string()).expect("a schema"));
    let bridge = Bridge::new(language, Language::XML, Some(bundle)).unwrap();
    let view = bridge.to_edit(host).unwrap().text;
    let mut links = Links::fresh(host, &view, Some(bridge.reading()));
    let mut next = view.clone();
    for (shown, typed) in edits {
        let at = next
            .find(shown)
            .unwrap_or_else(|| panic!("{shown} in {next}"));
        let piece = Piece {
            range: TextRange::new(text_size(at), text_size(at + shown.len())),
            inserted: typed.to_string(),
            source: Inserted::Typed,
        };
        let edited = format!("{}{typed}{}", &next[..at], &next[at + shown.len()..]);
        links.edited(xml, &next, &[piece], &mut Clipboard::default());
        next = edited;
    }
    let identity = language.behavior().items_hold_unshown(host);
    let told = links.told(xml, &next, host, identity);
    let untouched = links.untouched(
        xml,
        &next,
        bridge.schema.as_deref().map(|bundle| &bundle.schema),
    );
    bridge
        .to_host_linked(&next, host, host, told.as_ref(), untouched.as_deref())
        .unwrap_or_else(|error| panic!("{error:?}: {next}"))
}

fn read(language: Language, text: &str) -> Value {
    language.parse(text).expect(text)
}

/// The schema reads the host's `"08"` and the typed `8` alike (a list of
/// numbers, refusing both): the text typed is the user's, saved in the
/// host's type, not lost because the reading did not change.
#[test]
fn a_string_typed_in_another_spelling_the_schema_reads_alike_saves_as_typed() {
    let schema = json!({"properties": {"size": {"type": "array", "items": {"enum": [0]}}}});
    for (language, host) in [
        (Language::JSON, "{\"size\": \"08\"}\n"),
        (Language::HCL, "size = \"08\"\n"),
        (Language::YAML, "size: \"08\"\n"),
        (Language::TOML, "size = \"08\"\n"),
    ] {
        let saved = saved_after_typing(language, host, &schema, &[(">08<", ">8<")]);
        assert_eq!(
            read(language, &saved),
            json!({"size": "8"}),
            "{language}: {saved}"
        );
    }
}

/// The schema takes the view's number `0` for `d` and refuses the host's
/// string: the reading is saved, though the document fails elsewhere
/// whichever is saved (`mode` makes `port` a number).
#[test]
fn a_reading_the_schema_takes_is_saved_where_the_document_fails_elsewhere() {
    let schema = json!({
        "dependentSchemas": {"mode": {"properties": {"port": {"type": "number"}}}},
        "properties": {"port": {"oneOf": [
            {"properties": {"d": {"type": "integer"}}},
            {"required": ["n"], "properties": {"d": {}}},
        ]}},
    });
    for (language, host) in [
        (Language::JSON, "{\"mode\": 0, \"port\": {\"d\": \"a\"}}\n"),
        (Language::HCL, "mode = 0\n\nport {\n  d = \"a\"\n}\n"),
    ] {
        let saved = saved_after_typing(language, host, &schema, &[(">a<", ">0<")]);
        assert_eq!(
            read(language, &saved),
            json!({"mode": 0, "port": {"d": 0}}),
            "{language}: {saved}"
        );
    }
}

/// A value left as written under a key renamed onto one the schema types
/// (`name: "true"` made `size`, a boolean) is read there as the view reads
/// it: the boolean, which the schema takes and its host string it refuses.
#[test]
fn a_value_under_a_key_renamed_onto_a_typed_one_saves_as_the_view_reads_it() {
    let schema = json!({"properties": {"size": {"type": "boolean"}}});
    for (language, host) in [
        (Language::JSON, "{\"name\": \"true\"}\n"),
        (Language::YAML, "name: \"true\"\n"),
        (Language::TOML, "name = \"true\"\n"),
        (Language::HCL, "name = \"true\"\n"),
    ] {
        let saved = saved_after_typing(
            language,
            host,
            &schema,
            &[("<name>", "<size>"), ("</name>", "</size>")],
        );
        assert_eq!(
            read(language, &saved),
            json!({"size": true}),
            "{language}: {saved}"
        );
    }
}
