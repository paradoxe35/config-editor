//! A flat key/value language (one `KEY=value` entry per line) gets schema
//! completion and hover by handing its hooks to the `LanguageConfig::flat_*`
//! methods: what a JavaScript language with only `separators` gets. Checked
//! through `Schema::suggestions` / `Schema::hover`, for `=` and `:`
//! separators.

use config_editor_leptos::language::{
    CompletionContext, ConfigLanguage, InsertContext, LanguageConfig, LanguageInfo, ParseError,
    SchemaPathSegment,
};
use config_editor_leptos::{Language, Schema, TextPosition};
use serde_json::{Map, Value};
use std::collections::HashSet;

struct Flat {
    label: &'static str,
    separator: &'static [&'static str],
}

impl ConfigLanguage for Flat {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: self.label,
            icon: "F",
            extensions: &[],
        }
    }

    fn config(&self) -> LanguageConfig {
        LanguageConfig {
            line_comments: &["#"],
            separators: self.separator,
            ..LanguageConfig::default()
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let separator = self.separator[0];
        let mut map = Map::new();
        for line in source.lines() {
            if line.trim().is_empty() || line.trim().starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once(separator).ok_or(ParseError {
                message: "expected an entry".to_string(),
                line: None,
                column: None,
            })?;
            map.insert(key.trim().into(), Value::String(value.trim().into()));
        }
        Ok(Value::Object(map))
    }

    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        self.config().flat_completion_context(source, cursor)
    }

    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        self.config().flat_existing_keys(source, cursor_byte, path)
    }

    fn property_insert(
        &self,
        key: &str,
        _property: &Value,
        _root: &Value,
        _context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        self.config().flat_property_insert(key)
    }

    fn format_value(&self, value: &Value) -> String {
        self.config().flat_format_value(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        self.config().flat_hover_key(line, column)
    }
}

static ENV: Flat = Flat {
    label: "FlatEnv",
    separator: &["="],
};
static COLON: Flat = Flat {
    label: "FlatColon",
    separator: &[":"],
};

const SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "LOG_LEVEL": {"enum": ["debug", "info", "warn"], "description": "How much to log"},
        "DEBUG": {"type": "boolean", "description": "Debug endpoints"},
        "PORT": {"type": "string", "description": "Port to listen on"},
        "PATH_PREFIX": {"type": "string", "const": "/api", "description": "Served under"}
    }
}"#;

/// `marked` without its `|`, and the UTF-16 offset of the `|`.
fn unmark(marked: &str) -> (String, usize) {
    let at = marked.find('|').expect("a | caret marker");
    (
        marked.replacen('|', "", 1),
        marked[..at].encode_utf16().count(),
    )
}

fn offered(language: Language, marked: &str) -> Vec<String> {
    let (source, cursor) = unmark(marked);
    let mut labels: Vec<String> = Schema::compile(SCHEMA)
        .unwrap()
        .suggestions(language, &source, cursor)
        .iter()
        .map(|suggestion| suggestion.label().to_string())
        .collect();
    labels.sort();
    labels
}

fn accept(language: Language, marked: &str, label: &str) -> String {
    let (source, cursor) = unmark(marked);
    let suggestions = Schema::compile(SCHEMA)
        .unwrap()
        .suggestions(language, &source, cursor);
    let suggestion = suggestions
        .iter()
        .find(|suggestion| suggestion.label() == label)
        .unwrap_or_else(|| panic!("{label} is not offered in {marked:?}"));
    let (text, selection) = suggestion.accept(&source);
    let byte = utf16_byte(&text, selection.end);
    format!("{}|{}", &text[..byte], &text[byte..])
}

fn utf16_byte(text: &str, units: usize) -> usize {
    text.char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
        .find(|index| text[..*index].encode_utf16().count() == units)
        .unwrap()
}

#[test]
fn flat_languages_offer_top_level_keys_and_values() {
    let env = Language::new(&ENV);
    let colon = Language::new(&COLON);
    // (language, marked document, offered)
    let cases: &[(Language, &str, &[&str])] = &[
        (env, "PORT=80\nP|", &["PATH_PREFIX"]),
        (env, "# PORT\nP|", &["PATH_PREFIX", "PORT"]),
        (env, "LOG|", &["LOG_LEVEL"]),
        (env, "LOG_LEVEL=|", &["debug", "info", "warn"]),
        (env, "LOG_LEVEL=d|", &["debug"]),
        (env, "DEBUG=|", &["false", "true"]),
        (env, "# P|", &[]),
        (colon, "PORT: 80\nP|", &["PATH_PREFIX"]),
        (colon, "DEBUG:|", &["false", "true"]),
        (colon, "LOG_LEVEL: i|", &["info"]),
    ];
    let mut failures = Vec::new();
    for (language, marked, expected) in cases {
        let got = offered(*language, marked);
        if got != *expected {
            failures.push(format!(
                "{} {marked:?}: {got:?}, expected {expected:?}",
                language.label()
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn accepting_writes_the_key_and_separator_and_values_bare() {
    let env = Language::new(&ENV);
    let colon = Language::new(&COLON);
    assert_eq!(
        accept(env, "PORT=80\nLOG|", "LOG_LEVEL"),
        "PORT=80\nLOG_LEVEL=|"
    );
    assert_eq!(accept(colon, "LOG|", "LOG_LEVEL"), "LOG_LEVEL: |");
    assert_eq!(accept(env, "LOG_LEVEL=i|", "info"), "LOG_LEVEL=info|");
    assert_eq!(accept(colon, "DEBUG:|", "true"), "DEBUG: true|");
    // A `const` is offered, written bare.
    assert_eq!(accept(env, "PATH_PREFIX=|", "/api"), "PATH_PREFIX=/api|");
}

#[test]
fn hovering_a_flat_key_shows_its_docs() {
    let schema = Schema::compile(SCHEMA).unwrap();
    for language in [Language::new(&ENV), Language::new(&COLON)] {
        let separator = if language.label() == "FlatEnv" {
            "="
        } else {
            ": "
        };
        let source = format!("# Logging\nLOG_LEVEL{separator}info\n");
        let docs = schema
            .hover(language, &source, TextPosition::new(2, 3))
            .unwrap_or_else(|| panic!("{}: docs over LOG_LEVEL", language.label()));
        assert_eq!(docs.key, "LOG_LEVEL");
        assert_eq!(docs.description.as_deref(), Some("How much to log"));
        assert_eq!(
            schema.hover(language, &source, TextPosition::new(1, 3)),
            None
        );
    }
}
