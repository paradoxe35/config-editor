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
//! - `Ctrl+/` toggles the line comment the file writes most (`#` by
//!   default, `//`); `/* */` is also read.

mod tokens;
mod scan;
mod parse;
mod unclosed;
mod words;
mod suggestions;
mod hover;
mod editing;
mod convert;
mod floats;
mod heredoc;
mod expressions;
mod interpolation;
mod list_items;
mod read;
mod big_objects;
mod big_errors;
mod splice;
mod moved_maps;
// The fold ranges (the `folding` feature).
#[cfg(feature = "folding")]
mod folding;

use crate::editor::paths::{byte_at_column, first_line_span, line_column, span_between, utf16_len};
use tokens::*;
use scan::*;
use parse::*;
use suggestions::*;
use hover::*;
use editing::*;
use convert::*;

#[cfg(feature = "folding")]
use super::super::parse_cache::string_bodies;
use super::super::commands::indent_unit;
use super::super::highlight::{MultilineConstruct, Token as HighlightToken, TokenKind, Tokens};
use super::inline::{InlineEntry as CaretEntry, inline_caret, inline_key_at, inline_key_paths, caret_value_end, statement_start_at, statements, entry_follows, in_front_of_inline_entry, matching_close};
use super::super::completion::{Fill, InsertPlan, Planned, RequiredChild};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::item_sources::SaveTold;
use super::super::language::{Text, Typing, Hover, SaveLayout, Sharing, Repeats, Repeated, LanguageTraits, Toggled, toggle_line_comments, unwrap_block_comments};
use crate::editor::comment_toggle::comment_target;
use super::super::language::{EntryPosition, OneLine, LanguageConfig, bracket_newline, CompletionContext, Newline, InlineBlock, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError, SchemaPathSegment,
    inline_block, unclosed_brackets,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) struct HclBehavior;

/// An HCL scalar edit (see `SaveLayout::edits_one_scalar`): a
/// `key = value` line read as a body of its own; the edit adds and removes
/// no quote or escape (a heredoc's text), template, bracket or comma, and
/// no `*/` (a comment over lines).
const SCALAR_LINE: super::scalar_lines::LineRules = super::scalar_lines::LineRules {
    alone: super::scalar_lines::dedented,
    forbidden: &['"', '\\', '$', '%', ',', '(', ')', '[', ']', '{', '}'],
    closers: &["*/"],
};

impl ConfigLanguage for HclBehavior {
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
            brackets: &[('{', '}'), ('[', ']'), ('(', ')')],
            separators: &["=", ":"],
            quotes: &["\""],
            literal_quotes: &[],
            map_brackets: Some(('{', '}')),
            list_brackets: Some(('[', ']')),
            item_separator: Some(','),
            item_marker: None,
            written_separator: Some(" = "),
            // HCL numbers are one arbitrary-precision type: `2` is `2.0`.
            traits: LanguageTraits {
                one_number_type: true,
                ..LanguageTraits::NONE
            },
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        hcl_parse(source).map(super::super::codec::non_finite::escape_strings)
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

    fn key_spans(&self, source: &str, pointers: &[String]) -> Vec<Option<Span>> {
        hcl_key_spans(source, pointers).into_iter().map(|span| span.map(Span::from)).collect()
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
        write_insert(key, plan, context)
    }

    fn format_value(&self, value: &Value) -> String {
        hcl_text(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column)
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    /// An object whose keys are all identifiers becomes a block (`web { … }`),
    /// anything else an attribute. `hcl-rs` escapes `${` as `$${`.
    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        self.written(value, previous, SaveTold::default())
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

    fn sharing(&self) -> Option<&dyn Sharing> {
        Some(self)
    }

    fn repeats(&self) -> Option<&dyn Repeats> {
        Some(self)
    }
}

impl Text for HclBehavior {
    fn first_span(&self, source: &str) -> Option<Span> {
        first_line_span(source).map(Span::from)
    }

    fn lexical_state(&self, _language: &dyn ConfigLanguage, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = hcl_lexical_state(source, cursor);
        Lexical { quote, in_comment }
    }

    #[cfg(feature = "folding")]
    fn fold_ranges(&self, _language: &dyn ConfigLanguage, source: &str) -> Vec<super::super::language::FoldRange> {
        folding::hcl_fold_ranges(source, &string_bodies(self, source))
    }

    fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        heredoc_bodies(source)
    }

    fn comments(&self, _language: &dyn ConfigLanguage, source: &str) -> Vec<std::ops::Range<usize>> {
        comments(source)
    }

