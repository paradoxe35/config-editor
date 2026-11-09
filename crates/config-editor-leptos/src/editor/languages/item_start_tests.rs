//! The hover card finds a list item by what the language says opens one
//! ([`ConfigLanguage::item_start`]), not by characters the shared code
//! knows: a language without YAML's dash gets no card on a `-`.

use super::super::completion::key_hover;
use super::super::diagnostics::build_schema;
use super::Language;
use crate::editor::language::*;
use serde_json::Value;
use std::collections::HashSet;

/// YAML in everything but where an item opens, which `opens` says.
struct Wrapped(fn(&str, usize) -> Option<usize>);

fn yaml() -> &'static dyn ConfigLanguage {
    Language::YAML.behavior()
}

impl ConfigLanguage for Wrapped {
    fn info(&self) -> LanguageInfo {
        yaml().info()
    }
    fn config(&self) -> LanguageConfig {
        yaml().config()
    }
    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        yaml().parse(source)
    }
    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        yaml().serialize(value, previous)
    }
    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        yaml().completion_context(source, cursor)
    }
    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        yaml().existing_keys(source, cursor_byte, path)
    }
    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        yaml().hover_key(line, column)
    }
    fn hover_path(
        &self,
        source: &str,
        line_index: usize,
        column: usize,
    ) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        yaml().hover_path(source, line_index, column)
    }
    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        yaml().hover_cursor(line, column)
    }
    fn repetition(&self) -> Repetition {
        yaml().repetition()
    }
    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        yaml().lexical_state(source, cursor)
    }
    fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        yaml().string_body_lines(source)
    }
    fn item_start(&self, _: &str, _: usize, line: &str, column: usize) -> Option<usize> {
        (self.0)(line, column)
    }
}

#[test]
fn the_card_on_a_list_item_follows_what_the_language_says_opens_one() {
    let bundle = build_schema(
        r#"{"type": "object", "properties": {"ports": {"type": "array",
            "description": "Ports", "items": {"type": "integer", "description": "One port"}}}}"#,
    )
    .unwrap();
    let source = "ports:\n  - 80\n";
    let item = |language: &dyn ConfigLanguage| {
        key_hover(source, 1, 2, Some(&bundle), language)
            .and_then(|hover| hover.item)
            .and_then(|item| item.description)
    };
    assert_eq!(
        item(yaml()),
        Some("One port".to_string()),
        "YAML's own dash"
    );
    assert_eq!(
        item(&Wrapped(|_, _| None)),
        None,
        "a language with no item marker: the shared code must not know YAML's dash"
    );
    // A language whose items open with the character it names: here the
    // dash again, reported by the language, so the card is back.
    let dash =
        |line: &str, column: usize| (line.chars().nth(column) == Some('-')).then_some(column + 2);
    assert_eq!(item(&Wrapped(dash)), Some("One port".to_string()));
}
