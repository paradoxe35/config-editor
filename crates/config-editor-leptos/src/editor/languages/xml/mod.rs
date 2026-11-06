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

use tokens::*;
use scan::*;
use parse::*;
use suggestions::*;
use hover::*;
use editing::*;
use convert::*;

use super::super::commands::indent_unit;
use super::super::completion::{array_branch, object_branch, schema_const, schema_default};
use super::super::highlight::{Token as HighlightToken, TokenKind, Tokens};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{CompletionContext, Newline, Repetition, EntryPosition, InlineBlock, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError,
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
        if !closes || indent.is_empty() {
            return None;
        }
        let unit = indent_unit(source);
        Some(indent.strip_suffix(unit.as_str()).unwrap_or("").to_string())
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

    fn format_value(&self, value: &Value) -> String {
        xml_text(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        hover_name(line, column)
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
        let Value::Object(map) = value else {
            return Err("the XML document model is an object of elements".to_string());
        };
        if let Some(pointer) = empty_key(value, String::new()) {
            return Err(format!("an empty key cannot be an XML element name (at {pointer})"));
        }
        let framing = previous.map(xml_framing).unwrap_or_default();
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
        Ok(out)
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
