//! Suggestions at places only XML's markup has (start and end tags,
//! attributes, tags typed in part), against the tunnel schema. The
//! suggestion promises every language shares are in `completion_matrix`.
//!
//! `|` marks the caret.

use super::super::Language;
use super::super::harness::completion::{accepted, offered, parsed, keys_at};
use super::super::harness::report;

/// An XML element name typed in front of, or inside, an existing tag never
/// breaks that element: it becomes its own element or renames the tag.
#[test]
fn an_xml_element_name_typed_at_an_existing_tag_keeps_it_an_element() {
    let root = |body: &str| format!("<burrow config-root=\"true\">\n  {body}\n</burrow>\n");
    let cases = [
        (root("<pu|tcp/>"), "public_address"),
        (
            root("<li|tcp>\n    <nodelay>true</nodelay>\n  </tcp>"),
            "limits",
        ),
        (root("<tc|p/>"), "tcp"),
    ];
    let mut failures = Vec::new();
    for (marked, label) in cases {
        match accepted(Language::XML, &marked, label).and_then(|text| {
            let value =
                parsed(Language::XML, &text).map_err(|error| format!("{error}: {text:?}"))?;
            Ok((text, value))
        }) {
            Ok((text, value)) => {
                let keys = keys_at(&value, &[]);
                let text_leak = text.contains(">tcp") || text.contains(">p/>");
                if !keys.contains(&label.to_string())
                    || text_leak
                    || (label != "tcp" && !keys.contains(&"tcp".to_string()))
                {
                    failures.push(format!("{marked:?} + {label} → {text:?} (keys {keys:?})"));
                }
            }
            Err(error) => failures.push(format!("{marked:?} + {label} → {error}")),
        }
    }
    report(&failures);
}

/// An XML element name typed right before the tag's end (`<hea|>`,
/// `<hea|/>`) gets name suggestions like anywhere else in the name.
#[test]
fn an_xml_name_before_the_tag_end_gets_suggestions() {
    let root = |body: &str| format!("<burrow config-root=\"true\">\n  {body}\n</burrow>\n");
    let mut failures = Vec::new();
    for marked in [root("<hea|/>"), root("<hea|></hea>"), root("<hea|>")] {
        let labels = offered(Language::XML, &marked);
        if !labels.contains(&"heartbeat".to_string()) {
            failures.push(format!("{marked:?} offers {labels:?}"));
        }
    }
    report(&failures);
}

/// Accepting a name at the end of a complete tag's name renames the
/// element, end tag included.
#[test]
fn an_xml_name_accepted_at_the_end_of_a_complete_tag_renames_it() {
    let root = |body: &str| format!("<burrow config-root=\"true\">\n  {body}\n</burrow>\n");
    let mut failures = Vec::new();
    for (marked, expected) in [
        (root("<hea|></hea>"), root("<heartbeat></heartbeat>")),
        (root("<hea|/>"), root("<heartbeat/>")),
        (
            root("<hea|>\n    <interval_secs>5</interval_secs>\n  </hea>"),
            root("<heartbeat>\n    <interval_secs>5</interval_secs>\n  </heartbeat>"),
        ),
    ] {
        match accepted(Language::XML, &marked, "heartbeat") {
            Ok(text) if text == expected => {}
            Ok(text) => failures.push(format!("{marked:?} → {text:?}")),
            Err(error) => failures.push(format!("{marked:?} → {error}")),
        }
    }
    report(&failures);
}

