//! P2 (untouched text survives) for a value written once that the schema
//! reads as a list of one (an HCL block, an XML element): a value edited
//! inside it is patched where the text writes it, the rest kept as
//! written.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::schema::build_schema;

const SERVICES: &str = r#"{"properties":{"svc":{"type":"array","items":{"type":"object",
    "properties":{"ports":{"type":"array","items":{"type":"integer"}},"name":{"type":"string"}}}}}}"#;

#[test]
fn a_list_item_inside_a_block_written_once_is_patched_in_place() {
    let schema = std::sync::Arc::new(build_schema(SERVICES).unwrap());
    let host = "# services\nsvc {\n  ports = [8, 9] # open\n  name  = \"a\"\n}\n";
    let bridge = Bridge::new(Language::HCL, Language::YAML, Some(schema)).unwrap();
    let shown = bridge.to_edit(host).unwrap().text;
    let edited = shown.replacen("- 8", "- 7", 1);
    assert_ne!(edited, shown, "the view writes the item: {shown}");
    let saved = bridge.to_host(&edited, host).unwrap();
    assert_eq!(
        saved,
        "# services\nsvc {\n  ports = [7, 9] # open\n  name  = \"a\"\n}\n"
    );
}

#[test]
fn a_value_beside_a_first_item_inside_an_element_written_once_is_patched_in_place() {
    let schema = std::sync::Arc::new(build_schema(SERVICES).unwrap());
    let host = "<config>\n  <!-- services -->\n  <svc>\n    <ports>8</ports>\n    <ports>9</ports>\n    <name>a</name>\n  </svc>\n</config>\n";
    let bridge = Bridge::new(Language::XML, Language::YAML, Some(schema)).unwrap();
    let shown = bridge.to_edit(host).unwrap().text;
    let edited = shown.replacen("- 8", "- 7", 1);
    assert_ne!(edited, shown, "the view writes the item: {shown}");
    let saved = bridge.to_host(&edited, host).unwrap();
    assert_eq!(saved, host.replace("<ports>8<", "<ports>7<"));
}

/// P1 (no silent data change): a key renamed in an XML view keeps a list
/// of one the HCL host writes as a list; a block the schema reads as a
/// list of one stays a block under a key the schema does not list.
#[test]
fn a_renamed_key_keeps_a_list_the_host_writes_as_one() {
    let rename = |view: &str| view.replace("<a>", "<c>").replace("</a>", "</c>");
    let bridge = Bridge::new(Language::HCL, Language::XML, None).unwrap();
    for (host, saved) in [
        ("a = [\"x\"]\nb = 1\n", "c = [\"x\"]\nb = 1\n"),
        ("a = [{ x = 1 }]\nb = 1\n", "c = [{ x = 1 }]\nb = 1\n"),
    ] {
        let shown = bridge.to_edit(host).unwrap().text;
        assert_eq!(bridge.to_host(&rename(&shown), host).unwrap(), saved);
    }
    let schema = std::sync::Arc::new(
        build_schema(r#"{"properties":{"a":{"type":"array","items":{"type":"object"}}}}"#).unwrap(),
    );
    let bridge = Bridge::new(Language::HCL, Language::XML, Some(schema)).unwrap();
    let host = "a {\n  x = 1\n}\nb = 1\n";
    let shown = bridge.to_edit(host).unwrap().text;
    assert_eq!(
        bridge.to_host(&rename(&shown), host).unwrap(),
        "c {\n  x = 1\n}\nb = 1\n"
    );
}
