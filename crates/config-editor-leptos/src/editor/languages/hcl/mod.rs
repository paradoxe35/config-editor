//! HCL (Terraform) language support.
//!
//! - Syntax errors come from `hcl-rs`; duplicate attribute keys are errors,
//!   repeated blocks are not (multiple `ingress {}` is idiomatic).
//! - Structure comes from a tolerant token scan of `type "label" { … }`
//!   headers and `key = value` attributes.
//! - Paths follow the HCL JSON mapping: block types and labels nest as object
//!   keys (`resource "aws_x" "y" { count = 2 }` is `/resource/aws_x/y/count`),
//!   and repeated same-header blocks index as arrays (`/variable/name/1`).
//! - Objects insert as blocks, scalars as `key = value`; strings always quote.
//! - `Ctrl+/` toggles `#` line comments; `//` and `/* */` are also read.

mod tokens;
mod scan;
mod parse;
mod suggestions;
mod hover;
mod editing;
mod convert;

use tokens::*;
use scan::*;
use parse::*;
use suggestions::*;
use hover::*;
use editing::*;
use convert::*;

use super::super::commands::indent_unit;
use super::super::highlight::{Token as HighlightToken, TokenKind, Tokens};
use super::super::inline::{InlineEntry as CaretEntry, inline_caret, inline_key_at, inline_key_paths, inline_sibling_keys, inline_value_len, caret_value_end, statement_start, statements, entry_follows, in_front_of_inline_entry, matching_close};
use super::super::completion::{
    array_branch, object_branch, placeholder, schema_const, schema_default,
};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{EntryPosition, OneLine, LanguageConfig, bracket_newline, CompletionContext, Newline, Repetition, InlineBlock, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError, SchemaPathSegment,
    inline_block, unclosed_brackets,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) struct HclBehavior;


