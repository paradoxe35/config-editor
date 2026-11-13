//! Hover and completion inside `oneOf`/`anyOf` options and overlapping
//! `patternProperties`: the matched option's docs and keys alone, and every
//! pattern naming a key.

use crate::editor::Language;
use crate::editor::diagnostics::Schema;
use crate::editor::paths::TextPosition;
use serde_json::json;

fn hover(schema: &Schema, text: &str, line: usize, column: usize) -> crate::editor::HoverDocs {
    schema
        .hover(Language::YAML, text, TextPosition { line, column })
        .unwrap_or_else(|| panic!("a hover at {line}:{column} of {text:?}"))
}

/// S.combinators-4: the field's hover leads with no option's docs; the
/// matched option is named once, in its own row.
#[test]
fn hover_takes_no_title_from_an_option_the_object_is_not_in() {
    let schema = Schema::compile(
        &json!({"properties": {"dns": {"oneOf": [
            {"title": "Manual", "description": "manual docs", "required": ["provider"],
             "properties": {"provider": {"const": "manual"}}},
            {"title": "Static", "description": "static docs", "required": ["provider"],
             "properties": {"provider": {"const": "static"}}}
        ]}}})
        .to_string(),
    )
    .unwrap();
    for provider in ["static", "manual"] {
        let docs = hover(&schema, &format!("dns:\n  provider: {provider}\n"), 1, 1);
        assert_eq!(
            (docs.title.as_deref(), docs.description.as_deref()),
            (None, None),
            "{provider}"
        );
    }
    let fielded = Schema::compile(
        &json!({"properties": {"dns": {"title": "DNS", "oneOf": [
            {"title": "A", "required": ["a"], "properties": {"a": {}}},
            {"title": "B", "required": ["b"], "properties": {"b": {}}}
        ]}}})
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        hover(&fielded, "dns:\n  b: 1\n", 1, 1).title.as_deref(),
        Some("DNS")
    );
}

/// S.combinators-5: once an option is matched, another option's keys are
/// not offered; until then, every option's are.
#[test]
fn a_matched_option_offers_its_own_keys_alone() {
    let schema = Schema::compile(
        &json!({"properties": {"dns": {"oneOf": [
            {"title": "A", "required": ["cloudflare"], "additionalProperties": false,
             "properties": {"cloudflare": {"type": "object"}, "ttl": {"type": "integer"}}},
            {"title": "B", "required": ["route53"], "additionalProperties": false,
             "properties": {"route53": {"type": "object"}, "zone": {"type": "string"}}}
        ]}}})
        .to_string(),
    )
    .unwrap();
    let offered = |text: &str| {
        let caret = text.find('|').unwrap();
        let text = text.replace('|', "");
        let mut labels: Vec<String> = schema
            .suggestions(Language::YAML, &text, caret)
            .iter()
            .map(|suggestion| suggestion.label().to_string())
            .collect();
        labels.sort();
        labels
    };
    assert_eq!(offered("dns:\n  cloudflare: {}\n  |\n"), ["ttl"]);
    assert_eq!(
        offered("dns:\n  |\n"),
        ["cloudflare", "route53", "ttl", "zone"]
    );
}

/// S.patterns-4: two patterns naming one key both show in its hover, as
/// validation applies both.
#[test]
fn every_pattern_naming_a_key_shows_in_its_hover() {
    let schema = Schema::compile(
        &json!({"patternProperties": {
            "^k": {"type": "string", "title": "K pattern"},
            "y$": {"type": "integer", "title": "Y pattern"}
        }})
        .to_string(),
    )
    .unwrap();
    let docs = hover(&schema, "key: true\n", 1, 1);
    assert_eq!(docs.type_label, "string | integer", "in schema order");
    assert_eq!(docs.title.as_deref(), Some("K pattern, Y pattern"));
    assert_eq!(schema.validate(Language::YAML, "key: true\n").len(), 2);
}
