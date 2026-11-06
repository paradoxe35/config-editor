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
            "limits":{"type":"object","properties":{
                "handshake_timeout_secs":{"type":"integer"},
                "max_handshakes":{"type":"integer"},
                "max_pending":{"type":"integer"}
            }},
            "web":{"type":"object","properties":{
                "certificates":{"type":"array","items":{"type":"object","required":["hosts"],"properties":{
                    "hosts":{"type":"array","items":{"type":"string"}},
                    "cert":{"type":"string"}
                }}}
            }},
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

#[track_caller]
fn assert_not_offered(language: Language, marked: &str, label: &str) {
    let labels = offered(language, marked);
    assert!(
        !labels.iter().any(|offered| offered == label),
        "{language:?}: `{label}` is already set but offered at {marked:?}; offered {labels:?}"
    );
}

/// `(language, document with |, key already set, sibling still free)`.
const DUPLICATE_CASES: &[(Language, &str, &str, &str)] = &[
    // TOML: a table section, mid-typing (the document does not parse), keys
    // below the caret, inline tables, and another table using the same name.
    (
        Language::TOML,
        "[limits]\nhandshake_timeout_secs = 5\nmax_handshakes = 2048\nmax|",
        "max_handshakes",
        "max_pending",
    ),
    (
        Language::TOML,
        "[limits]\n|\nmax_handshakes = 1\n",
        "max_handshakes",
        "max_pending",
    ),
    (
        Language::TOML,
        "[limits]\nmax_handshakes = 1\nbroken =\n|",
        "max_handshakes",
        "max_pending",
    ),
    (
        Language::TOML,
        "clients = { acme = { max_connections = 1, | } }\n",
        "max_connections",
        "mode",
    ),
    (
        Language::TOML,
        "[clients.a]\nmode = \"fast\"\n[clients.b]\nsecret = \"x\"\n|",
        "secret",
        "mode",
    ),
    // YAML: block mappings (also mid-typing) and flow mappings.
    (
        Language::YAML,
        "limits:\n  max_handshakes: 1\n  max|",
        "max_handshakes",
        "max_pending",
    ),
    (
        Language::YAML,
        "limits:\n  |\n  max_handshakes: 1\n",
        "max_handshakes",
        "max_pending",
    ),
    (
        Language::YAML,
        "limits:\n  max_handshakes: 1\n    oops: 1\n  |\n",
        "max_handshakes",
        "handshake_timeout_secs",
    ),
    (
        Language::YAML,
        "clients:\n  acme: { max_connections: 1, | }\n",
        "max_connections",
        "mode",
    ),
    // JSON: nested objects, one-line and multi-line, also mid-typing.
    (
        Language::JSON,
        "{\"limits\": {\"max_handshakes\": 1, \"|\"}}",
        "max_handshakes",
        "max_pending",
    ),
    (
        Language::JSON,
        "{\n  \"limits\": {\n    \"max_handshakes\": 1,\n    \"|\n  }\n}",
        "max_handshakes",
        "max_pending",
    ),
    // HCL: blocks and value maps.
    (
        Language::HCL,
        "limits {\n  max_handshakes = 1\n  |\n}\n",
        "max_handshakes",
        "max_pending",
    ),
    (
        Language::HCL,
        "clients = { acme = { max_connections = 1, | } }\n",
        "max_connections",
        "mode",
    ),
    // XML: a scalar element is not repeated.
    (
        Language::XML,
        "<limits><max_handshakes>1</max_handshakes><|</limits>",
        "max_handshakes",
        "max_pending",
    ),
    (
        Language::XML,
        "<limits>\n  <max_handshakes>1</max_handshakes>\n  |\n</limits>",
        "max_handshakes",
        "max_pending",
    ),
];

#[test]
fn keys_already_set_are_not_offered_again() {
    for &(language, marked, taken, free) in DUPLICATE_CASES {
        assert_not_offered(language, marked, taken);
        assert_offers(language, marked, free);
    }
}

