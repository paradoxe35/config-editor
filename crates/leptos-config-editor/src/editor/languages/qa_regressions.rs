//! Regressions from hands-on QA of the demo: each test is a reported case,
//! replayed against the real burrow schema.

use super::super::completion::{accept_marked, completions};
use super::super::diagnostics::{SchemaBundle, build_schema};
use super::Language;

fn burrow() -> SchemaBundle {
    build_schema(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap()
}

fn accept(language: Language, marked: &str, label: &str) -> String {
    accept_marked(marked, label, &burrow(), language.behavior())
}

fn labels(language: Language, marked: &str) -> Vec<String> {
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&burrow()),
        true,
        language.behavior(),
    )
    .into_iter()
    .map(|option| option.label)
    .collect()
}

#[track_caller]
fn assert_parses(language: Language, text: &str) {
    if let Err(error) = language.parse(&text.replace('|', "")) {
        panic!("{language} does not parse: {error}\n{text}");
    }
}

#[test]
fn a_key_typed_in_front_of_an_existing_entry_is_inserted_before_it() {
    let yaml = accept(Language::YAML, "lim|listen: 7835\n", "limits");
    assert!(yaml.starts_with("limits:"), "{yaml}");
    assert!(
        yaml.contains("\nlisten: 7835\n"),
        "the existing entry is kept: {yaml}"
    );
    let toml = accept(Language::TOML, "lim|listen = 7835\n", "limits");
    assert!(toml.contains("\nlisten = 7835\n"), "{toml}");
    assert!(
        !toml.contains("[limits]"),
        "no header that would capture listen: {toml}"
    );
    assert_parses(Language::TOML, &toml);
    let hcl = accept(Language::HCL, "lim|listen = 7835\n", "limits");
    assert!(hcl.contains("\nlisten = 7835\n"), "{hcl}");
    assert_parses(Language::HCL, &hcl);
}

#[test]
fn completing_the_key_the_caret_is_inside_renames_it_and_keeps_the_value() {
    assert_eq!(
        accept(
            Language::YAML,
            "clients:\n  acme:\n    max_con|nections: 5\n",
            "max_connections"
        ),
        "clients:\n  acme:\n    max_connections|: 5\n"
    );
    assert_eq!(
        accept(
            Language::TOML,
            "[clients.acme]\nmax_con|nections = 5\n",
            "max_connections"
        ),
        "[clients.acme]\nmax_connections| = 5\n"
    );
    // A typo inside the key: the suggestion ends with the word after the caret.
    assert_eq!(
        accept(
            Language::YAML,
            "inse|ecure_plaintext: false",
            "insecure_plaintext"
        ),
        "insecure_plaintext|: false"
    );
}

#[test]
fn a_toml_table_in_the_middle_of_a_table_is_written_inline() {
    let toml = accept(
        Language::TOML,
        "[web]\nhsts = true\n|\ncompress = true\n",
        "letsencrypt",
    );
    assert_eq!(
        toml,
        "[web]\nhsts = true\nletsencrypt = { email = \"|\" }\ncompress = true\n"
    );
    let value = Language::TOML.parse(&toml.replace('|', "a@b.c")).unwrap();
    assert_eq!(value["web"]["compress"], true, "compress stays in [web]");
}

#[test]
fn an_hcl_object_without_required_keys_puts_the_caret_inside() {
    assert_eq!(
        accept(Language::HCL, "lim|\n", "limits"),
        "limits = {\n  |\n}\n"
    );
}

#[test]
fn the_first_key_of_an_empty_json_document_opens_the_root_object() {
    let json = accept(Language::JSON, "|", "limits");
    assert_eq!(json, "{\n  \"limits\": {|}\n}\n");
    assert_parses(Language::JSON, &json);
}

#[test]
fn string_values_start_quoted_with_the_caret_inside() {
    assert_eq!(
        accept(Language::JSON, "{\n  \"pub|\n}", "public_address"),
        "{\n  \"public_address\": \"|\"\n}"
    );
    assert_eq!(
        accept(Language::TOML, "pub|", "public_address"),
        "public_address = \"|\""
    );
    assert_eq!(
        accept(Language::HCL, "pub|", "public_address"),
        "public_address = \"|\""
    );
    // YAML needs no quotes.
    assert_eq!(
        accept(Language::YAML, "pub|", "public_address"),
        "public_address: |"
    );
}

