//! When suggestions open, and the places where nothing is offered.

use super::{burrow, labels};
use crate::editor::completion::completions;
use crate::editor::languages::Language;

#[test]
fn typing_a_number_does_not_open_value_suggestions_unless_asked() {
    let auto = |language: Language, marked: &str| {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        completions(
            &source,
            source[..at].encode_utf16().count(),
            false,
            Some(&burrow()),
            true,
            language.behavior(),
        )
        .len()
    };
    assert_eq!(auto(Language::YAML, "limits:\n  max_pending: 5|\n"), 0);
    assert_eq!(auto(Language::TOML, "[limits]\nmax_pending = 5|\n"), 0);
    // Asked for, they are listed.
    assert!(!labels(Language::YAML, "limits:\n  max_pending: 5|\n").is_empty());
}

#[test]
fn nothing_is_suggested_in_comments_and_toml_gets_no_null() {
    assert!(labels(Language::YAML, "# po|\n").is_empty());
    assert!(labels(Language::TOML, "# po|\n").is_empty());
    assert!(labels(Language::HCL, "# po|\n").is_empty());
    assert!(!labels(Language::TOML, "public_address = |\n").contains(&"null".to_string()));
    assert!(labels(Language::YAML, "public_address: |\n").contains(&"null".to_string()));
}

#[test]
fn nothing_is_offered_after_the_json_root_closes() {
    assert!(labels(Language::JSON, "{\n  \"listen\": 1\n}|").is_empty());
    assert!(labels(Language::JSON, "{\n  \"listen\": 1\n}\nli|").is_empty());
    assert!(!labels(Language::JSON, "{\n  |\n}").is_empty());
}

#[test]
fn a_space_after_a_complete_word_offers_nothing() {
    let auto = |language: Language, marked: &str| {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        completions(
            &source,
            source[..at].encode_utf16().count(),
            false,
            Some(&burrow()),
            true,
            language.behavior(),
        )
        .len()
    };
    assert_eq!(auto(Language::HCL, "insecure_plaintext = false |"), 0);
    assert_eq!(auto(Language::HCL, "listen = 7835 |"), 0);
    assert_eq!(auto(Language::TOML, "insecure_plaintext = f |"), 0);
    assert_eq!(auto(Language::YAML, "insecure_plaintext: false |"), 0);
    assert_eq!(auto(Language::YAML, "pu |"), 0);
    // Right after the separator, values are still offered.
    assert!(
        auto(Language::YAML, "insecure_plaintext: |") > 0
            || auto(Language::HCL, "insecure_plaintext = |") > 0
    );
}

#[test]
fn an_hcl_block_label_gets_no_property_suggestions() {
    let offered = labels(Language::HCL, "services \"|\" {\n}\n");
    assert!(offered.is_empty(), "{offered:?}");
    let offered = labels(Language::HCL, "clients \"a\" \"|\" {\n}\n");
    assert!(offered.is_empty(), "{offered:?}");
    // A string value is still a value.
    assert!(!labels(Language::HCL, "public_address = \"|\"\n").contains(&"listen".to_string()));
}
