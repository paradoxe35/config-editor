//! Suggestions and hover read the document wherever a schema is decided
//! by it, whatever the field holding that schema is named: a section
//! called `default`, `enum`, `const` or `examples` (common in config files)
//! narrows its `oneOf` as any other does.

use crate::editor::languages::Language;
use crate::editor::schema::Schema;

fn schema_under(name: &str) -> Schema {
    Schema::compile(
        &serde_json::json!({
            "type": "object",
            "properties": {
                name: {"oneOf": [
                    {"type": "object", "title": "Kind A", "required": ["kind"], "properties": {
                        "kind": {"const": "a"}, "onlyA": {"type": "string"}}},
                    {"type": "object", "title": "Kind B", "required": ["kind"], "properties": {
                        "kind": {"const": "b"}, "onlyB": {"type": "string"}}}
                ]}
            }
        })
        .to_string(),
    )
    .expect("a schema")
}

#[test]
fn a_section_named_like_a_keyword_is_narrowed_by_the_document() {
    for name in ["default", "enum", "const", "examples", "example", "plain"] {
        let schema = schema_under(name);
        let text = format!("{name}:\n  kind: a\n  \n");
        let cursor = text.len() - 1;
        let labels: Vec<String> = schema
            .suggestions(Language::YAML, &text, cursor)
            .iter()
            .map(|suggestion| suggestion.label().to_string())
            .collect();
        assert!(labels.contains(&"onlyA".to_string()), "{name}: {labels:?}");
        assert!(!labels.contains(&"onlyB".to_string()), "{name}: {labels:?}");
        let hover = crate::editor::completion::key_hover(
            &text,
            1,
            3,
            Some(&schema.bundle),
            Language::YAML.behavior(),
        )
        .expect("hover on kind");
        let choice = format!("{:?}", hover.choice);
        assert!(choice.contains("Kind A"), "{name}: {choice}");
        assert!(!choice.contains("Kind B"), "{name}: {choice}");
    }
}
