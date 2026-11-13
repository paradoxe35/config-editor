//! XML namespace declarations (`xmlns`, `xmlns:*`) and the schema-instance
//! attributes (`xsi:schemaLocation`, `xsi:*`) are markup, not data: a
//! `pom.xml` reads without them, so a closed schema that does not list them
//! finds nothing wrong, other views do not show them, and every save from
//! another view keeps them byte for byte, as the host wrote them.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::save_matrix::{ALL, report, save_value};
use serde_json::{Value, json};

/// A `pom.xml` head: the declarations over lines, as Maven writes them,
/// and a nested element with its own.
const POM: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<project xmlns=\"http://maven.apache.org/POM/4.0.0\"\n         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n         xsi:schemaLocation=\"http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd\">\n  <modelVersion>4.0.0</modelVersion>\n  <groupId>org.example</groupId>\n  <artifactId>app</artifactId>\n  <build xmlns:m=\"urn:m\" m:kind=\"jar\">\n    <finalName>app</finalName>\n  </build>\n</project>\n";

fn views() -> impl Iterator<Item = Language> {
    ALL.into_iter().filter(|edit| *edit != Language::XML)
}

#[test]
fn declarations_and_schema_instance_attributes_are_not_data() {
    let read = Language::XML.parse(POM).expect("reads");
    assert_eq!(
        read,
        json!({"project": {
            "modelVersion": "4.0.0",
            "groupId": "org.example",
            "artifactId": "app",
            "build": {"m:kind": "jar", "finalName": "app"}
        }})
    );
    // An element holding text and declarations alone is its text.
    let text = "<r xmlns=\"urn:r\"><a xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"t\">1</a><e xmlns=\"urn:e\"/></r>\n";
    assert_eq!(
        Language::XML.parse(text).expect("reads"),
        json!({"r": {"a": 1, "e": {}}})
    );
    // `xsi` is the prefix bound to the schema-instance namespace: bound to
    // another, its attributes are data.
    let other = "<r xmlns:xsi=\"urn:other\" xsi:k=\"1\"/>\n";
    assert_eq!(
        Language::XML.parse(other).expect("reads"),
        json!({"r": {"xsi:k": 1}})
    );
    let renamed =
        "<r xmlns:i=\"http://www.w3.org/2001/XMLSchema-instance\" i:nil=\"true\" k=\"1\"/>\n";
    assert_eq!(
        Language::XML.parse(renamed).expect("reads"),
        json!({"r": {"k": 1}})
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_closed_schema_that_lists_no_declaration_finds_nothing_wrong() {
    let schema = crate::editor::diagnostics::Schema::compile(
        r#"{"type": "object", "additionalProperties": false, "properties": {"project": {
            "type": "object", "additionalProperties": false,
            "properties": {
                "modelVersion": {"type": "string"},
                "groupId": {"type": "string"},
                "artifactId": {"type": "string"},
                "build": {"type": "object", "additionalProperties": false,
                    "properties": {"finalName": {"type": "string"}, "m:kind": {"type": "string"}}}
            }
        }}}"#,
    )
    .expect("compiles");
    let problems = schema.validate(Language::XML, POM);
    assert!(problems.is_empty(), "{problems:#?}");
}

/// No schema key is markup: hovering a declaration shows no key's docs,
/// even where a schema documents every key of its element.
#[test]
fn hovering_a_declaration_shows_no_keys_docs() {
    let schema = crate::editor::diagnostics::Schema::compile(
        r#"{"properties": {"project": {"patternProperties": {".*": {"description": "any key"}}}}}"#,
    )
    .expect("compiles");
    for (line, column) in [(2, 10), (3, 10), (4, 10)] {
        let docs = schema.hover(
            Language::XML,
            POM,
            crate::editor::TextPosition::new(line, column),
        );
        assert!(docs.is_none(), "{line}:{column}: {docs:?}");
    }
    let docs = schema.hover(Language::XML, POM, crate::editor::TextPosition::new(5, 4));
    assert!(docs.is_some(), "modelVersion is a key");
}

#[test]
fn other_views_do_not_show_declarations_and_keep_them_unchanged() {
    let mut failures = Vec::new();
    for edit in views() {
        let Some(bridge) = Bridge::new(Language::XML, edit, None) else {
            continue;
        };
        let shown = match bridge.to_edit(POM) {
            Ok(shown) => shown.text,
            Err(error) => {
                failures.push(format!("{edit}: {error}"));
                continue;
            }
        };
        if shown.contains("xmlns") || shown.contains("schemaLocation") {
            failures.push(format!("{edit} shows a declaration: {shown}"));
        }
        match bridge.to_host(&shown, POM) {
            Ok(saved) if saved == POM => {}
            other => failures.push(format!("{edit} round trip: {other:?}")),
        }
    }
    report(failures);
}

/// `value` without a key spelled as a declaration or a schema-instance
/// attribute, as a view that does not show them holds it.
fn undeclared(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|key, _| {
                !(key == "xmlns" || key.starts_with("xmlns:") || key.starts_with("xsi:"))
            });
            map.values_mut().for_each(undeclared);
        }
        Value::Array(items) => items.iter_mut().for_each(undeclared),
        _ => {}
    }
}

