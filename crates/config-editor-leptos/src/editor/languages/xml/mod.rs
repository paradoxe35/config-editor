//! XML language support.
//!
//! - Syntax errors come from `quick-xml`; duplicate attributes are errors.
//!   Structure comes from a tolerant token scan, since the document is
//!   usually invalid while a tag is being typed.
//! - A top-level `<any-name config-root="true">` is a transparent root: its
//!   children are the document's properties and its attributes are editor
//!   metadata, never data or validated. It must be the only top-level element.
//! - Without the marker, every top-level element is a property:
//!   `<listen>1</listen><port>2</port>` is `{"listen": 1, "port": 2}`, and a
//!   lone `<config>…</config>` is `{"config": …}`. The editor never adds a root.
//! - Child elements and attributes are both object properties; element text
//!   is a typed scalar (`<listen>7835</listen>` is `7835`); repeated siblings
//!   fold into an array; `#text` appears only for mixed content.
//! - `Ctrl+/` toggles `<!-- -->` comments.

mod tokens;
mod scan;
mod plan;
mod parse;
mod suggestions;
mod hover;
mod editing;
mod typed;
mod convert;
mod text;
mod one_line;
mod text_bodies;
mod spelling;
mod tag_spelling;
mod names;
mod attribute_values;
mod entities;
mod namespaces;
mod prefixed;
mod attribute_order;
mod markup;
mod declarations;
// The fold ranges (the `folding` feature).
#[cfg(feature = "folding")]
mod folding;

use tokens::*;
use scan::*;
use plan::*;
use parse::*;
use suggestions::*;
use hover::*;
use editing::*;
use convert::*;
use text_bodies::*;

use super::super::commands::indent_unit;
use super::super::completion::{child_fill, kept_const, kept_default, array_branch, object_branch};
use super::super::highlight::{MultilineConstruct, Token as HighlightToken, TokenKind, Tokens};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{Scalars, OneLine, CompletionContext, Newline, Repetition, EntryPosition, InlineBlock, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError,
    SchemaPathSegment, inline_block,
};
use quick_xml::XmlVersion;
use quick_xml::events::attributes::AttrError;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

pub(super) struct XmlBehavior;

/// Whether `value` holds a key with a prefix (`x:a`) anywhere.
fn has_prefixed_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| key.contains(':') || has_prefixed_key(child)),
        Value::Array(items) => items.iter().any(has_prefixed_key),
        _ => false,
    }
}


/// An XML scalar edit (see `ConfigLanguage::edits_one_scalar`): an
/// `<name>text</name>` line read as a document of its own; the edit adds
/// and removes no markup or reference, and no `--`, `]]>` or `?>` (which
/// would end or break a comment, a CDATA section or an instruction over
/// lines).
const SCALAR_LINE: super::scalar_lines::LineRules = super::scalar_lines::LineRules {
    alone: super::scalar_lines::dedented,
    forbidden: &['<', '>', '&'],
    closers: &["--", "]]>", "?>"],
};

impl ConfigLanguage for XmlBehavior {
    fn forget_kept(&self) {
        forget_scan();
        forget_spans();
        forget_tokens();
        forget_plan();
    }

    fn edits_one_scalar(&self, source: &str, range: std::ops::Range<usize>, inserted: &str) -> bool {
        super::scalar_lines::one_scalar_line(self, source, range, inserted, &SCALAR_LINE).is_some()
    }

    fn repetition(&self) -> Repetition {
        Repetition::Elements
    }

    fn reindent_on_type(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        // `</name` alone on its line, finished by `>`: it closes the element
        // one level out.
        if typed != '>' {
            return None;
        }
        let byte = utf16_to_byte_index(source, cursor);
        let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
        let line = &source[line_start..byte];
        let content = line.trim_start();
        let indent = &line[..line.len() - content.len()];
        let closes = content.starts_with("</")
            && content[2..].chars().all(|character| character.is_alphanumeric() || matches!(character, '_' | '-' | '.' | ':'));
        if !closes {
            return None;
        }
        // Lined up with the start tag it closes; without one in sight, one
        // level out from this line.
        let target = open_element_indent(&source[..line_start], &content[2..]).unwrap_or_else(|| {
            let unit = indent_unit(source);
            indent.strip_suffix(unit.as_str()).unwrap_or("").to_string()
        });
        (target != indent).then_some(target)
    }