#[test]
fn the_same_key_stays_available_in_a_sibling_container() {
    assert_offers(
        Language::TOML,
        "[clients.a]\nsecret = \"x\"\n[clients.b]\n|",
        "secret",
    );
    assert_offers(
        Language::YAML,
        "clients:\n  a:\n    secret: x\n  b:\n    |",
        "secret",
    );
    assert_offers(
        Language::JSON,
        "{\"clients\": {\"a\": {\"secret\": \"x\"}, \"b\": {\"|\"}}}",
        "secret",
    );
    assert_offers(
        Language::HCL,
        "clients {\n  a {\n    secret = \"x\"\n  }\n  b {\n    |\n  }\n}\n",
        "secret",
    );
}

#[test]
fn an_xml_array_element_can_be_repeated() {
    // Repeated siblings are how XML writes an array.
    assert_offers(
        Language::XML,
        "<services><web><visitors>alice</visitors><|</web></services>",
        "visitors",
    );
}

#[test]
fn an_object_key_typed_directly_in_an_inline_array_starts_a_new_item() {
    let cases: &[(Language, &str, &str)] = &[
        (
            Language::TOML,
            "[web]\ncertificates = [|]\n",
            "[web]\ncertificates = [{ hosts = [|] }]\n",
        ),
        (
            Language::JSON,
            "{\"web\": {\"certificates\": [\"|\"]}}",
            "{\"web\": {\"certificates\": [{\"hosts\": [\"|\"]}]}}",
        ),
        (
            Language::YAML,
            "web: { certificates: [|] }\n",
            "web: { certificates: [{ hosts: [|] }] }\n",
        ),
        (
            Language::HCL,
            "web = { certificates = [|] }\n",
            "web = { certificates = [{ hosts = [|] }] }\n",
        ),
    ];
    for &(language, marked, expected) in cases {
        let accepted = super::super::completion::accept_marked(
            marked,
            "hosts",
            &schema(),
            language.behavior(),
        );
        assert_eq!(accepted, expected, "{language:?}");
        language
            .parse(&accepted.replace('|', ""))
            .unwrap_or_else(|error| panic!("{language:?} result does not parse: {error}"));
    }
}

#[test]
fn an_array_of_tables_at_table_level_opens_a_toml_array_table() {
    let accepted = super::super::completion::accept_marked(
        "[web]\ncert|",
        "certificates",
        &schema(),
        Language::TOML.behavior(),
    );
    assert_eq!(accepted, "[web]\n[[web.certificates]]\nhosts = [|]\n");
}

fn shapes() -> SchemaBundle {
    build_schema(include_str!("../../../tests/fixtures/shapes.schema.json")).unwrap()
}

fn offered_with(language: Language, marked: &str, bundle: &SchemaBundle) -> Vec<String> {
    let at = marked.find('|').expect("a | marker");
    let source = marked.replacen('|', "", 1);
    completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(bundle),
        true,
        language.behavior(),
    )
    .into_iter()
    .map(|option| option.label)
    .collect()
}