    /// Outside strings and the template sequences in them (`"${ "#" }"`).
    fn comment_start(&self, _language: &dyn ConfigLanguage, line: &str) -> Option<usize> {
        line_comment_start(line)
    }

    /// Heredocs, their last line included: their lines are a string's,
    /// with the template sequences in them (see `highlight_continued`).
    fn multiline_constructs(&self, _language: &dyn ConfigLanguage, source: &str) -> Vec<MultilineConstruct> {
        tokenize(source)
            .iter()
            .filter(|token| token.kind == Kind::Heredoc)
            .map(|token| {
                let end = token.end - usize::from(source[..token.end].ends_with('\n'));
                MultilineConstruct::structure(token.start..end)
            })
            .collect()
    }

    /// A heredoc's line (its only multi-line construct): a string, its
    /// template sequences interpolations.
    fn highlight_continued(&self, _language: &dyn ConfigLanguage, text: &str) -> Vec<HighlightToken> {
        let mut out = Tokens::new();
        interpolation::push_heredoc_line(&mut out, text);
        out.finish()
    }
}

impl Typing for HclBehavior {
    fn rewritable_keys(&self, source: &str, cursor_byte: usize, path: &[SchemaPathSegment]) -> HashSet<String> {
        repeated_block_types(source, cursor_byte, path)
    }

    fn newline(&self, _language: &dyn ConfigLanguage, source: &str, cursor: usize) -> Newline {
        if let Some(opened) = open_one_line_block(source, cursor) {
            return opened;
        }
        bracket_newline(
            source,
            cursor,
            &[('{', '}'), ('[', ']'), ('(', ')')],
            &self.config(),
            // A comment-only line has no code: its braces open nothing.
            |line| code_before_comment(line).unwrap_or(""),
        )
    }

    /// `{` after `$` or `%` in a quoted string opens an interpolation or a
    /// directive (`"${`, `"%{`), and in a heredoc's text: its `}` is written
    /// too, as VS Code's HCL support does; `$${` is literal text.
    fn auto_close(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        if typed != '{' || hcl_lexical_state(source, cursor).0 != Some('"') {
            return None;
        }
        let before = &source[..utf16_to_byte_index(source, cursor)];
        let sigil = before.strip_suffix(['$', '%'])?;
        let marker = &before[sigil.len()..];
        let escaped = sigil.ends_with(marker);
        let next = source[before.len()..].chars().next();
        (!escaped && next.is_none_or(|next| !next.is_alphanumeric() && next != '_'))
            .then(|| "}".to_string())
    }

    /// An attribute's `=` (or `:` in an object), or a block's body after its
    /// type and labels (`pol| {`, `servi| "ssh" {`).
    fn value_follows_key(&self, _language: &dyn ConfigLanguage, rest: &str) -> bool {
        // The rest of a dotted name (`tcp|.nodelay`).
        if rest.starts_with('.') {
            return true;
        }
        let rest = rest.trim_start();
        // A body over lines (`{` ending the line) or on one (`{ … }`, `{}`),
        // after any labels.
        let block_body = rest.starts_with('{') || (rest.starts_with('"') && rest.contains('{'));
        rest.starts_with(':') || (rest.starts_with('=') && !rest.starts_with("==")) || block_body
    }

    /// As the default, commenting with the line comment the file writes
    /// most (`//` in a file of `//` comments, else `#`).
    fn toggle_comments(&self, _language: &dyn ConfigLanguage, source: &str, selection_start: usize, selection_end: usize) -> Option<Toggled> {
        let block = ("/*", "*/");
        if let Some(unwrapped) = unwrap_block_comments(source, selection_start, selection_end, block) {
            return Some(unwrapped);
        }
        let (start, end) = comment_target(self, source, selection_start, selection_end);
        unwrap_block_comments(source, start, end, block)
            .or_else(|| toggle_line_comments(source, start, end, comment_markers(source)))
    }
}

impl Hover for HclBehavior {
    fn hover_path(
        &self,
        source: &str,
        line_index: usize,
        column: usize,
    ) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        header_path(source, line_index, column)
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        // Just past the hovered key's `=`, so a key inside a value map
        // resolves under that map.
        code_before_comment(line)
            .and_then(|code| inline_key_at(code, b'=', byte_at_column(line, column)))
            .map_or(line.len(), |key| key.after_separator)
    }
}

impl SaveLayout for HclBehavior {
    fn serialize_over(&self, value: &Value, previous: Option<&str>, told: SaveTold<'_>) -> Option<Result<String, String>> {
        Some(self.written(value, previous, told))
    }

