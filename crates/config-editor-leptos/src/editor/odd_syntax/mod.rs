//! A deliberately unusual custom language, implemented on the trait as a
//! host would: `;` comments, `:=` between a key and its value, `@` before
//! each list item written on lines of its own, `( a | b )` lists and
//! `< k := v | j := w >` maps on one line, `yes`/`no`/`nil`, and no markup.
//! Saves, conversions, list moves and locks through it must work from what
//! the trait says, never from the built-in languages' syntax.

mod read;
mod save_tests;
mod write;

use serde_json::Value;

use super::language::{ConfigLanguage, LanguageConfig, LanguageInfo, ParseError, Span};

pub(super) struct Odd;

pub(super) static ODD: Odd = Odd;

impl ConfigLanguage for Odd {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "Odd",
            icon: "O",
            extensions: &["odd"],
        }
    }

    fn config(&self) -> LanguageConfig {
        LanguageConfig {
            line_comments: &[";"],
            block_comment: None,
            brackets: &[('(', ')'), ('<', '>')],
            separators: &[":="],
            quotes: &["\""],
            literal_quotes: &[],
            map_brackets: Some(('<', '>')),
            list_brackets: Some(('(', ')')),
            item_separator: Some('|'),
            item_marker: Some("@"),
            written_separator: Some(" := "),
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        read::read(source).map(|read| read.value)
    }

    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        write::document(value)
    }

    fn format_value(&self, value: &Value) -> String {
        write::inline(value)
    }

    fn indent_is_structure(&self) -> bool {
        true
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        read::read(source).ok()?.keys.remove(pointer)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        read::read(source).ok()?.values.remove(pointer)
    }
}