    fn auto_close(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        match typed {
            '>' => end_tag_for(source, cursor),
            '"' | '\'' => typed::attribute_quote(source, cursor, typed),
            _ => None,
        }
    }

    fn complete_on_type(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        (typed == '/').then(|| typed::end_tag_after_slash(source, cursor)).flatten()
    }

    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "XML",
            icon: "X",
            extensions: &["xml"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        xml_parse(source).map(super::super::codec::non_finite::escape_strings)
    }

    /// An attribute read `a` (no child of its name now) where the value
    /// written spells it `@a`.
    fn spelled_like(&self, source: &str, read: Value, like: &Value) -> Value {
        spelling::spelled_like(source, read, like)
    }

    /// An element written once, at any depth (several top-level elements
    /// are a list too): never an attribute, its text or an item of
    /// repeated elements.
    fn lone_item(&self, source: &str, pointer: &str) -> bool {
        xml_lone_element(source.strip_prefix('\u{FEFF}').unwrap_or(source), pointer)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        super::super::byte_order_mark::span(source, |text| xml_name_span(text, pointer).map(Span::from))
    }

    /// Element text ending in a line break before its end tag
    /// (`k2\n</secret>`) runs through the break.
    fn kept_value_end(&self, source: &str, end: usize) -> usize {
        let rest = &source[end..];
        if rest.trim_start().starts_with("</") { end + rest.len() - rest.trim_start().len() } else { end }
    }

    #[cfg(feature = "folding")]
    fn fold_ranges(&self, source: &str) -> Vec<super::super::language::FoldRange> {
        folding::xml_fold_ranges(source)
    }

