//! JSON text-layer behavior.
//!
//! Decisions: duplicate keys are a parse error, as in YAML; JSON has no
//! comments, so `Ctrl+/` does nothing.
//!
//! Syntax errors come from `serde_json`. Structure (key paths, key and value
//! spans, the container under the caret) comes from a token scan that tolerates
//! invalid input, the normal state while a key is being typed.

mod tokens;
mod scan;
mod parse;
mod suggestions;
mod editing;

use tokens::*;
use scan::*;
use parse::*;
use suggestions::*;
use editing::*;

use super::super::commands::indent_unit;
use super::super::completion::{
    array_branch, object_branch, placeholder, schema_const, schema_default,
};
use super::super::highlight::{self, TokenKind, Tokens};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{LanguageConfig, CompletionContext, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError, SchemaPathSegment,
    inline_block, unclosed_brackets,
};
use serde_json::{Map, Value};
use std::collections::HashSet;

pub(super) struct JsonBehavior;


impl ConfigLanguage for JsonBehavior {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "JSON",
            icon: "J",
            extensions: &["json"],
        }
    }

    fn config(&self) -> LanguageConfig {
        LanguageConfig {
            line_comments: &[],
            brackets: &[('{', '}'), ('[', ']')],
            separators: &[":"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        json_parse(source)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        json_key_span(source, pointer).map(Span::from)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        json_value_span(source, pointer).map(Span::from)
    }

    fn first_span(&self, source: &str) -> Option<Span> {
        first_line_span(source).map(Span::from)
    }

    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        completion_context(source, cursor)
    }

    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        _path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        existing_keys(source, cursor_byte)
    }

    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        property_insert(key, property, root, context.base_indent, context.unit)
    }

    fn format_value(&self, value: &Value) -> String {
        json_text(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column).map(|(_, key, start_column)| (key, start_column))
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        key_at_column(line, column).map_or(line.len(), |(token, _, _)| {
            let rest = &line[token.end..];
            // Skip whitespace to the colon, then step past it.
            token.end + (rest.len() - rest.trim_start().len()) + 1
        })
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = (open_string_quote(source, cursor), false);
        Lexical { quote, in_comment }
    }

    fn entry_started(&self, source: &str, cursor: usize) -> bool {
        completion_context(source, cursor).explicit_entry_start
    }

    /// Pretty JSON, indented like `previous` when it is given.
    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        use serde::Serialize;
        let unit = previous.map_or_else(|| "  ".to_string(), indent_unit);
        let mut out = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(unit.as_bytes());
        let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
        value
            .serialize(&mut serializer)
            .map_err(|error| error.to_string())?;
        let mut text = String::from_utf8(out).map_err(|error| error.to_string())?;
        text.push('\n');
        Ok(text)
    }

}


// Conversion




#[cfg(test)]
mod tests;
