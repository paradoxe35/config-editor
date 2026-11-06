//! Nested-structure parity across languages: hover docs, key completion, and
//! value completion for keys nested on one line (inline tables, flow mappings,
//! one-line objects, value maps, attributes) and in nested blocks.

use super::super::completion::{completions, key_hover};
use super::super::diagnostics::{SchemaBundle, build_schema};
use super::Language;

fn schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{
            "clients":{"type":"object","additionalProperties":{"type":"object","properties":{
                "secret":{"type":"string","description":"The client secret"},
                "max_connections":{"type":"integer","description":"Connection cap"},
                "mode":{"type":"string","enum":["fast","safe"],"description":"Transfer mode"}
            }}},
            "services":{"type":"object","additionalProperties":{"type":"object","properties":{
                "protocol":{"type":"string","enum":["tcp","udp"]},
                "backends":{"type":"array","items":{"type":"object","properties":{
                    "name":{"type":"string"},
                    "weight":{"type":"integer","description":"Share of traffic"}
                }}},
                "visitors":{"type":"array","items":{"type":"string","enum":["alice","bob"]}}
            }}}
        }}"#,
    )
    .unwrap()
}

/// The hover doc for `key` (its first occurrence on `line`, 0-based).
fn hover(language: Language, source: &str, line: usize, key: &str) -> Option<String> {
    let text = source.split('\n').nth(line)?;
    let byte = text.find(key)?;
    let column = text[..byte].chars().count() + 1;
    key_hover(source, line, column, Some(&schema()), language.behavior())
        .filter(|hover| hover.key == key)
        .and_then(|hover| hover.description)
}

/// Labels offered (as Ctrl+Space) at the `|` marker.
fn offered(language: Language, marked: &str) -> Vec<String> {
    let at = marked.find('|').expect("a | marker");
    let source = marked.replacen('|', "", 1);
    completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&schema()),
        true,
        language.behavior(),
    )
    .into_iter()
    .map(|option| option.label)
    .collect()
}

#[track_caller]
fn assert_hover(language: Language, source: &str, line: usize, key: &str, doc: &str) {
    assert_eq!(
        hover(language, source, line, key).as_deref(),
        Some(doc),
        "{language:?}: hover `{key}` on line {line} of {source:?}"
    );
}

#[track_caller]
fn assert_offers(language: Language, marked: &str, label: &str) {
    let labels = offered(language, marked);
    assert!(
        labels.iter().any(|offered| offered == label),
        "{language:?}: `{label}` not offered at {marked:?}; offered {labels:?}"
    );
}

#[test]
fn hover_documents_keys_nested_on_one_line() {
    let cases: &[(Language, &str, usize)] = &[
        (
            Language::TOML,
            "[clients]\nparadoxe = { secret = [], max_connections = 123 }\n",
            1,
        ),
        (
            Language::TOML,
            "clients = { acme = { secret = \"x\", max_connections = 3 } }\n",
            0,
        ),
        (
            Language::YAML,
            "clients:\n  acme: { secret: x, max_connections: 3 }\n",
            1,
        ),
        (
            Language::YAML,
            "clients: { acme: { secret: x, max_connections: 3 } }\n",
            0,
        ),
        (
            Language::JSON,
            "{\"clients\": {\"acme\": {\"secret\": \"x\", \"max_connections\": 3}}}\n",
            0,
        ),
        (
            Language::HCL,
            "clients = { acme = { secret = \"x\", max_connections = 3 } }\n",
            0,
        ),
        (
            Language::XML,
            "<clients><acme max_connections=\"3\"><secret>x</secret></acme></clients>\n",
            0,
        ),
    ];
    for &(language, source, line) in cases {
        assert_hover(language, source, line, "secret", "The client secret");
        assert_hover(language, source, line, "max_connections", "Connection cap");
    }
}

#[test]
fn hover_documents_keys_in_nested_blocks() {
    let cases: &[(Language, &str, usize)] = &[
        (Language::TOML, "[clients.acme]\nsecret = \"x\"\n", 1),
        (Language::YAML, "clients:\n  acme:\n    secret: x\n", 2),
        (
            Language::JSON,
            "{\n  \"clients\": {\n    \"acme\": {\n      \"secret\": \"x\"\n    }\n  }\n}\n",
            3,
        ),
        (
            Language::HCL,
            "clients {\n  acme {\n    secret = \"x\"\n  }\n}\n",
            2,
        ),
        (
            Language::XML,
            "<clients>\n  <acme>\n    <secret>x</secret>\n  </acme>\n</clients>\n",
            2,
        ),
    ];
    for &(language, source, line) in cases {
        assert_hover(language, source, line, "secret", "The client secret");
    }
}

