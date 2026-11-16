//! Completing a key that is already written renames it and keeps its value.

use super::accept;
use crate::editor::languages::Language;

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
fn completing_a_key_that_has_a_value_renames_only_the_key() {
    assert_eq!(
        accept(Language::YAML, "tcp:\n  keep|: 99\n", "keepalive_secs"),
        "tcp:\n  keepalive_secs|: 99\n"
    );
    assert_eq!(
        accept(
            Language::JSON,
            "{\n  \"publ|\": \"my.host\"\n}",
            "public_address"
        ),
        "{\n  \"public_address\"|: \"my.host\"\n}"
    );
    assert_eq!(
        accept(Language::TOML, "[tcp]\nkeep| = 99\n", "keepalive_secs"),
        "[tcp]\nkeepalive_secs| = 99\n"
    );
    assert_eq!(
        accept(
            Language::TOML,
            "hea| = { interval_secs = 5 }\n",
            "heartbeat"
        ),
        "heartbeat| = { interval_secs = 5 }\n"
    );
    assert_eq!(
        accept(Language::HCL, "tcp {\n  keep| = 99\n}\n", "keepalive_secs"),
        "tcp {\n  keepalive_secs| = 99\n}\n"
    );
}

#[test]
fn renaming_hcl_blocks_and_quoted_keys_keeps_the_rest() {
    assert_eq!(
        accept(Language::HCL, "pol| {\n  max_idle = 1\n}\n", "pool"),
        "pool| {\n  max_idle = 1\n}\n"
    );
    assert_eq!(
        accept(
            Language::HCL,
            "servi| \"ssh\" {\n  target = 1\n}\n",
            "services"
        ),
        "services| \"ssh\" {\n  target = 1\n}\n"
    );
    assert_eq!(
        accept(Language::JSON, "{\"pub|lic\": 1}", "public_address"),
        "{\"public_address\"|: 1}"
    );
}

#[test]
fn accepting_the_key_already_typed_in_front_of_its_value_changes_nothing() {
    assert_eq!(
        accept(Language::TOML, "[tcp]\nnodelay| = true\n", "nodelay"),
        "[tcp]\nnodelay| = true\n"
    );
    assert_eq!(
        accept(Language::YAML, "tcp:\n  nodelay|: true\n", "nodelay"),
        "tcp:\n  nodelay|: true\n"
    );
    assert_eq!(accept(Language::HCL, "tcp| {\n}\n", "tcp"), "tcp| {\n}\n");
}
