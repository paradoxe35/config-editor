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

use crate::editor::paths::{first_line_span, line_column, span_between, utf16_len};
use tokens::*;
use scan::*;
use parse::*;
use suggestions::*;
use editing::*;

use super::super::commands::indent_unit;
use super::super::completion::{Fill, InsertPlan, Planned, RequiredChild};
use super::super::highlight::{self, TokenKind, Tokens};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{Text, Typing, Hover, SaveLayout};
use super::super::language::{LanguageConfig, CompletionContext, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError, SchemaPathSegment,
    inline_block, unclosed_brackets,
};
use serde_json::Value;
use std::collections::HashSet;

pub(super) struct JsonBehavior;

/// A JSON scalar edit (see `SaveLayout::edits_one_scalar`): a
/// `"key": value` line read as an object of its own; the edit adds and
/// removes no quote, escape, bracket or comma.
const SCALAR_LINE: super::scalar_lines::LineRules = super::scalar_lines::LineRules {
    alone: |line| {
        let entry = line.trim();
        let entry = entry.strip_suffix(',').unwrap_or(entry);
        entry.starts_with('"').then(|| super::scalar_lines::Alone {
            cut: line.len() - line.trim_start().len(),
            text: format!("{{{entry}}}"),
            item: false,
        })
    },
    forbidden: &['"', '\\', ',', '[', ']', '{', '}'],
    closers: &[],
};

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
            traits: super::super::language::LanguageTraits::NONE,
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        json_parse(source).map(super::super::codec::non_finite::escape_strings)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        json_key_span(source, pointer).map(Span::from)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        json_value_span(source, pointer).map(Span::from)
    }

    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        completion_context(source, cursor)
    }

    fn write_insert(
        &self,
        key: &str,
        plan: &InsertPlan<'_>,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        write_insert(key, plan, context.base_indent, context.unit)
    }

    fn format_value(&self, value: &Value) -> String {
        json_text(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column).map(|(_, key, start_column)| (key, start_column))
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
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

    fn text(&self) -> &dyn Text {
        self
    }

    fn typing(&self) -> &dyn Typing {
        self
    }

    fn hover(&self) -> &dyn Hover {
        self
    }

    fn layout(&self) -> &dyn SaveLayout {
        self
    }
}

impl Text for JsonBehavior {
    /// From the scan: a value's own span, a container's through its closer.
    fn value_ranges(&self, _language: &dyn ConfigLanguage, source: &str, pointer: &str) -> Vec<std::ops::Range<usize>> {
        if crate::editor::paths::parsed_in_batch(self, source).is_err() {
            return Vec::new();
        }
        json_value_range(source, pointer).into_iter().collect()
    }

    #[cfg(feature = "folding")]
    fn fold_ranges(&self, _language: &dyn ConfigLanguage, source: &str) -> Vec<super::super::language::FoldRange> {
        folding::json_fold_ranges(source, &self.config())
    }

    fn first_span(&self, source: &str) -> Option<Span> {
        first_line_span(source).map(Span::from)
    }

    fn lexical_state(&self, _language: &dyn ConfigLanguage, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = (open_string_quote(source, cursor), false);
        Lexical { quote, in_comment }
    }
}

impl Typing for JsonBehavior {
    fn entry_started(&self, source: &str, cursor: usize) -> bool {
        completion_context(source, cursor).explicit_entry_start
    }
}

impl Hover for JsonBehavior {
    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        key_at_column(line, column).map_or(line.len(), |(token, _, _)| {
            let rest = &line[token.end..];
            // Skip whitespace to the colon, then step past it.
            token.end + (rest.len() - rest.trim_start().len()) + 1
        })
    }
}

impl SaveLayout for JsonBehavior {
    fn edits_one_scalar(&self, source: &str, range: std::ops::Range<usize>, inserted: &str) -> bool {
        super::scalar_lines::one_scalar_line(self, source, range, inserted, &SCALAR_LINE).is_some()
    }

    /// The specification allows any number: one past what the editor
    /// holds saves (see `wide_integers`).
    fn unheld_numbers(&self, source: &str, error: &ParseError) -> Vec<std::ops::Range<usize>> {
        super::wide_integers::unheld_all(source, error)
    }

    /// A character however it is escaped (`\u00e9` for `é`, `\/` for `/`).
    fn comparable_line<'a>(&self, code: &'a str) -> std::borrow::Cow<'a, str> {
        super::escapes::without_unicode_escapes(code)
    }

    /// Without a space after any `:` or `,`, the host writes compact JSON.
    /// Always a string (`"a b"`).
    fn written_key(&self, _language: &dyn ConfigLanguage, _source: &str, _pointer: &str, key: &str) -> Option<String> {
        serde_json::to_string(key).ok()
    }

    fn format_value_like(&self, _language: &dyn ConfigLanguage, value: &Value, written: &str) -> String {
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
}

// Conversion

#[cfg(test)]
mod tests;
