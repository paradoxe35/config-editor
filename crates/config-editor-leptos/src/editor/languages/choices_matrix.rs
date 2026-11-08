//! The options of an object's `oneOf`/`anyOf`, named in every language
//! against the schema-help fixture's `dns` (a Cloudflare option through a
//! `$ref` that requires `cloudflare`, a Route 53 one that requires
//! `route53`, and two that require `provider` with a `const` each): hover
//! on the field names the option the value is in (or lists them all), and
//! key suggestions carry their option's title, the matched option's keys
//! first. Each test lists every failing case at once.
//!
//! `|` marks the caret.

use super::super::completion::{ChoiceHover, ValueDoc, completions, key_hover};
use super::super::diagnostics::{SchemaBundle, build_schema};
use super::Language;

fn help() -> SchemaBundle {
    build_schema(include_str!(
        "../../../tests/fixtures/schema-help.schema.json"
    ))
    .unwrap()
}

fn choice_hover(language: Language, source: &str) -> Option<ChoiceHover> {
    let at = source.find("dns").expect("a dns key");
    let line = source[..at].matches('\n').count();
    let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
    let column = source[line_start..at].chars().count() + 1;
    key_hover(source, line, column, Some(&help()), language.behavior())
        .and_then(|hover| hover.choice)
}

fn matched(title: &str, description: &str) -> Option<ChoiceHover> {
    Some(ChoiceHover::Matched(ValueDoc {
        title: Some(title.into()),
        description: Some(description.into()),
    }))
}

#[test]
fn hover_names_the_option_the_value_is_in() {
    let cloudflare = matched("Cloudflare", "Records in a Cloudflare zone.");
    let file = matched("Static file", "Records read from a file.");
    let none = Some(ChoiceHover::OneOf(vec![
        "Cloudflare".into(),
        "Route 53".into(),
        "Manual".into(),
        "Static file".into(),
    ]));
    let cases: &[(Language, &str, &Option<ChoiceHover>)] = &[
        // By its required key (through a `$ref`).
        (
            Language::YAML,
            "dns:\n  cloudflare:\n    zone: z\n",
            &cloudflare,
        ),
        (
            Language::JSON,
            "{\n  \"dns\": {\n    \"cloudflare\": {\"zone\": \"z\"}\n  }\n}\n",
            &cloudflare,
        ),
        (
            Language::TOML,
            "[dns.cloudflare]\nzone = \"z\"\n",
            &cloudflare,
        ),
        (
            Language::HCL,
            "dns {\n  cloudflare {\n    zone = \"z\"\n  }\n}\n",
            &cloudflare,
        ),
        (
            Language::XML,
            "<dns>\n  <cloudflare><zone>z</zone></cloudflare>\n</dns>\n",
            &cloudflare,
        ),
        // Two options require `provider`: its const decides.
        (Language::YAML, "dns:\n  provider: static\n", &file),
        (
            Language::JSON,
            "{\n  \"dns\": {\n    \"provider\": \"static\"\n  }\n}\n",
            &file,
        ),
        (Language::TOML, "[dns]\nprovider = \"static\"\n", &file),
        (Language::HCL, "dns {\n  provider = \"static\"\n}\n", &file),
        (
            Language::XML,
            "<dns>\n  <provider>static</provider>\n</dns>\n",
            &file,
        ),
        // Nothing decides yet: every option, by title.
        (Language::YAML, "dns:\n  ttl: 60\n", &none),
        (
            Language::JSON,
            "{\n  \"dns\": {\n    \"ttl\": 60\n  }\n}\n",
            &none,
        ),
        (Language::TOML, "[dns]\nttl = 60\n", &none),
        (Language::HCL, "dns {\n  ttl = 60\n}\n", &none),
        (Language::XML, "<dns>\n  <ttl>60</ttl>\n</dns>\n", &none),
        (Language::YAML, "dns:\n  provider: other\n", &none),
    ];
    let mut failures = Vec::new();
    for &(language, source, expected) in cases {
        let found = choice_hover(language, source);
        if &found != expected {
            failures.push(format!("{language:?} {source:?}: {found:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The keys offered at the caret (as after Ctrl+Space), with their details.
fn offered(language: Language, marked: &str) -> Vec<(String, String)> {
    let at = marked.find('|').expect("a | caret marker");
    let source = marked.replacen('|', "", 1);
    completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&help()),
        true,
        language.behavior(),
    )
    .into_iter()
    .map(|option| (option.label, option.detail))
    .collect()
}

/// A key offered, its detail, and the keys the list starts with.
type Expected = (&'static str, &'static str, &'static [&'static str]);

#[test]
fn key_suggestions_carry_their_option_and_list_the_matched_one_first() {
    let cloudflare = (
        "ttl",
        "Cloudflare — Seconds records are cached.",
        &["ttl"][..],
    );
    let file = ("path", "Static file", &["path"][..]);
    let cases: &[(Language, &str, Expected)] = &[
        (
            Language::YAML,
            "dns:\n  cloudflare:\n    zone: z\n  |\n",
            cloudflare,
        ),
        (
            Language::JSON,
            "{\n  \"dns\": {\n    \"cloudflare\": {\"zone\": \"z\"},\n    |\n  }\n}\n",
            cloudflare,
        ),
        (
            Language::HCL,
            "dns {\n  cloudflare {\n    zone = \"z\"\n  }\n  |\n}\n",
            cloudflare,
        ),
        (
            Language::XML,
            "<dns>\n  <cloudflare><zone>z</zone></cloudflare>\n  <|\n</dns>\n",
            cloudflare,
        ),
        (
            Language::TOML,
            "[dns]\n|\n\n[dns.cloudflare]\nzone = \"z\"\n",
            cloudflare,
        ),
        (Language::YAML, "dns:\n  provider: static\n  |\n", file),
        (
            Language::JSON,
            "{\n  \"dns\": {\n    |\n    \"provider\": \"static\"\n  }\n}\n",
            file,
        ),
        (Language::TOML, "[dns]\nprovider = \"static\"\n|\n", file),
        (
            Language::HCL,
            "dns {\n  provider = \"static\"\n  |\n}\n",
            file,
        ),
        (
            Language::XML,
            "<dns>\n  <provider>static</provider>\n  <|\n</dns>\n",
            file,
        ),
    ];
    let mut failures = Vec::new();
    for &(language, marked, (key, detail, first)) in cases {
        let options = offered(language, marked);
        let found = options.iter().find(|(label, _)| label == key);
        let leading: Vec<&str> = options
            .iter()
            .take(first.len())
            .map(|(label, _)| label.as_str())
            .collect();
        if found.map(|(_, found)| found.as_str()) != Some(detail) || leading != first {
            failures.push(format!("{language:?} {marked:?}: {options:?}"));
        }
    }
    // Before anything decides, each key names its option.
    let options = offered(Language::YAML, "dns:\n  |\n");
    for (key, detail) in [("route53", "Route 53"), ("records", "Manual")] {
        if !options
            .iter()
            .any(|(label, found)| label == key && found == detail)
        {
            failures.push(format!("undecided {key}: {options:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
