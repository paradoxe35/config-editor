//! Lists: one-item lists in every language, and completion inside list items.

use super::{accept, assert_parses, labels};
use crate::editor::languages::Language;

#[test]
fn an_xml_array_key_writes_one_item_that_validates_as_a_one_item_array() {
    let marked =
        "<web>\n  <letsencrypt>\n    <email>a@b.c</email>\n    <dn|\n  </letsencrypt>\n</web>\n";
    let xml = accept(Language::XML, marked, "dns");
    assert!(xml.contains("    <dns>|</dns>\n"), "{xml}");
    // The element is one item: the item's keys are offered inside it.
    assert!(labels(Language::XML, &xml).contains(&"cloudflare".to_string()));
    let done = accept(Language::XML, &xml, "cloudflare").replace('|', "x");
    let schema = crate::Schema::compile(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap();
    assert_eq!(schema.validate(Language::XML, &done), [], "{done}");
    // A second item makes a two-item array.
    let two = done.replace("</dns>", "</dns>\n    <dns><domains>b.example</domains><cloudflare><token>y</token></cloudflare></dns>");
    assert_eq!(schema.validate(Language::XML, &two), [], "{two}");
}

#[test]
fn an_xml_client_secret_list_accepts_one_secret() {
    let marked = "<clients>\n  <bob>\n    <sec|\n  </bob>\n</clients>\n";
    let xml = accept(Language::XML, marked, "secret");
    assert!(xml.contains("    <secret>|</secret>\n"), "{xml}");
    let done = xml.replace('|', "s3cret");
    let schema = crate::Schema::compile(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap();
    assert_eq!(schema.validate(Language::XML, &done), [], "{done}");
}

#[test]
fn a_single_hcl_block_for_a_list_is_a_one_item_list() {
    let schema = crate::Schema::compile(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap();
    let one =
        "web {\n  certificates {\n    hosts = [\"a\"]\n    cert = \"c\"\n    key = \"k\"\n  }\n}\n";
    assert_eq!(schema.validate(Language::HCL, one), []);
    // Inside the block, the item's keys are offered.
    let inside = "web {\n  certificates {\n    hosts = [\"a\"]\n    |\n  }\n}\n";
    assert!(labels(Language::HCL, inside).contains(&"cert".to_string()));
    // A scalar where a list belongs is still an error.
    assert_eq!(
        schema
            .validate(Language::HCL, "self {\n  allow = \"a\"\n}\n")
            .len(),
        1
    );
}

#[test]
fn a_json_list_of_strings_starts_with_a_quoted_item() {
    let json = accept(
        Language::JSON,
        "{\n  \"clients\": {\n    \"bob\": {\n      \"sec|\n    }\n  }\n}\n",
        "secret",
    );
    assert!(
        json.contains("\"secret\": [\"|\"]") || json.contains("\"secret\": \"|\""),
        "{json}"
    );
    assert_parses(Language::JSON, &json.replace('|', "s"));
}

#[test]
fn keys_already_set_in_list_items_are_not_offered_again() {
    let hcl_multi = labels(
        Language::HCL,
        "web {\n  certificates = [\n    {\n      hosts = [\"a\"]\n      |\n    }\n  ]\n}\n",
    );
    assert!(hcl_multi.contains(&"cert".to_string()), "{hcl_multi:?}");
    assert!(!hcl_multi.contains(&"hosts".to_string()), "{hcl_multi:?}");
    let hcl_inline = labels(
        Language::HCL,
        "web {\n  certificates = [{ hosts = [\"a\"], | }]\n}\n",
    );
    assert!(!hcl_inline.contains(&"hosts".to_string()), "{hcl_inline:?}");
    let hcl_dns = labels(
        Language::HCL,
        "web {\n  letsencrypt {\n    email = \"a\"\n    dns = [\n      {\n        domains = [\"a.com\"]\n        wait_secs = 5\n        |\n      },\n    ]\n  }\n}\n",
    );
    assert!(
        !hcl_dns.contains(&"domains".to_string()) && !hcl_dns.contains(&"wait_secs".to_string()),
        "{hcl_dns:?}"
    );
    let toml = labels(
        Language::TOML,
        "[[web.letsencrypt.dns]]\ndomains = [\"x\"]\n|\n\n[web.letsencrypt.dns.cloudflare]\ntoken = \"t\"\n",
    );
    assert!(!toml.contains(&"cloudflare".to_string()), "{toml:?}");
    assert!(toml.contains(&"rfc2136".to_string()), "{toml:?}");
}

#[test]
fn yaml_indentless_sequences_complete_their_items() {
    assert_eq!(
        labels(Language::YAML, "web:\n  certificates:\n  - |"),
        ["hosts", "cert", "key", "upload"]
    );
    let inside = labels(
        Language::YAML,
        "web:\n  certificates:\n  - hosts: [a]\n    |",
    );
    assert!(
        inside.contains(&"cert".to_string()) && !inside.contains(&"hosts".to_string()),
        "{inside:?}"
    );
    // A key at the list's indentation ends it.
    assert!(
        labels(Language::YAML, "web:\n  certificates:\n  - hosts: [a]\n  |")
            .contains(&"hsts".to_string())
    );
}