/// Each save keeps the declarations as written: a value changed, a key
/// added, a key removed, the keys reordered, the element holding some
/// rewritten.
#[test]
fn saves_from_other_views_keep_declarations_byte_for_byte() {
    let head = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<project xmlns=\"http://maven.apache.org/POM/4.0.0\"\n         xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n         xsi:schemaLocation=\"http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd\">\n";
    type Change = fn(&mut Value);
    let cases: [(&str, Change, &str); 5] = [
        (
            "a value changed",
            |value| value["project"]["artifactId"] = json!("lib"),
            "  <modelVersion>4.0.0</modelVersion>\n  <groupId>org.example</groupId>\n  <artifactId>lib</artifactId>\n  <build xmlns:m=\"urn:m\" m:kind=\"jar\">\n    <finalName>app</finalName>\n  </build>\n</project>\n",
        ),
        (
            "a key added",
            |value| {
                // Before `build`, where a TOML view keeps its scalars.
                value["project"].as_object_mut().unwrap().shift_insert(
                    3,
                    "version".into(),
                    json!("1-SNAPSHOT"),
                );
            },
            "  <modelVersion>4.0.0</modelVersion>\n  <groupId>org.example</groupId>\n  <artifactId>app</artifactId>\n  <version>1-SNAPSHOT</version>\n  <build xmlns:m=\"urn:m\" m:kind=\"jar\">\n    <finalName>app</finalName>\n  </build>\n</project>\n",
        ),
        (
            "a key removed",
            |value| {
                value["project"]
                    .as_object_mut()
                    .unwrap()
                    .shift_remove("groupId");
            },
            "  <modelVersion>4.0.0</modelVersion>\n  <artifactId>app</artifactId>\n  <build xmlns:m=\"urn:m\" m:kind=\"jar\">\n    <finalName>app</finalName>\n  </build>\n</project>\n",
        ),
        (
            "the attribute using a declaration removed",
            |value| {
                value["project"]["build"]
                    .as_object_mut()
                    .unwrap()
                    .shift_remove("m:kind");
            },
            "  <modelVersion>4.0.0</modelVersion>\n  <groupId>org.example</groupId>\n  <artifactId>app</artifactId>\n  <build xmlns:m=\"urn:m\">\n    <finalName>app</finalName>\n  </build>\n</project>\n",
        ),
        (
            "the element holding declarations given a child",
            |value| {
                value["project"]["build"]
                    .as_object_mut()
                    .unwrap()
                    .insert("directory".into(), json!("out"));
            },
            "  <modelVersion>4.0.0</modelVersion>\n  <groupId>org.example</groupId>\n  <artifactId>app</artifactId>\n  <build xmlns:m=\"urn:m\" m:kind=\"jar\">\n    <finalName>app</finalName>\n    <directory>out</directory>\n  </build>\n</project>\n",
        ),
    ];
    let mut failures = Vec::new();
    for (label, change, body) in cases {
        let expected = format!("{head}{body}");
        for edit in views() {
            // The view's value holds no declaration: the save takes them
            // from the host.
            let saved = save_value(Language::XML, edit, POM, &|value| {
                undeclared(value);
                change(value);
            });
            match saved {
                Ok(saved) if saved == expected => {}
                other => failures.push(format!("{label} via {edit}: {other:?}")),
            }
        }
    }
    report(failures);
}

/// Reordering an element's attributes in a view moves only its data
/// attributes: its declarations stay where the host wrote them.
#[test]
fn reordered_attributes_leave_declarations_in_place() {
    let host = "<r xmlns:n=\"urn:x\" n:a=\"1\" b=\"2\"><s>v</s></r>\n";
    let mut failures = Vec::new();
    for edit in views() {
        let saved = save_value(Language::XML, edit, host, &|value| {
            let map = value["r"].as_object_mut().unwrap();
            let taken = std::mem::take(map);
            for key in ["b", "n:a", "s"] {
                map.insert(key.into(), taken[key].clone());
            }
        });
        match saved {
            Ok(saved) if saved == "<r xmlns:n=\"urn:x\" b=\"2\" n:a=\"1\"><s>v</s></r>\n" => {}
            other => failures.push(format!("via {edit}: {other:?}")),
        }
    }
    report(failures);
}

/// Converting drops markup the target has no place for, comments and
/// declarations alike; what converts back is plain XML.
#[test]
fn converting_leaves_declarations_behind_like_comments() {
    for target in views() {
        let converted = Language::XML.convert(POM, target).expect("converts");
        assert!(
            !converted.text.contains("xmlns"),
            "{target}: {}",
            converted.text
        );
        assert!(
            converted.losses.is_empty(),
            "{target}: {:?}",
            converted.losses
        );
        let plain = "<project xmlns=\"urn:p\"><a>1</a></project>\n";
        let there = Language::XML.convert(plain, target).expect("converts").text;
        let back = target
            .convert(&there, Language::XML)
            .expect("converts back")
            .text;
        assert_eq!(back, "<project>\n  <a>1</a>\n</project>\n", "{target}");
    }
}
