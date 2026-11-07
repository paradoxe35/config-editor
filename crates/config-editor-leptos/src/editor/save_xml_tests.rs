//! XML hosts edited from every other view: text keeps its CDATA sections
//! and entity or character references around the change, and mixed content
//! (text beside child elements), which no other language shows, refuses
//! the switch instead of being shown and saved wrong.

use super::Language;
use super::bridge::Bridge;
use super::codec::ConvertError;
use super::save_matrix::{ALL, from_every_view, report};
use serde_json::Value;

const XML: &str = "<!-- c -->\n<root>\n  <cd>before <![CDATA[<raw>]]> after</cd>\n  <ent>a &amp; b &#233; &gt;</ent>\n  <hex>&#x41;&#66;C</hex>\n  <at v=\"a &amp; b &#233;\" w='x &lt; y'/>\n  <whole><![CDATA[if a < b]]></whole>\n  <n>1</n>\n</root>\n";

/// The string at `pointer` replaced through `edit`.
fn edit_text(pointer: &'static str, edit: fn(&str) -> String) -> impl Fn(&mut Value) {
    move |value| {
        let slot = value.pointer_mut(pointer).expect("the value");
        let text = slot.as_str().expect("a string").to_string();
        *slot = edit(&text).into();
    }
}

/// A label, the string's pointer, the edit, and the text it makes of the
/// host (before, after).
type Case = (
    &'static str,
    &'static str,
    fn(&str) -> String,
    (&'static str, &'static str),
);

#[test]
fn xml_text_keeps_cdata_and_references_around_an_edit() {
    let mut failures = Vec::new();
    let cases: [Case; 9] = [
        (
            "text after CDATA",
            "/root/cd",
            |text| format!("{text}Z"),
            (" after</cd>", " afterZ</cd>"),
        ),
        (
            "text before CDATA",
            "/root/cd",
            |text| format!("Z{text}"),
            ("<cd>before", "<cd>Zbefore"),
        ),
        (
            "inside CDATA",
            "/root/cd",
            |text| text.replace("raw", "rawZ"),
            ("<![CDATA[<raw>]]>", "<![CDATA[<rawZ>]]>"),
        ),
        (
            "beside references",
            "/root/ent",
            |text| text.replacen('a', "aZ", 1),
            ("<ent>a &amp;", "<ent>aZ &amp;"),
        ),
        (
            "a reference replaced",
            "/root/ent",
            |text| text.replace('é', "e"),
            ("&#233;", "e"),
        ),
        (
            "character references kept",
            "/root/hex",
            |text| format!("{text}D"),
            ("C</hex>", "CD</hex>"),
        ),
        (
            "an attribute's references",
            "/root/at/v",
            |text| format!("{text}!"),
            ("&#233;\"", "&#233;!\""),
        ),
        (
            "an attribute in single quotes",
            "/root/at/w",
            |text| text.replace('x', "xx"),
            ("'x &lt; y'", "'xx &lt; y'"),
        ),
        (
            "a whole CDATA section appended to",
            "/root/whole",
            |text| format!("{text} then"),
            ("<![CDATA[if a < b]]>", "<![CDATA[if a < b then]]>"),
        ),
    ];
    for (label, pointer, edit, (before, after)) in cases {
        assert!(XML.contains(before), "{label}: {before:?}");
        from_every_view(
            &mut failures,
            label,
            Language::XML,
            XML,
            &edit_text(pointer, edit),
            &XML.replacen(before, after, 1),
        );
    }
    // Another value edited: the text beside it stays byte for byte.
    from_every_view(
        &mut failures,
        "a sibling",
        Language::XML,
        XML,
        &|value| value["root"]["n"] = 2.into(),
        &XML.replace("<n>1</n>", "<n>2</n>"),
    );
    report(failures);
}

const MIXED: &str = "<root>\n  <mixed>text <b>bold</b> tail</mixed>\n  <n>1</n>\n</root>\n";

#[test]
fn xml_mixed_content_refuses_to_be_edited_as_another_language() {
    let mut failures = Vec::new();
    for edit in ALL.into_iter().filter(|edit| *edit != Language::XML) {
        let bridge = Bridge::new(Language::XML, edit, None).expect("a bridge");
        match bridge.to_edit(MIXED) {
            Err(ConvertError::MixedContent { pointer }) if pointer == "/root/mixed" => {}
            other => failures.push(format!("XML host as {edit}: {other:?}")),
        }
        match Language::XML.convert(MIXED, edit) {
            Err(ConvertError::MixedContent { .. }) => {}
            other => failures.push(format!("XML converted to {edit}: {other:?}")),
        }
        // Typed in an XML view of another host: not saved.
        let host = edit;
        let bridge = Bridge::new(host, Language::XML, None).expect("a bridge");
        let host_text = host
            .serialize(&serde_json::json!({"n": 1}))
            .expect("serializes");
        let saved = bridge.to_host("<mixed>a <b>c</b> d</mixed>\n<n>1</n>\n", &host_text);
        if !matches!(&saved, Err(error) if error.pointer() == "/mixed") {
            failures.push(format!("XML view of {host}: {saved:?}"));
        }
    }
    // Text beside attributes alone is not mixed content.
    let attributes = "<root>\n  <name lang=\"en\">Edge</name>\n</root>\n";
    if let Err(error) = Language::XML.convert(attributes, Language::YAML) {
        failures.push(format!("text beside attributes: {error}"));
    }
    report(failures);
}

/// The refusal is worded by the `strings` setting.
#[test]
fn the_mixed_content_refusal_is_worded_by_the_strings_setting() {
    use super::edit_as::SaveError;
    let strings = crate::EditorStrings {
        mixed_content: "mixed at {pointer}".to_string(),
        ..crate::EditorStrings::default()
    };
    let error = SaveError::Convert(ConvertError::MixedContent {
        pointer: "/root/mixed".to_string(),
    });
    assert_eq!(error.text(&strings, Language::XML), "mixed at /root/mixed");
}
