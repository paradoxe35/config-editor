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
//!   lone `<burrow>…</burrow>` is `{"burrow": …}`. The editor never adds a root.
//! - Child elements and attributes are both object properties; element text
//!   is a typed scalar (`<listen>7835</listen>` is `7835`); repeated siblings
//!   fold into an array; `#text` appears only for mixed content.
//! - `Ctrl+/` toggles `<!-- -->` comments.

mod tokens;
mod scan;
mod parse;
mod suggestions;
mod hover;
mod editing;
mod convert;
mod text;
// The fold ranges (the `folding` feature).
#[cfg(feature = "folding")]
mod folding;

use tokens::*;
use scan::*;
use parse::*;
use suggestions::*;
use hover::*;
use editing::*;
use convert::*;

use super::super::commands::indent_unit;
use super::super::completion::{array_branch, object_branch, schema_const, schema_default, schema_example};
use super::super::highlight::{Token as HighlightToken, TokenKind, Tokens};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{OneLine, CompletionContext, Newline, Repetition, EntryPosition, InlineBlock, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError,
    SchemaPathSegment, inline_block,
};
use quick_xml::XmlVersion;
use quick_xml::events::attributes::AttrError;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

pub(super) struct XmlBehavior;


impl ConfigLanguage for XmlBehavior {
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
        (typed == '>').then(|| end_tag_for(source, cursor)).flatten()
    }

    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "XML",
            icon: "X",
            extensions: &["xml"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        xml_parse(source)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        xml_name_span(source, pointer).map(Span::from)
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

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        xml_value_span(source, pointer).map(Span::from)
    }

    fn first_span(&self, source: &str) -> Option<Span> {
        first_content_span(source).map(Span::from)
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

    fn mixed_content(&self, source: &str) -> Option<String> {
        parse::mixed_content(source)
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
        let framing = previous.map(xml_framing).unwrap_or_default();
        let attributes = previous.map(attribute_paths).unwrap_or_default();
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
        // are written that way again, not as `<x a="1"/>`.
        Ok(match previous {
            Some(previous) => with_end_tags_of(&out, previous),
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

    fn rename_key(&self, source: &str, pointer: &str, new_key: &str) -> Option<String> {
        rename_element(source, pointer, new_key)
    }

    fn one_line_body(&self, value: &Value, _indent: &str, _unit: &str) -> Option<String> {
        one_line_children(value)
    }

    fn one_line_container(&self, source: &str, pointer: &str) -> Option<OneLine> {
        one_line_element(source, pointer).map(|range| OneLine { range: range.clone(), written: String::new(), value: range })
    }

    /// An end tag, `</name>`.
    fn closing_markup_len(&self, text: &str) -> Option<usize> {
        if !text.starts_with("</") {
            return None;
        }
        text.find('>').map(|end| end + 1)
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
