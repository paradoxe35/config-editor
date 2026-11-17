//! Keys offered and accepted: where an accepted key is written, and which keys
//! are offered.

use super::{accept, assert_parses, labels};
use crate::editor::languages::Language;

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
        "limits {\n  |\n}\n"
    );
}

#[test]
fn the_first_key_of_an_empty_json_document_opens_the_root_object() {
    let json = accept(Language::JSON, "|", "limits");
    assert_eq!(json, "{\n  \"limits\": {|}\n}\n");
    assert_parses(Language::JSON, &json);
}

#[test]
fn an_empty_xml_object_element_offers_its_children() {
    let offered = labels(Language::XML, "<web>|</web>");
    assert!(offered.contains(&"letsencrypt".to_string()), "{offered:?}");
}

#[test]
fn a_yaml_block_insert_before_existing_content_adds_no_blank_line() {
    let yaml = accept(Language::YAML, "metr|\nlisten: 1\n", "metrics");
    assert!(!yaml.contains("\n\n"), "{yaml}");
    assert!(yaml.ends_with("\nlisten: 1\n"), "{yaml}");
}

#[test]
fn hcl_keys_are_offered_after_a_sibling_in_a_multi_line_object_value() {
    let offered = labels(Language::HCL, "web = {\n  hsts = true\n  le|\n}\n");
    assert!(offered.contains(&"letsencrypt".to_string()), "{offered:?}");
    assert!(
        !labels(Language::HCL, "web = {\n  hsts = true\n  |\n}\n").contains(&"hsts".to_string())
    );
}

#[test]
fn toml_table_accepted_before_another_table_starts_its_own_lines() {
    let toml = accept(
        Language::TOML,
        "listen = 1\n|\n[tcp]\nnodelay = true\n",
        "pool",
    );
    assert_eq!(toml, "listen = 1\n[pool]\n|\n[tcp]\nnodelay = true\n");
    // The next accept lands on its own line under the header.
    let next = accept(Language::TOML, &toml, "max_idle");
    assert!(next.contains("[pool]\nmax_idle = "), "{next}");
    assert_parses(Language::TOML, &next);
}

#[test]
fn a_word_typed_in_an_empty_xml_object_element_completes_a_child() {
    let marked = "<web>certif|</web>";
    assert!(labels(Language::XML, marked).contains(&"certificates".to_string()));
    let xml = accept(Language::XML, marked, "certificates");
    assert!(xml.contains("<certificates>"), "{xml}");
    assert!(
        !xml.contains("certif<"),
        "the typed word is replaced: {xml}"
    );
    assert_parses(Language::XML, &xml);
    // A scalar element keeps its value completions.
    assert!(
        !labels(
            Language::XML,
            "<services><web><mode>bal|</mode></web></services>"
        )
        .is_empty()
    );
}

#[test]
fn keys_not_values_are_offered_on_a_new_line_after_the_last_hcl_attribute() {
    let offered = labels(Language::HCL, "listen = 7835\n|");
    assert!(offered.contains(&"limits".to_string()), "{offered:?}");
    assert!(!offered.contains(&"7835".to_string()));
}

#[test]
fn after_an_xml_end_tag_the_parents_keys_are_offered() {
    let offered = labels(Language::XML, "<pool>\n  <min_idle>4</min_idle>\n</pool>|");
    assert!(offered.contains(&"limits".to_string()), "{offered:?}");
    assert!(!offered.contains(&"max_idle".to_string()));
}

#[test]
fn a_key_after_a_value_without_a_comma_in_a_one_line_object_gets_one() {
    assert!(
        labels(Language::TOML, "pool = { min_idle = 4 | }\n").contains(&"max_idle".to_string())
    );
    let toml = accept(Language::TOML, "pool = { min_idle = 4 | }\n", "max_idle");
    assert!(
        toml.starts_with("pool = { min_idle = 4, max_idle = "),
        "{toml}"
    );
    assert_parses(Language::TOML, &toml);
    let hcl = accept(Language::HCL, "pool = { min_idle = 4 | }\n", "max_idle");
    assert!(
        hcl.starts_with("pool = { min_idle = 4, max_idle = "),
        "{hcl}"
    );
    assert_parses(Language::HCL, &hcl);
    let yaml = accept(Language::YAML, "pool: {min_idle: 4 |}\n", "max_idle");
    assert!(yaml.starts_with("pool: {min_idle: 4, max_idle: "), "{yaml}");
    assert_parses(Language::YAML, &yaml);
}

#[test]
fn a_toml_key_typed_after_a_quote_takes_the_closing_quote() {
    let toml = accept(Language::TOML, "listen = 1\n\"pub|\"", "public_address");
    assert!(!toml.contains("\"\"\""), "{toml}");
    assert!(toml.starts_with("listen = 1\npublic_address = "), "{toml}");
}

#[test]
fn hcl_leaves_out_an_existing_key_even_when_its_whole_name_is_typed() {
    let nested = labels(Language::HCL, "web {\n  http2 = true\n  http2|\n}\n");
    assert!(!nested.contains(&"http2".to_string()), "{nested:?}");
    let root = labels(Language::HCL, "listen = 1\nlisten|\n");
    assert!(!root.contains(&"listen".to_string()), "{root:?}");
    let block = labels(Language::HCL, "tcp {\n}\ntcp|\n");
    assert!(!block.contains(&"tcp".to_string()), "{block:?}");
}

#[test]
fn xml_leaves_out_children_that_come_after_the_caret() {
    let offered = labels(
        Language::XML,
        "<websocket><listen>1</listen>\n  <t|\n  <tls><cert>a</cert><key>b</key></tls></websocket>\n",
    );
    assert!(!offered.contains(&"tls".to_string()), "{offered:?}");
}

#[test]
fn a_key_typed_in_front_of_an_inline_entry_goes_before_it() {
    assert_eq!(
        accept(Language::XML, "<tcp k|nodelay=\"true\"/>", "keepalive_secs"),
        "<tcp keepalive_secs=\"30|\" nodelay=\"true\"/>"
    );
    assert_eq!(
        accept(Language::YAML, "tcp: {k|nodelay: true}\n", "keepalive_secs"),
        "tcp: {keepalive_secs: 30|, nodelay: true}\n"
    );
    assert_eq!(
        accept(
            Language::TOML,
            "tcp = {k|nodelay = true}\n",
            "keepalive_secs"
        ),
        "tcp = {keepalive_secs = 30|, nodelay = true}\n"
    );
    // A suggestion completing the key renames it.
    assert_eq!(
        accept(Language::YAML, "tcp: {node|lay: true}\n", "nodelay"),
        "tcp: {nodelay|: true}\n"
    );
}

#[test]
fn an_hcl_map_accepted_in_a_body_is_a_block_as_documents_write_it() {
    let hcl = accept(Language::HCL, "cli|\n", "clients");
    assert!(hcl.starts_with("clients {\n"), "{hcl}");
    // In an object expression it stays an attribute.
    let inline = accept(Language::HCL, "web = { le| }\n", "letsencrypt");
    assert!(inline.contains("letsencrypt = {"), "{inline}");
}