/// An XML attribute accepted right before the tag's end keeps the end and
/// gets its `="…"`.
#[test]
fn an_xml_attribute_accepted_before_the_tag_end_is_whole() {
    let mut failures = Vec::new();
    for (marked, expected) in [
        ("<tcp no|>\n</tcp>\n", "<tcp nodelay=\"true\">\n</tcp>\n"),
        ("<tcp no|></tcp>\n", "<tcp nodelay=\"true\"></tcp>\n"),
        ("<tcp no|/>\n", "<tcp nodelay=\"true\"/>\n"),
    ] {
        match accepted(Language::XML, marked, "nodelay") {
            Ok(text) if text.replace("\"\"", "\"true\"") == expected || text == expected => {}
            Ok(text) => failures.push(format!("{marked:?} → {text:?}")),
            Err(error) => failures.push(format!("{marked:?} → {error}")),
        }
    }
    report(&failures);
}

/// An XML attribute is not offered when the element already has
/// that key as a child element, and the reverse.
#[test]
fn an_xml_key_is_one_key_whether_attribute_or_child() {
    let attributes = offered(
        Language::XML,
        "<tcp |>\n  <nodelay>true</nodelay>\n</tcp>\n",
    );
    assert!(
        !attributes.contains(&"nodelay".to_string()),
        "{attributes:?}"
    );
    assert!(
        attributes.contains(&"keepalive_secs".to_string()),
        "{attributes:?}"
    );
    let children = offered(Language::XML, "<tcp nodelay=\"true\">\n  <|\n</tcp>\n");
    assert!(!children.contains(&"nodelay".to_string()), "{children:?}");
}

/// Completing in an XML end tag's name renames the element, its
/// start tag included.
#[test]
fn an_xml_end_tag_name_completes_and_renames_both_tags() {
    let marked = "<burrow config-root=\"true\">\n  <publ>x</pu|bl>\n</burrow>\n";
    let text = accepted(Language::XML, marked, "public_address").unwrap();
    assert_eq!(
        text,
        "<burrow config-root=\"true\">\n  <public_address>x</public_address>\n</burrow>\n"
    );
}

/// A closing tag being typed offers only the element it closes,
/// and accepting writes that end tag, renaming nothing.
#[test]
fn a_closing_tag_being_typed_closes_its_element() {
    let mut failures = Vec::new();
    for (marked, name, expected) in [
        (
            "<websocket>\n  <path>/x</path>\n  </we|",
            "websocket",
            "<websocket>\n  <path>/x</path>\n  </websocket>",
        ),
        (
            "<websocket>\n  <path>/x</|",
            "path",
            "<websocket>\n  <path>/x</path>",
        ),
        (
            "<websocket>\n  <path>/x</path>\n  </|\n",
            "websocket",
            "<websocket>\n  <path>/x</path>\n  </websocket>\n",
        ),
    ] {
        let labels = offered(Language::XML, marked);
        if labels != [name] {
            failures.push(format!("{marked:?}: offered {labels:?}"));
        }
        match accepted(Language::XML, marked, name) {
            Ok(text) if text == expected => {}
            other => failures.push(format!("{marked:?}: {other:?}")),
        }
    }
    report(&failures);
}

/// In an XML document with misspelled end tags, the elements
/// after one are not nested ever deeper (which made every keystroke slower
/// with the document's length): a misspelled end closes the innermost.
#[test]
fn a_misspelled_xml_end_tag_closes_the_innermost_element() {
    let mut text = String::new();
    for index in 0..200 {
        text.push_str(&format!("<listen>{index}</listn>\n"));
    }
    text.push_str("<tcp>\n  <no|");
    let at = text.find('|').unwrap();
    let source = text.replacen('|', "", 1);
    let context = Language::XML
        .behavior()
        .completion_context(&source, source[..at].encode_utf16().count());
    assert_eq!(
        context.path,
        vec![crate::editor::language::SchemaPathSegment::Key("tcp".into())]
    );
}

/// `/>` inside a quoted attribute value does not end the tag.
#[test]
fn a_tag_end_inside_a_quoted_attribute_value_is_text() {
    let labels = offered(
        Language::XML,
        "<websocket path=\"a/>b\">\n  |\n</websocket>\n",
    );
    assert!(labels.contains(&"host".to_string()), "{labels:?}");
}