    fn edits_one_scalar(&self, source: &str, range: std::ops::Range<usize>, inserted: &str) -> bool {
        super::scalar_lines::one_scalar_line(self, source, range, inserted, &SCALAR_LINE).is_some()
    }

    /// The specification allows any number: one past what the editor
    /// holds saves (see `wide_integers`).
    fn unheld_numbers(&self, source: &str, error: &ParseError) -> Vec<std::ops::Range<usize>> {
        super::wide_integers::unheld_all(source, error)
    }

    /// A block stays a block and an object attribute an attribute; an
    /// expression stays an expression and a literal string a literal.
    fn changed_form(&self, previous: &str, patched: &str, told: SaveTold<'_>) -> Option<String> {
        changed_form(previous, patched, told).or_else(|| expressions::changed_expression(previous, patched))
    }

    fn changed_form_reason(&self, value: &Value) -> Option<String> {
        expressions::broken_expression(value.as_str()?).then(|| {
            "this value no longer reads as an HCL expression (`${…}` holding one), so HCL would write it as plain text: correct the expression, or edit it as HCL".to_string()
        })
    }

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

    fn opens_list_item(&self, language: &dyn ConfigLanguage, lines: &[&str], index: usize) -> bool {
        let comment_start = |line: &str| language.comment_start(line).unwrap_or(line.len());
        list_items::opens_list_item(&comment_start, lines, index)
    }

    fn item_lines(&self, source: &str, pointer: &str) -> Option<std::ops::Range<usize>> {
        repeated_block_lines(source, pointer)
    }

    fn writes_as_labels(&self, source: &str, pointer: &str) -> bool {
        let wanted = super::super::language::pointer_segments(pointer);
        labelled_blocks(source)
            .iter()
            .any(|(path, labels)| wanted.len() >= path.len() && wanted.len() < path.len() + labels && wanted[..path.len()] == path[..])
    }

    fn one_line_container(&self, source: &str, pointer: &str) -> Option<OneLine> {
        one_line_block(source, pointer)
    }

    /// An assignment aligned as `terraform fmt` writes it (`p    = 1`) is
    /// the serializer's `p = 1`.
    fn comparable_line<'a>(&self, code: &'a str) -> std::borrow::Cow<'a, str> {
        // Aligned as written, a character however it is escaped.
        match super::aligned::single_spaced(code) {
            std::borrow::Cow::Borrowed(code) => super::escapes::without_unicode_escapes(code),
            std::borrow::Cow::Owned(code) => {
                std::borrow::Cow::Owned(super::escapes::without_unicode_escapes(&code).into_owned())
            }
        }
    }

    fn key_renames(&self, source: &str, pointer: &str, new_key: &str) -> Option<Vec<crate::editor::language::KeyEdit>> {
        block_key_renames(source, pointer, new_key)
    }

    /// An identifier (`café`, `x-y`) bare; any other key quoted where a
    /// string may stand (a block label, an object's key), and nowhere else:
    /// a block type or an attribute name is an identifier. In an object,
    /// `null`, `true` and `false` are values, so quoted.
    fn written_key(&self, _language: &dyn ConfigLanguage, source: &str, pointer: &str, key: &str) -> Option<String> {
        let names = names_block_or_attribute(source, pointer);
        let keyword = matches!(key, "null" | "true" | "false");
        if hcl::Identifier::new(key).is_ok() && (names || !keyword) {
            return Some(key.to_string());
        }
        (!names).then(|| hcl_text(&Value::String(key.to_string())))
    }

    /// A heredoc (`<<EOT` … `EOT`) stays one while its text ends with a line
    /// break and holds no carriage return or other control character (a
    /// heredoc has no escapes, and readers drop a raw one before a line
    /// break): else a quoted string. Text holding a line equal to its
    /// terminator takes another one (`EOT_1`).
    fn format_value_like(&self, _language: &dyn ConfigLanguage, value: &Value, written: &str) -> String {
        if let (Some(text), Some(header)) = (value.as_str(), written.lines().next().filter(|line| line.starts_with("<<")))
            && let Some(terminator) = written.lines().last().filter(|_| written.lines().count() > 1)
            && text.ends_with('\n')
            && !text.chars().any(|character| character.is_control() && !matches!(character, '\t' | '\n'))
            && let Some((header, terminator)) = heredoc::free_delimiter(header, terminator, text)
        {
            let (header, terminator) = (header.as_str(), terminator.as_str());
            // An indented heredoc (`<<-EOT`) keeps its text's indentation:
            // the readers take off the shortest one.
            let body: Vec<&str> = written.lines().skip(1).take(written.lines().count().saturating_sub(2)).collect();
            let indent = header
                .starts_with("<<-")
                .then(|| {
                    body.iter()
                        .filter(|line| !line.trim().is_empty())
                        .map(|line| &line[..line.len() - line.trim_start_matches([' ', '\t']).len()])
                        .min_by_key(|indent| indent.len())
                })
                .flatten()
                .unwrap_or("");
            // Its `${` and `%{` escaped as text, unless the host's heredoc
            // is a template (`${var.x}` in it), whose text stays its own.
            let raw = body.join("\n");
            let text =
                if heredoc::has_sequence(&raw) { text.to_string() } else { heredoc::escaped(text) };
            let text: String = text
                .split_inclusive('\n')
                .map(|line| if line.trim().is_empty() { line.to_string() } else { format!("{indent}{line}") })
                .collect();
            return format!("{header}\n{text}{terminator}");
        }
        // An expression the host wrote (`var.azs`, read as `${var.azs}`;
        // `file("${var.dir}/x")`, `[for s in l : upper(s)]`) stays one,
        // as the view spells it.
        if let Some(expression) = value.as_str().and_then(|text| expressions::rewritten(written, text)) {
            return expression;
        }
        // A template the host wrote (`"${var.port}"`) stays one: its `${`
        // is not escaped into text.
        if let Some(text) = value.as_str()
            && is_template(written)
        {
            return serde_json::to_string(text).unwrap_or_else(|_| text.to_string());
        }
        let text = self.format_value(value);
        if value.is_string() { super::escapes::like_written(text, written, super::escapes::Astral::Long) } else { text }
    }

    fn scalar_reads_as(&self, written: &str, value: &Value) -> bool {
        self.parse(&format!("k = {}\n", written.trim())).ok().and_then(|document| document.get("k").cloned()).as_ref() == Some(value)
    }

    fn number_in_form(&self, written: &str, value: &serde_json::Number) -> Option<String> {
        super::number_forms::checked(written, value, &super::number_forms::DECIMAL, |text, value| {
            self.scalar_reads_as(text, value)
        })
    }
}

