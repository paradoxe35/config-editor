//! Values offered and accepted: their order, quoting and selection, and how
//! they replace what is already written.

use super::{accept, assert_parses, burrow, labels};
use crate::editor::completion::completions;
use crate::editor::languages::Language;

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
fn an_hcl_value_after_a_space_gets_no_second_space() {
    let hcl = accept(
        Language::HCL,
        "services {\n  web {\n    mode = |\n  }\n}\n",
        "balance",
    );
    assert!(hcl.contains("mode = \"balance\""), "{hcl}");
}

#[test]
fn enum_values_are_offered_in_schema_order() {
    let offered = labels(Language::YAML, "services:\n  web:\n    mode: |\n");
    assert_eq!(
        offered[..4],
        ["failover", "balance", "least_connections", "ip_hash"]
    );
}

/// The text left selected after accepting `label` at the caret.
fn selected_after_accept(language: Language, marked: &str, label: &str) -> String {
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let options = completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&burrow()),
        true,
        language.behavior(),
    );
    let option = options.iter().find(|option| option.label == label).unwrap();
    let (updated, anchor, caret) = crate::editor::completion::apply_completion(&source, option);
    updated[crate::editor::text::TextRange::new(anchor, caret)].to_string()
}

#[test]
fn an_inserted_default_is_selected_so_typing_replaces_it() {
    for (language, marked) in [
        (Language::YAML, "limits:\n  max_p|\n"),
        (Language::TOML, "[limits]\nmax_p|\n"),
        (Language::HCL, "limits {\n  max_p|\n}\n"),
        (Language::JSON, "{\n  \"limits\": {\n    \"max_p|\n  }\n}\n"),
        (Language::XML, "<limits>\n  <max_p|\n</limits>\n"),
    ] {
        assert_eq!(
            selected_after_accept(language, marked, "max_pending"),
            "512",
            "{language}"
        );
    }
    // A quoted default is selected inside its quotes.
    for (language, marked) in [
        (Language::TOML, "[websocket]\npa|\n"),
        (Language::JSON, "{\n  \"websocket\": {\n    \"pa|\n  }\n}\n"),
        (Language::HCL, "websocket {\n  pa|\n}\n"),
        (Language::YAML, "websocket:\n  pa|\n"),
        (Language::XML, "<websocket>\n  <pa|\n</websocket>\n"),
    ] {
        assert_eq!(
            selected_after_accept(language, marked, "path"),
            "/tunnel",
            "{language}"
        );
    }
    // A key without a default leaves nothing selected.
    assert_eq!(
        selected_after_accept(Language::YAML, "web:\n  hst|\n", "hsts"),
        ""
    );
}

#[test]
fn an_xml_value_accepted_mid_word_replaces_the_whole_value() {
    assert_eq!(
        accept(
            Language::XML,
            "<services><web><mode>fail|over</mode></web></services>",
            "failover"
        ),
        "<services><web><mode>failover|</mode></web></services>"
    );
}

#[test]
fn accepting_a_value_inside_existing_quotes_keeps_the_rest_of_the_line() {
    // TOML inline table.
    let toml = accept(
        Language::TOML,
        "[services.s]\ntarget = 1\nweb = { hosts = [\"a\"], tls = \"|\", hsts = true }\n",
        "auto",
    );
    assert!(toml.contains("tls = \"auto\"|, hsts = true }"), "{toml}");
    // TOML with a trailing comment.
    let toml = accept(
        Language::TOML,
        "[services.s]\ntarget = 1\nmode = \"|\" # pick one\n",
        "failover",
    );
    assert!(toml.contains("mode = \"failover\"| # pick one"), "{toml}");
    // HCL inline object.
    let hcl = accept(
        Language::HCL,
        "services {\n  s {\n    target = 1\n    web = { hosts = [\"a\"], tls = \"|\", hsts = true }\n  }\n}\n",
        "auto",
    );
    assert!(hcl.contains("tls = \"auto\"|, hsts = true }"), "{hcl}");
    // YAML flow map.
    let yaml = accept(
        Language::YAML,
        "services: {s: {target: 1, mode: \"|\", bandwidth: 1m}}\n",
        "failover",
    );
    assert!(yaml.contains(", bandwidth: 1m}}"), "{yaml}");
    assert_parses(Language::YAML, &yaml);
    // The value after an accepted key, in nested inline tables.
    let toml = accept(
        Language::TOML,
        "services = { s = { target = 1, mode = \"|\" } }\n",
        "failover",
    );
    assert!(toml.contains("mode = \"failover\"| } }"), "{toml}");
}

#[test]
fn an_xml_attribute_value_accepted_inside_its_quotes_leaves_the_caret_after_them() {
    let xml = accept(
        Language::XML,
        "<burrow config-root=\"true\">\n  <services>\n    <s target=\"1\" mode=\"|\"/>\n  </services>\n</burrow>\n",
        "balance",
    );
    assert!(xml.contains("mode=\"balance\"|/>"), "{xml}");
}
