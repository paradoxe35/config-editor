//! Keys another host spells as the XML model spells an attribute (`@a`,
//! an npm `@scope`): an XML view shows them as attributes, and a save from
//! it keeps the host's `@a` (the view read back has `a`), whatever else is
//! edited; what no XML attribute can hold (`@a` holding a map, `@1`) and
//! what XML would read otherwise (text beside child elements) makes the
//! switch wait.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::save_matrix::{report, save_value};
use serde_json::{Value, json};

type Change = fn(&mut Value);

/// The same document (`r` holding `@a` 1 and `b` 2) in each other host,
/// with the texts expected after each change.
struct Host {
    language: Language,
    text: &'static str,
    a_edited: &'static str,
    b_edited: &'static str,
    b_removed: &'static str,
}

const HOSTS: [Host; 4] = [
    Host {
        language: Language::YAML,
        text: "r:\n  \"@a\": 1 # a\n  b: 2\n",
        a_edited: "r:\n  \"@a\": 5 # a\n  b: 2\n",
        b_edited: "r:\n  \"@a\": 1 # a\n  b: 3\n",
        b_removed: "r:\n  \"@a\": 1 # a\n",
    },
    Host {
        language: Language::JSON,
        text: "{\n  \"r\": {\n    \"@a\": 1,\n    \"b\": 2\n  }\n}\n",
        a_edited: "{\n  \"r\": {\n    \"@a\": 5,\n    \"b\": 2\n  }\n}\n",
        b_edited: "{\n  \"r\": {\n    \"@a\": 1,\n    \"b\": 3\n  }\n}\n",
        b_removed: "{\n  \"r\": {\n    \"@a\": 1\n  }\n}\n",
    },
    Host {
        language: Language::TOML,
        text: "[r]\n\"@a\" = 1 # a\nb = 2\n",
        a_edited: "[r]\n\"@a\" = 5 # a\nb = 2\n",
        b_edited: "[r]\n\"@a\" = 1 # a\nb = 3\n",
        b_removed: "[r]\n\"@a\" = 1 # a\n",
    },
    Host {
        language: Language::HCL,
        text: "r = {\n  \"@a\" = 1 # a\n  b = 2\n}\n",
        a_edited: "r = {\n  \"@a\" = 5 # a\n  b = 2\n}\n",
        b_edited: "r = {\n  \"@a\" = 1 # a\n  b = 3\n}\n",
        b_removed: "r = {\n  \"@a\" = 1 # a\n}\n",
    },
];

#[test]
fn an_at_key_of_another_host_is_an_attribute_that_saves_back_as_itself() {
    let mut failures = Vec::new();
    for host in &HOSTS {
        let cases: [(&str, Change, &str); 3] = [
            (
                "the attribute edited",
                |value| value["r"]["a"] = json!("5"),
                host.a_edited,
            ),
            (
                "the child edited",
                |value| value["r"]["b"] = json!("3"),
                host.b_edited,
            ),
            (
                "the child removed",
                |value| {
                    value["r"].as_object_mut().unwrap().shift_remove("b");
                },
                host.b_removed,
            ),
        ];
        for (label, change, expected) in cases {
            match save_value(host.language, Language::XML, host.text, &change) {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!(
                    "{label}: {} host via XML:\n--- expected\n{expected}--- saved\n{saved}",
                    host.language
                )),
                Err(error) => {
                    failures.push(format!("{label}: {} host via XML: {error}", host.language))
                }
            }
        }
    }
    report(failures);
}

/// An npm-style document: a scoped package beside a plain one.
#[test]
fn a_scoped_package_keeps_its_at_through_an_edit_of_its_neighbour() {
    let host = "{\n  \"deps\": {\n    \"@babel\": \"1\",\n    \"lodash\": \"2\"\n  }\n}\n";
    let saved = save_value(Language::JSON, Language::XML, host, &|value: &mut Value| {
        value["deps"]["lodash"] = json!("3");
    });
    assert_eq!(
        saved.as_deref(),
        Ok("{\n  \"deps\": {\n    \"@babel\": \"1\",\n    \"lodash\": \"3\"\n  }\n}\n")
    );
}

/// The host texts an XML view cannot show so that its save reads them
/// back: the switch waits, naming the value.
#[test]
fn the_switch_to_xml_waits_for_what_xml_would_read_otherwise() {
    let mut failures = Vec::new();
    let cases: [(&str, Language, &str, &str); 6] = [
        (
            "text beside a child",
            Language::YAML,
            "r:\n  \"#text\": t\n  b: 1\n",
            "/r",
        ),
        (
            "text beside a child",
            Language::JSON,
            "{\"r\": {\"#text\": \"t\", \"b\": 1}}\n",
            "/r",
        ),
        (
            "text beside a child",
            Language::TOML,
            "[r]\n\"#text\" = \"t\"\nb = 1\n",
            "/r",
        ),
        (
            "text beside a child",
            Language::HCL,
            "r = {\n  \"#text\" = \"t\"\n  b = 1\n}\n",
            "/r",
        ),
        (
            "an @ key holding a map",
            Language::YAML,
            "r:\n  \"@a\":\n    x: 1\n",
            "/r/@a",
        ),
        (
            "an @ key no attribute name",
            Language::JSON,
            "{\"r\": {\"@1\": 1}}\n",
            "/r/@1",
        ),
    ];
    for (label, host, text, pointer) in cases {
        let bridge = Bridge::new(host, Language::XML, None).unwrap();
        match bridge.to_edit(text) {
            Err(error) if error.to_string().contains(pointer) => {}
            Err(error) => failures.push(format!("{label}: {host}: no {pointer} in {error}")),
            Ok(shown) => failures.push(format!("{label}: {host}: switched to\n{}", shown.text)),
        }
    }
    report(failures);
}

/// A converted document is read alone: what it reads back otherwise is
/// listed (`@a` reads back `a`), as are documents that would not convert
/// back (text beside child elements).
#[test]
fn converting_to_xml_lists_an_at_key_and_refuses_text_beside_children() {
    let converted = Language::YAML
        .convert("r:\n  \"@a\": 1\n  b: 2\n", Language::XML)
        .unwrap();
    assert_eq!(converted.text, "<r a=\"1\">\n  <b>2</b>\n</r>\n");
    assert_eq!(converted.losses, ["/r/@a"]);
    // `@a` beside a child `a` reads back as itself.
    let beside = Language::YAML
        .convert("r:\n  \"@a\": 1\n  a: 2\n", Language::XML)
        .unwrap();
    assert_eq!(beside.losses, Vec::<String>::new());
    let mixed = Language::YAML.convert("r:\n  \"#text\": t\n  b: 1\n", Language::XML);
    assert!(mixed.is_err_and(|error| error.to_string().contains("/r")));
}