#[test]
fn an_empty_xml_object_element_offers_its_children() {
    let offered = labels(Language::XML, "<web>|</web>");
    assert!(offered.contains(&"letsencrypt".to_string()), "{offered:?}");
}

#[test]
fn an_hcl_value_after_a_space_gets_no_second_space() {
    let hcl = accept(
        Language::HCL,
        "services {\n  web {\n    mode = |\n  }\n}\n",
        "balance",
    );
    assert!(hcl.contains("mode = \"balance\""), "{hcl}");
}

#[test]
fn a_yaml_block_insert_before_existing_content_adds_no_blank_line() {
    let yaml = accept(Language::YAML, "metr|\nlisten: 1\n", "metrics");
    assert!(!yaml.contains("\n\n"), "{yaml}");
    assert!(yaml.ends_with("\nlisten: 1\n"), "{yaml}");
}

#[test]
fn enum_values_are_offered_in_schema_order() {
    let offered = labels(Language::YAML, "services:\n  web:\n    mode: |\n");
    assert_eq!(
        offered[..4],
        ["failover", "balance", "least_connections", "ip_hash"]
    );
}

#[test]
fn containers_show_hover_docs_from_their_header_or_tag() {
    use super::super::completion::key_hover;
    let bundle = burrow();
    let cases: &[(Language, &str, usize, &str)] = &[
        (
            Language::TOML,
            "[web.letsencrypt]\nemail = \"a@b.c\"\n",
            0,
            "letsencrypt",
        ),
        (Language::TOML, "[websocket]\nlisten = 1\n", 0, "websocket"),
        (
            Language::HCL,
            "web {\n  letsencrypt {\n    email = \"a@b.c\"\n  }\n}\n",
            1,
            "letsencrypt",
        ),
        (
            Language::XML,
            "<web><letsencrypt email=\"a@b.c\"/></web>\n",
            0,
            "letsencrypt",
        ),
    ];
    for &(language, source, line, key) in cases {
        let text = source.split('\n').nth(line).unwrap();
        let column = text.find(key).unwrap() + 1;
        let hover = key_hover(source, line, column, Some(&bundle), language.behavior());
        assert!(
            hover
                .as_ref()
                .is_some_and(|hover| hover.key == key && hover.description.is_some()),
            "{language}: no docs for `{key}` in {source:?}"
        );
    }
}

#[test]
fn every_unknown_key_gets_its_own_marker() {
    use super::super::diagnostics::validate;
    let bundle = burrow();
    let source = "{\"listen\": 1, \"foo\": 1, \"bar\": 2, \"baz\": 3}";
    let columns: Vec<Option<usize>> =
        validate(source, Some(&bundle), None, true, Language::JSON.behavior())
            .into_iter()
            .map(|diagnostic| diagnostic.column)
            .collect();
    assert_eq!(columns.len(), 3, "{columns:?}");
    let mut unique = columns.clone();
    unique.dedup();
    assert_eq!(unique.len(), 3, "markers on different keys: {columns:?}");
}

#[test]
fn an_xml_start_tag_missing_its_bracket_is_reported_on_its_own_line() {
    let source = "<listen>1</listen>\n<insecure_plaintextfalse</insecure_plaintext>\n<pool>\n  <min_idle>4</min_idle>\n</pool>\n";
    let error = Language::XML.parse(source).unwrap_err();
    assert_eq!(error.line, Some(2), "{error}");
}

#[test]
fn hcl_syntax_errors_read_as_a_sentence() {
    let error = Language::HCL
        .parse("listen = 1\nhealth_check : true\n")
        .unwrap_err();
    assert!(
        !error.message.contains("-->") && !error.message.contains('|'),
        "{}",
        error.message
    );
    assert_eq!(error.line, Some(2));
}