impl ConfigLanguage for HclBehavior {
    fn repetition(&self) -> Repetition {
        Repetition::Blocks
    }

    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "HCL",
            icon: "H",
            extensions: &["hcl", "tf", "tfvars"],
        }
    }

    fn config(&self) -> LanguageConfig {
        LanguageConfig {
            line_comments: &["#", "//"],
            block_comment: Some(("/*", "*/")),
            brackets: &[('{', '}'), ('[', ']')],
            separators: &["=", ":"],
            quotes: &["\""],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        hcl_parse(source)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        hcl_key_span(source, pointer).map(Span::from)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        hcl_value_span(source, pointer).map(Span::from)
    }

    fn value_spans(&self, source: &str, pointers: &[String]) -> Vec<Option<Span>> {
        hcl_value_spans(source, pointers).into_iter().map(|span| span.map(Span::from)).collect()
    }

    /// A one-line block holds a single attribute; more open it over lines.
    fn one_line_body(&self, value: &Value, indent: &str, unit: &str) -> Option<String> {
        let map = value.as_object()?;
        if let Some((key, child)) = map.iter().next().filter(|_| map.len() == 1)
            && is_ident(key)
            && !child.is_object()
        {
            return Some(format!("{{ {key} = {} }}", hcl_text(child)));
        }
        let body = hcl::format::to_string(&hcl_body(map, &HostForms::default(), &[]).ok()?).ok()?;
        let mut text = String::from("{\n");
        for line in body.lines() {
            if !line.is_empty() {
                text.push_str(indent);
                text.push_str(unit);
                text.push_str(line);
            }
            text.push('\n');
        }
        text.push_str(indent);
        text.push('}');
        Some(text)
    }

    fn one_line_container(&self, source: &str, pointer: &str) -> Option<OneLine> {
        one_line_block(source, pointer)
    }

    fn rename_key(&self, source: &str, pointer: &str, new_key: &str) -> Option<String> {
        rename_block_key(source, pointer, new_key)
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
        existing_body_keys(source, cursor_byte, path)
    }

    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        property_insert(key, property, root, context.base_indent, context.unit, context.position == EntryPosition::Block)
    }

    fn format_value(&self, value: &Value) -> String {
        hcl_text(value)
    }

    /// A heredoc (`<<EOT` … `EOT`) stays one while its text ends with a line
    /// break and holds no line equal to its terminator.
    fn format_value_like(&self, value: &Value, written: &str) -> String {
        if let (Some(text), Some(header)) = (value.as_str(), written.lines().next().filter(|line| line.starts_with("<<")))
            && let Some(terminator) = written.lines().last().filter(|_| written.lines().count() > 1)
            && text.ends_with('\n')
            && !text.lines().any(|line| line.trim() == terminator.trim())
        {
            return format!("{header}\n{text}{terminator}");
        }
        // A template the host wrote (`"${var.port}"`) stays one: its `${`
        // is not escaped into text.
        if let Some(text) = value.as_str()
            && is_template(written)
        {
            return serde_json::to_string(text).unwrap_or_else(|_| text.to_string());
        }
        self.format_value(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column)
    }

    fn hover_path(
        &self,
        source: &str,
        line_index: usize,
        column: usize,
    ) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        // A block type (`services` in `services "web" {`) names a key of the
        // enclosing body.
        let line = source.split('\n').nth(line_index)?;
        let code = code_before_comment(line).unwrap_or(line);
        let trimmed = code.trim_start();
        let indent = code.len() - trimmed.len();
        let name_end = trimmed
            .find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '-')))
            .unwrap_or(trimmed.len());
        let rest = trimmed[name_end..].trim_start();
        let is_block = name_end > 0 && (rest.starts_with('"') || rest.starts_with('{')) && code.trim_end().ends_with('{');
        if !is_block || column < indent || column >= indent + name_end {
            return None;
        }
        let name = trimmed[..name_end].to_string();
        let line_start: usize = source.split('\n').take(line_index).map(|line| line.len() + 1).sum();
        let mut path = self
            .completion_context(source, source[..line_start].encode_utf16().count())
            .path;
        path.push(SchemaPathSegment::Key(name.clone()));
        Some((path, name, indent))
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        // Just past the hovered key's `=`, so a key inside a value map
        // resolves under that map.
        code_before_comment(line)
            .and_then(|code| inline_key_at(code, b'=', byte_at_column(line, column)))
            .map_or(line.len(), |key| key.after_separator)
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    fn newline(&self, source: &str, cursor: usize) -> Newline {
        if let Some(opened) = open_one_line_block(source, cursor) {
            return opened;
        }
        bracket_newline(
            source,
            cursor,
            &[('{', '}'), ('[', ']'), ('(', ')')],
            // A comment-only line has no code: its braces open nothing.
            |line| code_before_comment(line).unwrap_or(""),
        )
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = hcl_lexical_state(source, cursor);
        Lexical { quote, in_comment }
    }


    /// An object whose keys are all identifiers becomes a block (`web { … }`),
    /// anything else an attribute. `hcl-rs` escapes `${` as `$${`.
    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        let Value::Object(map) = value else {
            return Err("an HCL document is a body: the top level must be an object".to_string());
        };
        // Maps are written in the host's forms again: labelled blocks,
        // attributes, blocks on one line.
        let forms = previous.map(host_forms).unwrap_or_default();
        let rendered = hcl::format::to_string(&hcl_body(map, &forms, &[])?).map_err(|error| error.to_string())?;
        Ok(fold_one_line_blocks(&rendered, &forms.one_line))
    }

    /// An attribute's `=` (or `:` in an object), or a block's body after its
    /// type and labels (`pol| {`, `servi| "ssh" {`).
    fn value_follows_key(&self, rest: &str) -> bool {
        let rest = rest.trim_start();
        // A body over lines (`{` ending the line) or on one (`{ … }`, `{}`),
        // after any labels.
        let block_body = rest.starts_with('{') || (rest.starts_with('"') && rest.contains('{'));
        rest.starts_with(':') || (rest.starts_with('=') && !rest.starts_with("==")) || block_body
    }


    fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        heredoc_bodies(source)
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


#[cfg(test)]
mod tests;
