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
mod colors;
mod style;
mod block_scalar;
mod folded;
mod newlines;
mod plain;
mod constructs;
mod enter;
use style::{comparable_yaml_line, node_properties_len};
mod aliases;
mod wide_negatives;
mod tags;
// The fold ranges (the `folding` feature).
#[cfg(feature = "folding")]
mod folding;

use parse::*;

/// The span of `pointer` in a document whose root is a flow collection
/// opened at `root`: found in it as in a nested one.
fn in_flow_root(language: &dyn ConfigLanguage, text: &str, root: (usize, usize, usize), pointer: &str, key: bool) -> Option<Span> {
    let segments = super::super::language::pointer_segments(pointer);
    if segments.is_empty() {
        return (!key).then(|| Span::from(root));
    }
    super::super::codec::in_flow_container(language, text, Span::from(root), &segments, key)
}
use suggestions::*;
use hover::*;
use editing::*;
use colors::*;

use super::super::commands::indent_unit;
use super::super::highlight::{MultilineConstruct, Token, TokenKind, Tokens};
use super::super::inline::{caret_value_end, 
    InlineEntry as CaretEntry, inline_caret, inline_key_at, inline_sibling_keys,
    statement_start, entry_follows, in_front_of_inline_entry,
};
use super::super::completion::{array_branch, object_branch, schema_const, schema_default, schema_example};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::{LanguageConfig, CompletionContext, Newline, EntryPosition, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError,
    SchemaPathSegment,
};
use noyalib::{DuplicateKeyPolicy, ParserConfig};
use serde_json::Value;
use std::collections::HashSet;

pub(super) struct YamlBehavior;