#[test]
fn keys_complete_inside_inline_containers() {
    let cases: &[(Language, &str)] = &[
        (Language::TOML, "[clients]\nacme = { se| }\n"),
        (
            Language::TOML,
            "clients = { acme = { max_connections = 1, se| } }\n",
        ),
        (Language::YAML, "clients:\n  acme: { se| }\n"),
        (
            Language::YAML,
            "clients:\n  acme: { max_connections: 1, se| }\n",
        ),
        (Language::JSON, "{\"clients\": {\"acme\": {\"se|\"}}}\n"),
        (Language::HCL, "clients = { acme = { se| } }\n"),
        (Language::XML, "<clients><acme><se|</acme></clients>\n"),
        (Language::XML, "<clients><acme se|/></clients>\n"),
    ];
    for &(language, marked) in cases {
        assert_offers(language, marked, "secret");
    }
}

#[test]
fn values_complete_inside_inline_containers() {
    let cases: &[(Language, &str)] = &[
        (Language::TOML, "[clients]\nacme = { mode = | }\n"),
        (
            Language::TOML,
            "[clients]\nacme = { secret = \"x\", mode = | }\n",
        ),
        (Language::YAML, "clients:\n  acme: { mode: | }\n"),
        (Language::YAML, "clients:\n  acme: { secret: x, mode: | }\n"),
        (Language::JSON, "{\"clients\": {\"acme\": {\"mode\": |}}}\n"),
        (Language::HCL, "clients = { acme = { mode = | } }\n"),
        (Language::XML, "<clients><acme mode=\"|\"/></clients>\n"),
        (
            Language::XML,
            "<clients><acme><mode>|</mode></acme></clients>\n",
        ),
    ];
    for &(language, marked) in cases {
        assert_offers(language, marked, "fast");
    }
}

#[test]
fn values_complete_in_nested_blocks() {
    let cases: &[(Language, &str)] = &[
        (Language::TOML, "[clients.acme]\nmode = |\n"),
        (Language::YAML, "clients:\n  acme:\n    mode: |\n"),
        (
            Language::JSON,
            "{\n  \"clients\": {\n    \"acme\": {\n      \"mode\": |\n    }\n  }\n}\n",
        ),
        (Language::HCL, "clients {\n  acme {\n    mode = |\n  }\n}\n"),
    ];
    for &(language, marked) in cases {
        assert_offers(language, marked, "fast");
    }
}

#[test]
fn keys_and_hover_resolve_inside_inline_arrays_of_objects() {
    let cases: &[(Language, &str)] = &[
        (
            Language::TOML,
            "services = { web = { backends = [{ name = \"a\" }, { we| }] } }\n",
        ),
        (
            Language::YAML,
            "services:\n  web: { backends: [{ name: a }, { we| }] }\n",
        ),
        (
            Language::HCL,
            "services = { web = { backends = [{ name = \"a\" }, { we| }] } }\n",
        ),
        (
            Language::JSON,
            "{\"services\": {\"web\": {\"backends\": [{\"name\": \"a\"}, {\"we|\"}]}}}\n",
        ),
    ];
    for &(language, marked) in cases {
        assert_offers(language, marked, "weight");
    }
    let hovers: &[(Language, &str, usize)] = &[
        (
            Language::TOML,
            "services = { web = { backends = [{ name = \"a\" }, { weight = 2 }] } }\n",
            0,
        ),
        (
            Language::YAML,
            "services:\n  web: { backends: [{ name: a }, { weight: 2 }] }\n",
            1,
        ),
        (
            Language::HCL,
            "services = { web = { backends = [{ name = \"a\" }, { weight = 2 }] } }\n",
            0,
        ),
        (
            Language::JSON,
            "{\"services\": {\"web\": {\"backends\": [{\"name\": \"a\"}, {\"weight\": 2}]}}}\n",
            0,
        ),
    ];
    for &(language, source, line) in hovers {
        assert_hover(language, source, line, "weight", "Share of traffic");
    }
}

#[test]
fn inline_array_items_complete_from_the_item_schema() {
    let cases: &[(Language, &str)] = &[
        (
            Language::TOML,
            "services = { web = { visitors = [\"alice\", |] } }\n",
        ),
        (
            Language::YAML,
            "services:\n  web: { visitors: [alice, |] }\n",
        ),
        (
            Language::HCL,
            "services = { web = { visitors = [\"alice\", |] } }\n",
        ),
        (
            Language::JSON,
            "{\"services\": {\"web\": {\"visitors\": [\"alice\", |]}}}\n",
        ),
    ];
    for &(language, marked) in cases {
        assert_offers(language, marked, "bob");
    }
}

#[test]
fn an_empty_xml_element_offers_values_or_children_by_schema() {
    // A scalar offers its values, an object its child elements.
    assert_offers(
        Language::XML,
        "<services><web><protocol>|</protocol></web></services>\n",
        "udp",
    );
    assert_offers(
        Language::XML,
        "<clients><acme>|</acme></clients>\n",
        "secret",
    );
}
