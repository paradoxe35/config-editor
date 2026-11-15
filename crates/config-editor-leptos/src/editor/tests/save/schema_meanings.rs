//! P2 (PutGet) under a schema: a save writes what the view means. A value
//! the view reads in another type where the schema says so (a key renamed
//! onto a boolean) is written in the host's default form for that meaning,
//! not kept in the form it had under its old key.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::diagnostics::build_schema;

#[test]
fn a_value_renamed_onto_a_typed_key_is_written_as_the_view_reads_it() {
    let schema =
        std::sync::Arc::new(build_schema(r#"{"properties":{"size":{"type":"boolean"}}}"#).unwrap());
    let host = "name: \"true\"\n";
    let bridge = Bridge::new(Language::YAML, Language::XML, Some(schema.clone())).unwrap();
    let shown = bridge.to_edit(host).unwrap().text;
    let edited = shown.replace("name", "size");
    assert_ne!(edited, shown, "the view writes the key: {shown}");
    // Waiting (with a reason) keeps the promise too; a save is read back.
    if let Ok(saved) = bridge.to_host(&edited, host) {
        let read = Language::YAML.parse(&saved).unwrap();
        assert_eq!(read, serde_json::json!({"size": true}), "saved {saved:?}");
    }
}
