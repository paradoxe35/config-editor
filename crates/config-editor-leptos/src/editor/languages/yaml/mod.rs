//! YAML text-layer behavior.
//!
//! Decisions: duplicate keys are a parse error; comments are `#` line comments,
//! toggled per line. Parsing goes through `noyalib`, but completion context and
//! key/value spans come from line-based scans so they work while the document
//! is mid-edit and unparsable.



mod parse;
mod suggestions;
mod hover;
mod editing;

use parse::*;
use suggestions::*;
use hover::*;
use editing::*;

use super::super::commands::indent_unit;
use super::super::highlight::{Token, TokenKind, Tokens};
use super::super::inline::{caret_value_end, 
    InlineEntry as CaretEntry, inline_caret, inline_key_at, inline_sibling_keys,
    statement_start, entry_follows, in_front_of_inline_entry,
};
use super::super::completion::{array_branch, object_branch, schema_const, schema_default};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::{LanguageConfig, bracket_newline, CompletionContext, Newline, EntryPosition, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError,
    SchemaPathSegment,
};
use noyalib::{DuplicateKeyPolicy, ParserConfig};
use serde_json::Value;
use std::collections::HashSet;

pub(super) struct YamlBehavior;


impl ConfigLanguage for YamlBehavior {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "YAML",
            icon: "Y",
            extensions: &["yaml", "yml"],
        }
    }

    fn config(&self) -> LanguageConfig {
        LanguageConfig {
            line_comments: &["#"],
            brackets: &[('{', '}'), ('[', ']')],
            separators: &[":"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        if let Some(error) = tab_indentation(source) {
            return Err(error);
        }
        // Only comments and document markers: an empty document, as an empty
        // file is.
        let has_content = source.lines().any(|line| {
            let code = code_before_comment(line).trim();
            !code.is_empty() && code != "---" && code != "..."
        });
        if !has_content {
            return Ok(Value::Object(serde_json::Map::new()));
        }
        yaml_parse(source).map_err(|mut error| {
            // A second document has no line from the parser: mark its `---`.
            if error.line.is_none()
                && let Some(index) = source
                    .lines()
                    .enumerate()
                    .skip_while(|(_, line)| code_before_comment(line).trim().is_empty())
                    .skip(1)
                    .find(|(_, line)| line.trim_end() == "---")
                    .map(|(index, _)| index)
            {
                error.line = Some(index + 1);
                error.column = Some(1);
            }
            error
        })
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        yaml_span_for_path(source, pointer).map(Span::from)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        yaml_value_span_for_path(source, pointer).map(Span::from)
    }

    fn first_span(&self, source: &str) -> Option<Span> {
        first_yaml_span(source).map(Span::from)
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
        existing_mapping_keys(source, cursor_byte, path)
    }

    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        if context.position == EntryPosition::Inline {
            return flow_property_insert(key, property, root);
        }
        property_completion_insert(key, property, root, context.base_indent, context.unit)
    }

    fn format_value(&self, value: &Value) -> String {
        yaml_value(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column)
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        hover_context_offset(line, column)
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    fn newline(&self, source: &str, cursor: usize) -> Newline {
        // Between flow brackets (`[|]`) the collection opens across lines.
        let byte = utf16_to_byte_index(source, cursor);
        let before = source[..byte].trim_end_matches([' ', '\t']);
        let after = source[byte..].trim_start_matches([' ', '\t']);
        if before.ends_with(['[', '{']) && after.starts_with([']', '}']) {
            return bracket_newline(source, cursor, &[('{', '}'), ('[', ']')], code_before_comment);
        }
        // Enter on an empty `- ` item ends the list: the dash goes, and the
        // caret stays at the list's indentation.
        let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
        let line = &source[line_start..byte];
        if line.trim() == "-" && after.split('\n').next().is_some_and(|rest| rest.trim().is_empty()) {
            let content_start = line.len() - line.trim_start().len();
            return Newline {
                text: String::new(),
                caret: 0,
                replaces: 0,
                replaces_before: line.len() - content_start,
            };
        }
        Newline::new(newline_with_indent(source, cursor))
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = yaml_lexical_state(source, cursor);
        Lexical { quote, in_comment }
    }

    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        noyalib::to_string(value).map_err(|error| error.to_string())
    }

    fn comment_start(&self, line: &str) -> Option<usize> {
        line_comment_start(line)
    }


    /// Values left empty at line ends (`key: `) and in flow maps
    /// (`{ cert: , key:  }`).
    fn empty_value_slots(&self, text: &str) -> Vec<usize> {
        let mut slots = super::super::language::empty_slots_at_line_ends(text, &[": ", ":"]);
        for (at, _) in text.match_indices(':') {
            let rest = &text[at + 1..];
            let spaces = rest.len() - rest.trim_start_matches(' ').len();
            if spaces > 0 && rest[spaces..].starts_with([',', '}']) {
                slots.push(at + 2);
            }
        }
        slots
    }

    fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        block_scalar_bodies(source)
    }
}


#[cfg(test)]
#[cfg_attr(
    not(feature = "schema-validation"),
    allow(unused_imports, dead_code, reason = "the completion tests need schema validation")
)]
mod tests;
