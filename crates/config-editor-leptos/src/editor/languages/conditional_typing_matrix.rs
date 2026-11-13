//! The keys offered while a new entry is typed follow the branch the
//! object's `if` takes, also where the text around the caret does not
//! parse yet: a JSON entry ending in a comma before the one being typed
//! (`"kind": "a",` then a new line) reads as the object without it. In
//! every language.
//!
//! `|` marks the caret.

use super::super::completion::completions;
use super::super::diagnostics::build_schema;
use super::Language;

const SCHEMA: &str = r#"{"type":"object","properties":{"kind":{"enum":["a","b"]}},
    "if":{"properties":{"kind":{"const":"a"}},"required":["kind"]},
    "then":{"properties":{"alpha":{"type":"string"}}},
    "else":{"properties":{"beta":{"type":"string"}}}}"#;

#[test]
fn the_branch_an_if_takes_is_offered_while_a_key_is_typed() {
    let schema = build_schema(SCHEMA).unwrap();
    let cases = [
        (Language::YAML, "kind: a\n|\n"),
        (Language::JSON, "{\n  \"kind\": \"a\",\n  |\n}\n"),
        (Language::JSON, "{\n  \"kind\": \"a\",\n  \"|\n}\n"),
        (Language::JSON, "{\n  \"kind\": \"a\",\n  \"|\"\n}\n"),
        (Language::TOML, "kind = \"a\"\n|\n"),
        (Language::HCL, "kind = \"a\"\n|\n"),
        (Language::XML, "<kind>a</kind>\n|\n"),
    ];
    let mut failures = Vec::new();
    for (language, marked) in cases {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let mut offered: Vec<String> = completions(
            &source,
            source[..at].encode_utf16().count(),
            true,
            Some(&schema),
            true,
            language.behavior(),
        )
        .into_iter()
        .map(|option| option.label)
        .collect();
        offered.sort();
        if offered != ["alpha"] {
            failures.push(format!("{language} {marked:?}: {offered:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