impl Sharing for HclBehavior {
    fn expressions(&self, source: &str) -> Vec<String> {
        expressions::expression_pointers(source)
    }
}

impl Repeats for HclBehavior {
    fn repeated(&self) -> Repeated {
        Repeated::Blocks
    }

    /// A block written once (`/b`, `/svc/a`): never an attribute, nor a
    /// block type with labels (an object keyed by them).
    fn lone_item(&self, source: &str, pointer: &str) -> bool {
        hcl_lone_block(source, pointer)
    }
}

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod completion_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod splice_tests;
#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod big_errors_tests;
#[cfg(test)]
mod expressions_scale_tests;

impl HclBehavior {
    /// [`ConfigLanguage::serialize`], told which host item each item of
    /// `value` is by an Edit-as save (see `SaveTold`).
    fn written(&self, value: &Value, previous: Option<&str>, told: SaveTold<'_>) -> Result<String, String> {
        let Value::Object(map) = value else {
            return Err("an HCL document is a body: the top level must be an object".to_string());
        };
        // Maps are written in the host's forms again: labelled blocks,
        // attributes, blocks on one line.
        let mut forms = previous.map(host_forms).unwrap_or_default();
        if let Some(previous) = previous {
            renamed_labelled_types(&mut forms, previous, map);
            // A map the save moves keeps its form (see `moved_maps`).
            let held = (std::mem::take(&mut forms.attributes), std::mem::take(&mut forms.blocks));
            (forms.attributes, forms.blocks) = moved_maps::with_moved(held, told, convert::forms_of);
        }
        let rendered = splice::writing(|| hcl::format::to_string(&hcl_body(map, &forms, &[])?).map_err(|error| error.to_string()))?;
        let rendered = fold_one_line_blocks(&rendered, &forms.one_line);
        // List items have no span of their own: the shared scanner finds them.
        let spans = |text: &str, pointers: &[String]| -> Vec<Option<Span>> {
            hcl_value_spans(text, pointers)
                .into_iter()
                .zip(pointers)
                .map(|(span, pointer)| span.map(Span::from).or_else(|| super::super::codec::inline_location(self, text, pointer, false)))
                .collect()
        };
        Ok(floats::with_whole_floats(&rendered, value, spans))
    }
}
