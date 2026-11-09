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
// The fold ranges (the `folding` feature).
#[cfg(feature = "folding")]
mod folding;

use tokens::*;
use scan::*;
use parse::*;
use suggestions::*;
use editing::*;

use super::super::commands::indent_unit;
use super::super::completion::{
    array_branch, object_branch, fill_value, placeholder, schema_const, schema_default,
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

    #[cfg(feature = "folding")]
    fn fold_ranges(&self, source: &str) -> Vec<super::super::language::FoldRange> {
        folding::json_fold_ranges(source, &self.config())
    }

    fn config(&self) -> LanguageConfig {
        LanguageConfig {
            line_comments: &[],
            block_comment: None,
            brackets: &[('{', '}'), ('[', ']')],
            separators: &[":"],
            quotes: &["\""],
            literal_quotes: &[],
            map_brackets: Some(('{', '}')),
            list_brackets: Some(('[', ']')),
            item_separator: Some(','),
            item_marker: None,
            written_separator: Some(": "),
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        json_parse(source).map(super::super::codec::non_finite::escape_strings)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        super::super::byte_order_mark::span(source, |text| json_key_span(text, pointer).map(Span::from))
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        super::super::byte_order_mark::span(source, |text| json_value_span(text, pointer).map(Span::from))
    }

    fn first_span(&self, source: &str) -> Option<Span> {
        super::super::byte_order_mark::span(source, |text| first_line_span(text).map(Span::from))
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

    /// A character however it is escaped (`\u00e9` for `é`, `\/` for `/`).
    fn comparable_line<'a>(&self, code: &'a str) -> std::borrow::Cow<'a, str> {
        super::escapes::without_unicode_escapes(code)
    }

    fn format_value(&self, value: &Value) -> String {
        json_text(value)
    }

    /// Without a space after any `:` or `,`, the host writes compact JSON.
    /// Always a string (`"a b"`).
    fn written_key(&self, _source: &str, _pointer: &str, key: &str) -> Option<String> {
        serde_json::to_string(key).ok()
    }

    fn format_value_like(&self, value: &Value, written: &str) -> String {
        let compact = !written.contains(": ") && !written.contains(", ") && (written.contains(':') || written.contains(','));
        let plain = super::super::codec::non_finite::unescape_strings(value);
        let text = if compact { serde_json::to_string(&*plain).unwrap_or_else(|_| json_text(value)) } else { json_text(value) };
        if !value.is_string() {
            return text;
        }
        // A string keeps the escapes the host wrote in the one it replaces.
        let text = super::escapes::like_written(text, written, super::escapes::Astral::SurrogatePair);
        if written.contains("\\/") { text.replace('/', "\\/") } else { text }
    }

    fn scalar_reads_as(&self, written: &str, value: &Value) -> bool {
        serde_json::from_str::<Value>(written.trim()).ok().as_ref() == Some(value)
    }

    fn number_in_form(&self, written: &str, value: &serde_json::Number) -> Option<String> {
        super::number_forms::checked(written, value, &super::number_forms::DECIMAL, |text, value| {
            self.scalar_reads_as(text, value)
        })
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
        super::super::codec::non_finite::unescape_strings(value)
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