    /// Lines inside element text written over several lines.
    fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        text_body_lines(source)
    }

    fn comments(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        comments(source)
    }

    /// Start tags with attributes over lines, attribute values and CDATA
    /// text over lines.
    fn multiline_constructs(&self, source: &str) -> Vec<MultilineConstruct> {
        multiline_constructs(source)
    }

    /// The inside of a start tag: its attributes.
    fn highlight_continued(&self, text: &str) -> Vec<HighlightToken> {
        highlight_continued(text)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        super::super::byte_order_mark::span(source, |text| xml_value_span(text, pointer).map(Span::from))
    }

    fn first_span(&self, source: &str) -> Option<Span> {
        super::super::byte_order_mark::span(source, |text| first_content_span(text).map(Span::from))
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
        existing_elements(source, cursor_byte)
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

    /// Comments, and processing instructions (`<?pi …?>`, not the XML
    /// declaration), which no other language shows either: a save keeps
    /// both.
    fn has_comments(&self, source: &str) -> bool {
        source.contains("<!--") || self.has_processing_instructions(source)
    }

    fn has_processing_instructions(&self, source: &str) -> bool {
        source
            .match_indices("<?")
            .any(|(at, _)| !source[at..].starts_with("<?xml ") && !source[at..].starts_with("<?xml?"))
    }

    fn mixed_content(&self, source: &str) -> Option<String> {
        parse::mixed_content(source)
    }

    fn scalars(&self) -> Scalars {
        Scalars::Text
    }

    /// Every attribute value and element text exactly as written.
    fn parse_text(&self, source: &str) -> Result<Value, ParseError> {
        xml_parse_as(source, Reading::Text).map(super::super::codec::non_finite::escape_strings)
    }

    /// Numbers that write back unchanged, `true`, `false` and `null`.
    fn guess_scalar(&self, text: &str) -> Value {
        typed_scalar(text)
    }

    fn unconvertible(&self, source: &str) -> Option<String> {
        namespace_clash(source)
            .or_else(|| namespaces::unbound_prefix(source))
            .or_else(|| entities::declared_entity(source))
    }

    fn merged_keys(&self, source: &str) -> Option<String> {
        namespace_clash(source)
    }

    fn format_value(&self, value: &Value) -> String {
        xml_text(value)
    }

    /// Escaped for where it goes: a quoted attribute value keeps its quotes,
    /// element text escapes `&` and `<`.
    fn format_value_like(&self, value: &Value, written: &str) -> String {
        let text = xml_text(value);
        match written.chars().next() {
            Some(quote @ ('"' | '\'')) if written.len() >= 2 && written.ends_with(quote) => {
                let escape = |text: &str| {
                    let escaped = escape_attribute(text);
                    if quote == '\'' { escaped.replace('\'', "&apos;") } else { escaped }
                };
                let inner = &written[1..written.len() - 1];
                format!("{quote}{}{quote}", text::attribute_like(&text, inner, &escape))
            }
            _ => text::element_text_like(&text, written),
        }
    }

    /// Text however it is written: CDATA markers and entity references as
    /// the characters they stand for (`<![CDATA[a < b]]>` is `a &lt; b`).
    fn comparable_line<'a>(&self, code: &'a str) -> std::borrow::Cow<'a, str> {
        text::comparable_text(code)
    }

    fn written_value_range(&self, source: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        text::written_text_range(source, range)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        hover_name(line, column)
    }

    fn hover_path(&self, source: &str, line_index: usize, column: usize) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        element_hover_path(source, line_index, column)
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        // A line can hold a whole subtree; resolve just past the hovered name.
        hover_context_end(line, column)
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    fn newline(&self, source: &str, cursor: usize) -> Newline {
        let (text, caret) = newline_insertion(source, cursor);
        Newline::with_caret(text, caret)
    }

    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        toggle_xml_comments(source, selection_start, selection_end)
    }

    fn whole_constructs(&self, source: &str, start: usize, end: usize) -> (usize, usize) {
        editing::over_balanced_xml_lines(source, start, end)
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = xml_lexical_state(source, cursor);
        Lexical { quote, in_comment }
    }

    fn entry_started(&self, source: &str, cursor: usize) -> bool {
        completion_context(source, cursor).explicit_entry_start
    }


    /// Object keys become elements and arrays repeated siblings. The output is
    /// rootless unless `previous` had a config root, which is kept along with
    /// its declaration.
    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        if let Some(pointer) = empty_key(value, String::new()) {
            return Err(format!("an empty key cannot be an XML element name (at {pointer})"));
        }
        if let Some((pointer, character)) = first_non_xml_char(value, "") {
            return Err(format!("{} (at {pointer})", not_xml_char_message(character)));
        }
        // A character XML holds, in a key that is no name (`/*`).
        if let Some(reason) = names::first_non_name(value) {
            return Err(reason);
        }
        // Namespace declarations and `xsi:*` are no data (see
        // `declarations`): a save keeps the host's as written.
        if let Some(reason) = declarations::first_markup_key(value, previous) {
            return Err(reason);
        }
        let framing = previous.map(xml_framing).unwrap_or_default();
        let attributes = previous.map(attribute_paths).unwrap_or_default();
        let old = previous.filter(|_| !attributes.is_empty()).and_then(|previous| xml_parse(previous).ok());
        if let Some(reason) = attribute_values::first_container_attribute(value, old.as_ref(), &attributes) {
            return Err(reason);
        }
        // An element renamed to a key with a prefix would be an attribute.
        if has_prefixed_key(value) {
            let parsed = old.is_none().then(|| previous.and_then(|previous| xml_parse(previous).ok())).flatten();
            if let Some(old) = old.as_ref().or(parsed.as_ref())
                && let Some(reason) = prefixed::renamed_element(value, old, &attributes)
            {
                return Err(reason);
            }
        }
        let Value::Object(map) = &with_attributes(value, &attributes, &mut Vec::new()) else {
            return Err("the XML document model is an object of elements".to_string());
        };
        let mut out = String::new();
        if let Some(declaration) = &framing.declaration {
            out.push_str(declaration);
            out.push('\n');
        }
        match &framing.root {
            Some(root) => {
                // The marker is an attribute (`@` prefix for quick-xml).
                let mut wrapped = serde_json::Map::new();
                wrapped.insert(format!("@{CONFIG_ROOT_ATTRIBUTE}"), Value::Bool(true));
                wrapped.extend(map.clone());
                write_element(&mut out, root, &Value::Object(wrapped))?;
            }
            None => {
                for (key, child) in map {
                    write_element(&mut out, key, child)?;
                }
            }
        }
        // Empty elements the host closes with an end tag (`<x a="1"></x>`)
        // are written that way again, not as `<x a="1"/>`, and each tag is
        // spelled as the host spells it (`<x a='1' />`).
        Ok(match previous {
            Some(previous) => declarations::kept(
                &tag_spelling::with_tag_spelling_of(
                    &one_line::with_one_line_elements_of(&with_end_tags_of(&out, previous), previous),
                    previous,
                ),
                previous,
            ),
            None => out,
        })
    }

    fn comment_start(&self, line: &str) -> Option<usize> {
        line.find("<!--")
    }

    /// An attribute name followed by its `="…"`.
    fn value_follows_key(&self, rest: &str) -> bool {
        rest.trim_start().starts_with('=')
    }

    /// Inside empty elements (`<k>|</k>`).
    fn empty_value_slots(&self, text: &str) -> Vec<usize> {
        text.match_indices("></").map(|(at, _)| at + 1).collect()
    }

    fn key_renames(&self, source: &str, pointer: &str, new_key: &str) -> Option<Vec<crate::editor::language::KeyEdit>> {
        element_renames(source, pointer, new_key)
    }

    /// An XML name (`a.b`, `café`): a letter or `_` first, then letters,
    /// digits, `_`, `-` and `.`; for an attribute, one with a prefix too
    /// (`x:attr`; the model keys elements by their local name, so an
    /// element's is never renamed to one).
    fn written_key(&self, source: &str, pointer: &str, key: &str) -> Option<String> {
        let name = |part: &str| {
            let mut characters = part.chars();
            characters.next().is_some_and(|first| first.is_alphabetic() || first == '_')
                && characters.all(|character| character.is_alphanumeric() || matches!(character, '_' | '-' | '.'))
        };
        let written = match key.split_once(':') {
            Some((prefix, local)) => name(prefix) && name(local) && is_attribute(source, pointer),
            None => name(key),
        };
        written.then(|| key.to_string())
    }

    fn members_in_order(&self, source: &str, pointer: &str, order: &[String]) -> Option<String> {
        attribute_order::in_order(source, pointer, order)
    }

    fn entries_apart(&self, source: &str) -> Option<String> {
        one_line::children_apart(source)
    }

    fn entries_together(&self, patched: &str, previous: &str) -> String {
        one_line::with_one_line_elements_of(patched, previous)
    }

    fn one_line_body(&self, value: &Value, _indent: &str, _unit: &str) -> Option<String> {
        one_line_children(value)
    }

    fn one_line_container(&self, source: &str, pointer: &str) -> Option<OneLine> {
        one_line_element(source, pointer).map(|range| OneLine { range: range.clone(), written: String::new(), value: range })
    }

    /// An end tag, `</name>`.
    /// Attribute values are quoted; a backslash in them is text.
    fn config(&self) -> super::super::language::LanguageConfig {
        super::super::language::LanguageConfig {
            quotes: &["\"", "'"],
            literal_quotes: &["\"", "'"],
            ..Default::default()
        }
    }

    /// A required attribute is listed `@a`.
    fn required_property<'a>(&self, name: &'a str) -> &'a str {
        name.strip_prefix('@').unwrap_or(name)
    }

    fn text_key(&self) -> Option<&'static str> {
        Some("#text")
    }

    fn markup_delimiters(&self) -> Option<(char, char)> {
        Some(('<', '>'))
    }

    fn closing_markup_len(&self, text: &str) -> Option<usize> {
        markup::end_tag_len(text)
    }

    fn key_markup_len(&self, text: &str) -> Option<usize> {
        markup::name_markup_len(text)
    }

    fn element_end(&self, text: &str, open: usize) -> Option<usize> {
        markup::element_end(text, open)
    }

    fn unclosed_containers(&self, text: &str) -> usize {
        markup::open_elements(text)
    }

    fn container_closer_len(&self, text: &str) -> Option<usize> {
        markup::closer_len(text)
    }

    fn content_texts(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        markup::element_texts(text)
    }

    fn space_is_content(&self, before: &str, after: &str) -> bool {
        markup::space_is_content(before, after)
    }

    fn writes_container_on_line(&self, line: &str) -> bool {
        line.matches('<').count() >= 4
    }

    fn opens_list_item(&self, lines: &[&str], index: usize) -> bool {
        let comment_start = |line: &str| self.comment_start(line).unwrap_or(line.len());
        markup::opens_repeated_element(&comment_start, lines, index)
    }
}

/// The pointer of the first object holding an empty key, if any.
fn empty_key(value: &Value, pointer: String) -> Option<String> {
    match value {
        Value::Object(map) => {
            if map.contains_key("") {
                return Some(if pointer.is_empty() { "/".to_string() } else { pointer });
            }
            map.iter()
                .find_map(|(key, child)| empty_key(child, format!("{pointer}/{key}")))
        }
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, item)| empty_key(item, format!("{pointer}/{index}"))),
        _ => None,
    }
}


#[cfg(test)]
mod tests;
#[cfg(test)]
mod existing_tests;
