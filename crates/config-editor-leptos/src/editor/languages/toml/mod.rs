//! TOML language support.
//!
//! - Syntax errors come from the `toml` crate; duplicate keys are errors.
//! - Structure comes from a tolerant line scan: `[table]` headers set the
//!   path from the root, `[[array]]` headers add the occurrence index, and
//!   dotted keys nest under the current table.
//! - Inline tables (`key = { … }`) are scanned within their line; completion
//!   inside one stays inline, since a `[table]` header is invalid there.
//! - Objects insert as `[table]` headers, except beneath array items, which
//!   headers cannot address, where they are written inline.
//! - `Ctrl+/` toggles `#` line comments.

mod parse;
mod suggestions;
mod editing;

use parse::*;
use suggestions::*;
use editing::*;

use super::super::inline::{InlineEntry as CaretEntry, inline_caret, inline_key_at, inline_keys, inline_sibling_keys, inline_value_len, statement_start, entry_follows};
use super::super::completion::{
    array_branch, object_branch, placeholder, schema_const, schema_default,
};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{bracket_newline, toggle_line_comments, CompletionContext, Newline, EntryPosition, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError,
    SchemaPathSegment,
};
use super::super::highlight::{Token, TokenKind, Tokens};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) struct TomlBehavior;


impl ConfigLanguage for TomlBehavior {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "TOML",
            icon: "T",
            extensions: &["toml"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        toml_parse(source)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        toml_key_span(source, pointer).map(Span::from)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        toml_value_span(source, pointer).map(Span::from)
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
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        existing_table_keys(source, cursor_byte, path)
    }

    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        property_insert(key, property, root, context)
    }

    fn format_value(&self, value: &Value) -> String {
        toml_text(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column)
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        hover_context_offset(line, column)
    }

    fn hover_path(
        &self,
        source: &str,
        line_index: usize,
        column: usize,
    ) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        dotted_hover_path(self, source, line_index, column)
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    fn newline(&self, source: &str, cursor: usize) -> Newline {
        inline_table_exit(source, cursor).unwrap_or_else(|| {
            bracket_newline(source, cursor, &[('[', ']')], |line| split_toml_comment(line).0)
        })
    }

    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        toggle_line_comments(source, selection_start, selection_end, &["#"])
    }

    fn auto_close(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        // A third quote typed in a row opens a multi-line string.
        if !matches!(typed, '"' | '\'') {
            return None;
        }
        let byte = utf16_to_byte_index(source, cursor);
        let before = &source[..byte];
        let pair: String = [typed, typed].iter().collect();
        let opens = before.ends_with(&pair)
            && !before[..before.len() - 2].ends_with(typed)
            && toml_lexical_state(source, cursor.saturating_sub(2)).0.is_none();
        opens.then(|| [typed; 3].iter().collect())
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = toml_lexical_state(source, cursor);
        Lexical { quote, in_comment }
    }


    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        use super::super::codec::first_pointer;
        if !value.is_object() {
            return Err("a TOML document is a table: the top level must be an object".to_string());
        }
        if let Some(pointer) = first_pointer(value, &Value::is_null) {
            return Err(format!("TOML has no null value (at {pointer})"));
        }
        toml::to_string(value).map_err(|error| error.to_string())
    }

    fn comment_start(&self, line: &str) -> Option<usize> {
        super::super::language::comment_start_outside_quotes(line, &["#"])
    }

    fn value_follows_key(&self, rest: &str) -> bool {
        let rest = rest.trim_start();
        rest.starts_with('=') && !rest.starts_with("==")
    }

    fn line_has_value(&self, line_before: &str) -> bool {
        super::super::language::value_after_separator(line_before, &['='])
    }

    fn empty_value_slots(&self, text: &str) -> Vec<usize> {
        super::super::language::empty_slots_at_line_ends(text, &[" = "])
    }
}

fn utf16(text: &str) -> usize {
    text.encode_utf16().count()
}

/// One-based line and character column of a byte offset.
fn line_column(source: &str, byte: usize) -> (usize, usize) {
    let before = &source[..byte.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, before[line_start..].chars().count() + 1)
}


// Conversion




#[cfg(test)]
mod tests;