/// Why a text of several YAML documents (`---` between them) is not read.
const MULTI_DOCUMENT: &str = "This text holds more than one YAML document (separated by ---): the editor reads one document per file. Keep one, or split them into files";


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
            block_comment: None,
            brackets: &[('{', '}'), ('[', ']')],
            separators: &[":"],
            quotes: &["\"", "'"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        // A lone carriage return breaks a line, as CRLF does (YAML 1.2's
        // line breaks): read as LF, as the editor holds the text, so a file
        // with them reads the same headless (a comment line ending in one
        // would otherwise run on over the data after it).
        let normalized;
        let source = if source.contains('\r') {
            normalized = source.replace("\r\n", "\n").replace('\r', "\n");
            normalized.as_str()
        } else {
            source
        };
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
        // A `---` (at column 0) with nothing after it but comments ends the
        // document before it: an empty document, not a second one.
        let code_lines: Vec<(usize, &str, &str)> = source
            .split_inclusive('\n')
            .scan(0, |at, line| {
                let start = *at;
                *at += line.len();
                Some((start, line, code_before_comment(line).trim()))
            })
            .filter(|(_, _, code)| !code.is_empty())
            .collect();
        let source = match code_lines.split_last() {
            Some((&(start, line, "---"), before))
                if line.starts_with("---") && before.iter().any(|(_, _, code)| *code != "---") =>
            {
                &source[..start]
            }
            _ => source,
        };
        yaml_parse(source).map(with_merge_keys).map_err(|mut error| {
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
            if error.message.contains("more than one document") {
                error.message = MULTI_DOCUMENT.to_string();
            }
            error
        })
        .and_then(|value| match wide_negatives::first_wide_negative(source, &value) {
            Some(error) => Err(error),
            None => Ok(value),
        })
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        super::super::byte_order_mark::span(source, |text| match root_flow_span(text) {
            Some(root) => in_flow_root(self, text, root, pointer, true),
            None => yaml_span_for_path(text, pointer).map(Span::from),
        })
    }

    /// The root's value is found only when it is a flow collection.
    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        super::super::byte_order_mark::span(source, |text| match root_flow_span(text) {
            Some(root) => in_flow_root(self, text, root, pointer, false),
            None => yaml_value_span_for_path(text, pointer).map(Span::from),
        })
    }

    fn first_span(&self, source: &str) -> Option<Span> {
        super::super::byte_order_mark::span(source, |text| first_yaml_span(text).map(Span::from))
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

    /// Bare where a value could be (and an identifier), else in double
    /// quotes: `"on"`, `"1_000"`, `"host name"`.
    /// As the document writer writes it: plain where every reader takes it
    /// as that text (`foo bar`, `a/b`, `Über`), quoted where YAML 1.1 types
    /// it (`on`, `1_000`, a date) or it holds what plain text cannot.
    fn written_key(&self, _source: &str, _pointer: &str, key: &str) -> Option<String> {
        let mut entry = serde_json::Map::new();
        entry.insert(key.to_string(), Value::from(0));
        let written = self
            .serialize(&Value::Object(entry), None)
            .ok()
            .and_then(|text| Some(text.trim_end_matches('\n').strip_suffix(": 0")?.to_string()))
            .filter(|written| !written.contains('\n') && !written.starts_with('?'));
        Some(written.unwrap_or_else(|| yaml_quoted(key)))
    }

    /// A key opening a block (`websocket:`) takes its indented lines along:
    /// commented alone, they would be left without their parent.
    fn comment_scope(&self, source: &str, start: usize, end: usize) -> (usize, usize) {
        children_of_selected_keys(source, start, end)
    }

    fn comparable_line<'a>(&self, code: &'a str) -> std::borrow::Cow<'a, str> {
        comparable_yaml_line(code)
    }

    fn reference_len(&self, text: &str) -> Option<usize> {
        let name = text.strip_prefix('*')?;
        let length = name.find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '-'))).unwrap_or(name.len());
        (length > 0).then_some(length + 1)
    }

    fn value_properties_len(&self, text: &str) -> usize {
        node_properties_len(text)
    }

    /// `tcp: {` after a bare key (a JSON object's key is quoted).
    fn flow_map_opener(&self, code: &str) -> bool {
        code.ends_with('{') && code.contains(": ") && !code.starts_with(['"', '\''])
    }

    fn detach_shared(&self, previous: &str, old: &Value, new: &Value) -> Option<String> {
        aliases::detach_shared(previous, old, new)
    }

    fn tags(&self, source: &str) -> Vec<(String, String)> {
        tags::yaml_tags(source)
    }

    fn scalar_reads_as(&self, written: &str, value: &Value) -> bool {
        self.parse(&format!("k: [{}]\n", written.trim())).ok().and_then(|document| document.get("k")?.get(0).cloned()).as_ref() == Some(value)
    }

    fn format_value_like(&self, value: &Value, written: &str) -> String {
        // Where the host writes a value without its place (a flow
        // collection's entries): as inside one.
        style::yaml_value_like(self, value, written, "", true)
    }

    /// A block scalar kept with `+` holds the blank lines after its text.
    fn written_value_range(&self, source: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        block_scalar::with_kept_blank_lines(source, range)
    }

    fn format_value_at(&self, value: &Value, source: &str, range: std::ops::Range<usize>) -> String {
        let line_start = source[..range.start].rfind('\n').map_or(0, |index| index + 1);
        let in_flow = style::in_flow_collection(source, range.clone());
        let prefix = &source[line_start..range.start];
        // After a `!!str` tag a plain value is text whatever it looks like
        // (`!!str 2`): it stays plain.
        if let Some(text) = value.as_str()
            && prefix.trim_end().ends_with("!!str")
            && !source[range.clone()].trim_start().starts_with(['"', '\''])
            && !text.contains(['\n', '#'])
            && self.parse(&format!("k: !!str {text}\n")).ok() == Some(serde_json::json!({ "k": text }))
            && (!in_flow || self.parse(&format!("k: [!!str {text}]\n")).ok() == Some(serde_json::json!({ "k": [text] })))
        {
            return text.to_string();
        }
        style::yaml_value_like(self, value, &source[range.clone()], prefix, in_flow)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column)
    }

    fn hover_path(&self, source: &str, line_index: usize, column: usize) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        explicit_key_path(source, line_index, column)
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        hover_context_offset(line, column)
    }

    fn item_start(&self, source: &str, line_start: usize, line: &str, column: usize) -> Option<usize> {
        // A block sequence's `- ` (or a dash alone on its line) opens an item.
        let (at, marker) = line.char_indices().nth(column)?;
        let after = &line[at + marker.len_utf8()..];
        match after.chars().next() {
            _ if marker != '-' => crate::editor::language::bracket_item_start(self, source, line_start, line, column),
            None => Some(at + 1),
            Some(next) if next.is_whitespace() => Some(at + 1 + next.len_utf8()),
            Some(_) => None,
        }
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    fn newline(&self, source: &str, cursor: usize) -> Newline {
        enter::newline(source, cursor)
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = yaml_lexical_state(source, cursor);
        Lexical { quote, in_comment }
    }

    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        use super::super::codec::non_finite::Floats;
        // Strings of line breaks only are written `"\n"` (see `newlines`).
        let marked = newlines::marked(value);
        let (value, markers) = match &marked {
            Some((marked, markers)) => (marked, markers.as_slice()),
            None => (value, &[][..]),
        };
        noyalib::to_string(&Floats(value))
            .map(|text| plain::quote_typed_plain(&text, value, &previous.map(plain::keys_written_bare).unwrap_or_default()))
            .map(|text| newlines::written(text, markers))
            .map(|text| match previous {
                // Nested sequences written compact, as the host writes them.
                Some(previous) if style::writes_compact_sequences(previous) => style::compact_sequences(&text),
                _ => text,
            })
            .map_err(|error| error.to_string())
    }

    /// `.inf`, `-.inf` and `.nan`.
    fn writes_non_finite(&self) -> bool {
        true
    }

    fn indent_is_structure(&self) -> bool {
        true
    }

    fn plain_scalars(&self) -> bool {
        true
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

    #[cfg(feature = "folding")]
    fn fold_ranges(&self, source: &str) -> Vec<super::super::language::FoldRange> {
        folding::yaml_fold_ranges(source, &self.string_body_lines(source))
    }

    fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        let mut bodies = block_scalar_bodies(source);
        bodies.extend(quoted_scalar_bodies(source));
        bodies
    }

    /// With a plain scalar's continuation lines (`p: plain` then
    /// `  continued`), which the editor leaves to be typed as keys.
    fn saved_string_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        let mut bodies = self.string_body_lines(source);
        let plain = plain_scalar_bodies(source, &bodies);
        bodies.extend(plain);
        bodies.sort_by_key(|range| range.start);
        bodies
    }

    /// Quoted scalars, plain scalars' continuation lines, and flow
    /// collections over lines.
    fn multiline_constructs(&self, source: &str) -> Vec<MultilineConstruct> {
        constructs::multiline_constructs(source, &self.string_body_lines(source))
    }

    /// The inside of a flow collection: its entries.
    fn highlight_continued(&self, text: &str) -> Vec<Token> {
        let mut tokens = Tokens::new();
        constructs::flow_tokens(text, 0, &mut tokens);
        tokens.finish()
    }
}


#[cfg(test)]
#[cfg_attr(
    not(feature = "schema-validation"),
    allow(unused_imports, dead_code, reason = "the completion tests need schema validation")
)]
mod tests;
