//! `Schema::hover` gives what the card shows beyond the key: the documented
//! value under the pointer, the deprecation message and the docs link, in
//! every language alike.

use crate::{HoverDocs, HoverValueDocs, Language, Schema, TextPosition};

const SCHEMA: &str = r#"{"properties": {
    "mode": {"enum": ["fast", "safe"], "description": "Request handling",
             "x-enum-descriptions": ["Skips checks", "Checks every request"]},
    "level": {"description": "Log level", "oneOf": [
        {"const": "low", "title": "Low", "description": "Errors only"},
        {"const": "high", "title": "High"}]},
    "old": {"type": "string", "deprecated": true, "deprecationMessage": "Use mode"},
    "port": {"type": "integer", "x-docs-url": "https://example.com/port"}
}}"#;

/// One document per language, with `mode`, `level`, `old` and `port`.
fn documents() -> Vec<(Language, &'static str)> {
    vec![
        #[cfg(feature = "yaml")]
        (Language::YAML, "mode: fast\nlevel: low\nold: x\nport: 80\n"),
        #[cfg(feature = "json")]
        (
            Language::JSON,
            "{\n\"mode\": \"fast\",\n\"level\": \"low\",\n\"old\": \"x\",\n\"port\": 80\n}\n",
        ),
        #[cfg(feature = "toml")]
        (
            Language::TOML,
            "mode = \"fast\"\nlevel = \"low\"\nold = \"x\"\nport = 80\n",
        ),
    ]
}

/// The docs over the second character of the first `word` in `text`.
fn hover(language: Language, text: &str, word: &str) -> Option<HoverDocs> {
    let at = text.find(word).expect("the word is in the text") + 1;
    let line = text[..at].matches('\n').count() + 1;
    let column = at - text[..at].rfind('\n').map_or(0, |end| end + 1) + 1;
    Schema::compile(SCHEMA).expect("a schema").hover(
        language,
        text,
        TextPosition::new(line, column),
    )
}

fn value_docs(value: &str, title: Option<&str>, description: Option<&str>) -> HoverValueDocs {
    HoverValueDocs {
        value: value.to_string(),
        title: title.map(str::to_string),
        description: description.map(str::to_string),
    }
}

#[test]
fn an_enum_value_gives_its_x_enum_descriptions_line_with_its_key_docs() {
    let documents = documents();
    assert!(!documents.is_empty());
    for (language, text) in documents {
        let docs = hover(language, text, "fast").expect("docs over `fast`");
        assert_eq!(docs.key, "mode", "{language:?}");
        assert_eq!(docs.description.as_deref(), Some("Request handling"));
        assert_eq!(
            docs.value,
            Some(value_docs("fast", None, Some("Skips checks"))),
            "{language:?}"
        );
        // Over the key: the key's docs, no value.
        let docs = hover(language, text, "mode").expect("docs over `mode`");
        assert_eq!(docs.value, None, "{language:?}");
    }
}

#[test]
fn a_const_branch_value_gives_its_title_and_description() {
    for (language, text) in documents() {
        let docs = hover(language, text, "low").expect("docs over `low`");
        assert_eq!(docs.key, "level", "{language:?}");
        assert_eq!(
            docs.value,
            Some(value_docs("low", Some("Low"), Some("Errors only"))),
            "{language:?}"
        );
    }
}

#[test]
fn a_deprecated_key_gives_its_deprecation_message() {
    for (language, text) in documents() {
        let docs = hover(language, text, "old").expect("docs over `old`");
        assert!(docs.deprecated, "{language:?}");
        assert_eq!(
            docs.deprecation.as_deref(),
            Some("Use mode"),
            "{language:?}"
        );
        // Another key says nothing of it.
        let docs = hover(language, text, "port").expect("docs over `port`");
        assert_eq!(docs.deprecation, None, "{language:?}");
    }
}

#[test]
fn a_key_with_x_docs_url_gives_its_docs_link() {
    for (language, text) in documents() {
        let docs = hover(language, text, "port").expect("docs over `port`");
        assert_eq!(
            docs.docs_url.as_deref(),
            Some("https://example.com/port"),
            "{language:?}"
        );
        let docs = hover(language, text, "mode").expect("docs over `mode`");
        assert_eq!(docs.docs_url, None, "{language:?}");
    }
}

#[test]
fn a_value_the_schema_does_not_document_gives_its_key_docs_alone() {
    for (language, text) in documents() {
        // `80` of `port`: an integer with no docs of its own gives its
        // key's, as the card shows them.
        let docs = hover(language, text, "80").expect("docs over `80`");
        assert_eq!(docs.key, "port", "{language:?}");
        assert_eq!(docs.value, None, "{language:?}");
    }
}