/// Arbitrary nesting from a schema: matrices of objects (arrays of arrays of
/// objects), arrays of objects, objects in arrays in objects. XML and HCL are
/// left out: XML names cannot start with a digit and XML has no arrays of
/// arrays; HCL top-level keys must be identifiers.
#[test]
fn completion_and_hover_work_in_every_nesting_shape() {
    let bundle = shapes();
    let matrix_key: &[(Language, &str)] = &[
        (
            Language::JSON,
            "{\"6_deeply_nested_combinations\": {\"matrix_of_objects\": [[{\"x\": 1, \"|\"}]]}}",
        ),
        (
            Language::YAML,
            "6_deeply_nested_combinations:\n  matrix_of_objects:\n    - - x: 1\n        |\n",
        ),
        (
            Language::YAML,
            "6_deeply_nested_combinations:\n  matrix_of_objects: [[{ x: 1, | }]]\n",
        ),
        (
            Language::TOML,
            "[6_deeply_nested_combinations]\nmatrix_of_objects = [[{ x = 1, | }]]\n",
        ),
    ];
    for &(language, marked) in matrix_key {
        let labels = offered_with(language, marked, &bundle);
        assert!(
            labels.contains(&"color".to_string()) && labels.contains(&"y".to_string()),
            "{language:?} {marked:?}: {labels:?}"
        );
        assert!(
            !labels.contains(&"x".to_string()),
            "{language:?} {marked:?}: x is set, offered {labels:?}"
        );
    }
    let matrix_value: &[(Language, &str)] = &[
        (
            Language::JSON,
            "{\"6_deeply_nested_combinations\": {\"matrix_of_objects\": [[{\"color\": |}]]}}",
        ),
        (
            Language::YAML,
            "6_deeply_nested_combinations:\n  matrix_of_objects:\n    - - color: |\n",
        ),
        (
            Language::TOML,
            "[6_deeply_nested_combinations]\nmatrix_of_objects = [[{ color = | }]]\n",
        ),
    ];
    for &(language, marked) in matrix_value {
        let labels = offered_with(language, marked, &bundle);
        assert!(
            labels.contains(&"red".to_string()),
            "{language:?} {marked:?}: {labels:?}"
        );
    }
    let array_of_objects: &[(Language, &str)] = &[
        (
            Language::JSON,
            "{\"4_array_of_objects\": [{\"id\": 1, \"status\": \"active\"}, {\"|\"}]}",
        ),
        (Language::YAML, "4_array_of_objects:\n  - id: 1\n  - |\n"),
        (
            Language::TOML,
            "[[4_array_of_objects]]\nid = 1\n[[4_array_of_objects]]\n|\n",
        ),
    ];
    for &(language, marked) in array_of_objects {
        let labels = offered_with(language, marked, &bundle);
        assert!(
            labels.contains(&"status".to_string()),
            "{language:?} {marked:?}: {labels:?}"
        );
    }
    let history: &[(Language, &str)] = &[
        (
            Language::JSON,
            "{\"6_deeply_nested_combinations\": {\"object_containing_arrays_of_objects\": {\"metadata\": {\"history\": [{\"event\": |}]}}}}",
        ),
        (
            Language::YAML,
            "6_deeply_nested_combinations:\n  object_containing_arrays_of_objects:\n    metadata:\n      history:\n        - event: |\n",
        ),
        (
            Language::TOML,
            "[[6_deeply_nested_combinations.object_containing_arrays_of_objects.metadata.history]]\nevent = |\n",
        ),
    ];
    for &(language, marked) in history {
        let labels = offered_with(language, marked, &bundle);
        assert!(
            labels.contains(&"updated".to_string()),
            "{language:?} {marked:?}: {labels:?}"
        );
    }
    let inner_array_items: &[(Language, &str)] = &[
        (
            Language::JSON,
            "{\"3_key_and_array_value\": {\"tags\": [\"json\", |]}}",
        ),
        (
            Language::YAML,
            "3_key_and_array_value:\n  tags: [json, |]\n",
        ),
        (
            Language::TOML,
            "[3_key_and_array_value]\ntags = [\"json\", |]\n",
        ),
    ];
    for &(language, marked) in inner_array_items {
        let labels = offered_with(language, marked, &bundle);
        assert!(
            labels.contains(&"api".to_string()),
            "{language:?} {marked:?}: {labels:?}"
        );
    }
    let hovers: &[(Language, &str, usize)] = &[
        (
            Language::JSON,
            "{\"6_deeply_nested_combinations\": {\"matrix_of_objects\": [[{\"x\": 0, \"y\": 0}]]}}",
            0,
        ),
        (
            Language::YAML,
            "6_deeply_nested_combinations:\n  matrix_of_objects: [[{ x: 0, y: 0 }]]\n",
            1,
        ),
        (
            Language::TOML,
            "[6_deeply_nested_combinations]\nmatrix_of_objects = [[{ x = 0, y = 0 }]]\n",
            1,
        ),
    ];
    for &(language, source, line) in hovers {
        let text = source.split('\n').nth(line).unwrap();
        // The key `x`, not the `x` in `matrix_of_objects`.
        let column = ["\"x\"", "{ x"]
            .iter()
            .find_map(|needle| text.find(needle).map(|at| at + needle.len() - 1))
            .unwrap();
        let hover = key_hover(source, line, column, Some(&bundle), language.behavior());
        assert_eq!(
            hover.and_then(|hover| hover.description).as_deref(),
            Some("Column"),
            "{language:?} {source:?}"
        );
    }
}
