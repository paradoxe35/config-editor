//! XML text-layer behavior.
//!
//! Decisions: duplicate sibling elements and duplicate attributes on one
//! element are a parse error, as in JSON (D2); comments (`<!-- -->`) are
//! supported for toggling, unlike JSON (Ctrl+/ toggles XML comments).
//!
//! Strict validation comes from `quick-xml` (mismatched tags, unclosed tags,
//! stray text). Structure (element paths, name and value spans, the container
//! under the caret) comes from a tolerant token scan that keeps working while
//! the document is invalid mid-typing, which is the normal state while the
//! user is completing a tag name.
//!
//! Document model: an element's children form a JSON object; `@`-prefixed keys
//! hold attributes; `#text` holds element text. For example,
//! `<root><item id="1">x</item></root>` parses to
//! `{"root": {"item": {"@id": "1", "#text": "x"}}}`. Repeated sibling elements
//! are rejected rather than folded into an array, so a schema array has no XML
//! counterpart here.
//!
//! NOTE: a host dependency on `quick-xml = "0.42.0"` is required (see the crate
//! `Cargo.toml`): this file uses only its `Reader` event API.

use super::super::commands::indent_unit;
use super::super::completion::{array_branch, object_branch, schema_const, schema_default};
use super::super::keyboard::utf16_to_byte_index;
#[cfg(feature = "schema-validation")]
use super::super::language::pointer_segments;
use super::super::language::{CompletionContext, LanguageBehavior, ParseError, SchemaPathSegment};
use leptos::prelude::*;
use quick_xml::XmlVersion;
use quick_xml::events::attributes::AttrError;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use serde_json::{Map, Value};
use std::collections::HashSet;

/// XML implementation of the shared text-layer trait.
pub(super) struct XmlBehavior;

impl LanguageBehavior for XmlBehavior {
    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        xml_parse(source)
    }

    #[cfg(feature = "schema-validation")]
    fn span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)> {
        xml_name_span(source, path)
    }

    #[cfg(feature = "schema-validation")]
    fn value_span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)> {
        xml_value_span(source, path)
    }

    #[cfg(feature = "schema-validation")]
    fn first_span(&self, source: &str) -> Option<(usize, usize, usize)> {
        first_content_span(source)
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
        base_indent: &str,
        unit: &str,
    ) -> Option<(String, usize)> {
        property_insert(key, property, root, base_indent, unit)
    }

    fn format_value(&self, value: &Value) -> String {
        xml_text(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        hover_name(line, column)
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        // One line can hold a whole `<parent><child>text</child></parent>` tree:
        // read the context just after the hovered name so nested elements
        // resolve below the right path instead of at the line end.
        hover_context_end(line, column)
    }

    fn highlight_line(&self, line: &str) -> AnyView {
        highlight_line(line)
    }

    fn newline_with_indent(&self, source: &str, cursor: usize) -> String {
        newline_with_indent(source, cursor)
    }

    fn newline_insertion(&self, source: &str, cursor: usize) -> (String, usize) {
        newline_insertion(source, cursor)
    }

    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        toggle_xml_comments(source, selection_start, selection_end)
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> (Option<char>, bool) {
        xml_lexical_state(source, cursor)
    }

    fn entry_started(&self, source: &str, cursor: usize) -> bool {
        completion_context(source, cursor).explicit_entry_start
    }

    fn confidence(&self, source: &str) -> u8 {
        xml_confidence(source)
    }
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `<name ...`, `</name`, or `<>`: `name` is empty for a bare `<`.
    OpenTag { closing: bool, name_end: usize },
    /// `</name>` written as one token is still recorded separately so the
    /// scan can match it against the open stack.
    EndTag,
    /// Tag, attribute, or element text outside any tag.
    Text { quoted: bool },
    /// `=` between an attribute name and its value.
    Equals,
    /// `<!-- ... -->` (possibly unterminated while typing).
    Comment,
    /// `<? ... ?>`, `<!DOCTYPE ...>`, `<![CDATA[ ... ]]>`.
    Special,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    start: usize,
    end: usize,
    kind: Kind,
}

/// Splits `source` into XML tokens. Never fails: an unclosed tag, quote, or
/// comment ends at the input end. Comments stay whole so `<!-- <a> -->` never
/// feeds markup to the scan; other `<!...>`/`<?...?>` markup is a `Special`
/// the scan ignores.
fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < source.len() {
        let rest = &source[index..];
        let character = rest.chars().next().unwrap_or('\0');
        if character == '<' {
            let tag_start = index;
            if let Some(token) = tokenize_markup(source, bytes, tag_start) {
                index = token.end;
                tokens.push(token);
            } else {
                // A bare `<` at the very end opens a tag that has no name yet.
                tokens.push(Token {
                    start: tag_start,
                    end: source.len(),
                    kind: Kind::OpenTag {
                        closing: false,
                        name_end: source.len(),
                    },
                });
                index = source.len();
            }
        } else if character.is_whitespace() {
            index += character.len_utf8();
        } else {
            let start = index;
            while index < source.len() {
                let next = source[index..].chars().next().unwrap_or('\0');
                if next == '<' || next.is_whitespace() {
                    break;
                }
                index += next.len_utf8();
            }
            tokens.push(Token {
                start,
                end: index,
                kind: Kind::Text { quoted: false },
            });
        }
    }
    tokens
}

/// Tokenizes one `<...>` sequence starting at `tag_start`. Returns `None` for
/// a bare `<` at the end of input.
fn tokenize_markup(source: &str, bytes: &[u8], tag_start: usize) -> Option<Token> {
    let index = tag_start + 1;
    if bytes.get(index) == Some(&b'/') {
        return Some(end_tag_token(source, tag_start, index + 1));
    }
    if source[index..].starts_with("!--") {
        let end = source[index..]
            .find("-->")
            .map_or(source.len(), |relative| index + relative + 3);
        return Some(Token {
            start: tag_start,
            end,
            kind: Kind::Comment,
        });
    }
    if source[index..].starts_with("![CDATA[") {
        let end = source[index..]
            .find("]]>")
            .map_or(source.len(), |relative| index + relative + 3);
        return Some(Token {
            start: tag_start,
            end,
            kind: Kind::Special,
        });
    }
    if bytes.get(index) == Some(&b'?') || bytes.get(index) == Some(&b'!') {
        let end = find_markup_end(source, index + 1);
        return Some(Token {
            start: tag_start,
            end,
            kind: Kind::Special,
        });
    }
    if index >= source.len() {
        return None;
    }
    // Open tag: the name runs to the first whitespace, `/`, `>`, `=`, or quote.
    let mut name_end = index;
    while name_end < source.len() {
        let next = source[name_end..].chars().next().unwrap_or('\0');
        if next.is_whitespace() || matches!(next, '/' | '>' | '=' | '"' | '\'') {
            break;
        }
        name_end += next.len_utf8();
    }
    Some(Token {
        start: tag_start,
        end: find_markup_end(source, name_end),
        kind: Kind::OpenTag {
            closing: false,
            name_end,
        },
    })
}

/// Tokenizes `</...>` starting just after the `/`, through its `>` if any.
fn end_tag_token(source: &str, tag_start: usize, after_slash: usize) -> Token {
    let mut name_end = after_slash;
    while name_end < source.len() {
        let next = source[name_end..].chars().next().unwrap_or('\0');
        if next.is_whitespace() || matches!(next, '/' | '>') {
            break;
        }
        name_end += next.len_utf8();
    }
    let end = find_markup_end(source, name_end);
    Token {
        start: tag_start,
        end,
        kind: Kind::EndTag,
    }
}

/// End of a `<...>` sequence: the next `>` byte, or the input end when the tag
/// is still being typed.
fn find_markup_end(source: &str, from: usize) -> usize {
    source[from..]
        .find('>')
        .map_or(source.len(), |relative| from + relative + 1)
}

/// Splits the inside of an open tag (`<name ...`) into attribute tokens:
/// names and quoted values are `Text`, the `=` between them is `Equals`.
/// Quotes may be unterminated mid-typing; a quote then runs to the tag end.
fn tokenize_tag_inside(source: &str, open: &Token) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut index = match open.kind {
        Kind::OpenTag { name_end, .. } => name_end,
        _ => open.start,
    };
    while index < open.end {
        let character = source[index..].chars().next().unwrap_or('\0');
        if character.is_whitespace() || character == '/' {
            index += character.len_utf8();
        } else if character == '=' {
            tokens.push(Token {
                start: index,
                end: index + 1,
                kind: Kind::Equals,
            });
            index += 1;
        } else if character == '"' || character == '\'' {
            let start = index;
            let mut end = source.len().min(open.end);
            let mut cursor = index + 1;
            while cursor < open.end {
                if source[cursor..].starts_with(character) {
                    end = cursor + 1;
                    break;
                }
                cursor += source[cursor..].chars().next().map_or(1, char::len_utf8);
            }
            tokens.push(Token {
                start,
                end,
                kind: Kind::Text { quoted: true },
            });
            index = end;
        } else {
            let start = index;
            while index < open.end {
                let next = source[index..].chars().next().unwrap_or('\0');
                if next.is_whitespace() || matches!(next, '=' | '/' | '"' | '\'') {
                    break;
                }
                index += next.len_utf8();
            }
            tokens.push(Token {
                start,
                end: index,
                kind: Kind::Text { quoted: false },
            });
        }
    }
    tokens
}

/// The local name of the tag opened or closed at `token` (`<a:b>` yields
/// `b`). Attribute tokens and comments yield an empty name.
fn token_name(source: &str, token: &Token) -> String {
    match token.kind {
        Kind::OpenTag { name_end, .. } => {
            let mut name = source[token.start + 1..name_end].to_string();
            if let Some(colon) = name.rfind(':') {
                name = name[colon + 1..].to_string();
            }
            name
        }
        Kind::EndTag => {
            let mut name = source[token.start + 2..]
                .split(['>', '/', '"', '\'', '=', '\t', '\n', '\r', ' '])
                .next()
                .unwrap_or("")
                .to_string();
            if let Some(colon) = name.rfind(':') {
                name = name[colon + 1..].to_string();
            }
            name
        }
        _ => String::new(),
    }
}

/// Decodes one attribute value or text fragment: entities (`&lt;`, `&#65;`)
/// resolve, unknown entities stay literal so completion keeps working.
fn decode_entities(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(semi) = after.find(';') else {
            decoded.push_str(&rest[start..]);
            break;
        };
        let entity = &after[..semi];
        if let Some(character) = named_entity(entity) {
            decoded.push(character);
        } else if let Some(character) = numeric_entity(entity) {
            decoded.push(character);
        } else {
            decoded.push_str(&rest[start..start + 1 + semi + 1]);
        }
        rest = &after[semi + 1..];
    }
    decoded.push_str(rest);
    decoded
}

fn named_entity(entity: &str) -> Option<char> {
    match entity {
        "lt" => Some('<'),
        "gt" => Some('>'),
        "amp" => Some('&'),
        "apos" => Some('\''),
        "quot" => Some('"'),
        _ => None,
    }
}

fn numeric_entity(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix('#')?;
    let code = if let Some(hex) = digits
        .strip_prefix('x')
        .or_else(|| digits.strip_prefix('X'))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        digits.parse::<u32>().ok()?
    };
    char::from_u32(code)
}

// ---------------------------------------------------------------------------
// Structure scan
// ---------------------------------------------------------------------------

/// One step of an element path: a child element or one repeated sibling.
#[derive(Clone, Debug)]
enum Seg {
    Key(String),
    Index(usize),
}

/// An open element: its schema path, how often each child name appeared in
/// this element so far, and the id other tables key on.
#[derive(Clone, Debug)]
struct Frame {
    id: usize,
    path: Vec<Seg>,
    child_counts: Vec<(String, usize)>,
}

/// Parser state at the caret.
struct Snapshot {
    /// Path of the element holding the caret.
    path: Vec<Seg>,
    /// The innermost open element, if the caret is inside one.
    frame: Option<Frame>,
    /// The name or text being typed under the caret, if any.
    partial: Option<Token>,
    /// Whether the caret sits inside a tag (`<...>`) rather than in text.
    in_tag: bool,
    /// The open tag the caret is inside, if any.
    open_tag: Option<Token>,
    /// Tokens inside that open tag, if any.
    inner: Vec<Token>,
    /// The attribute awaiting a value (`name = |`), if any.
    pending_attr: Option<String>,
}

#[derive(Default)]
struct Scan {
    snapshot: Option<Snapshot>,
    /// Completed sibling names as `(owner frame id, name, start byte)`. The
    /// name being typed at the caret is excluded.
    keys: Vec<(usize, String, usize)>,
    /// Byte spans of element and attribute names, keyed by document path.
    /// Attributes live under `@name`; repeated siblings under `name/index`.
    #[cfg_attr(not(feature = "schema-validation"), allow(dead_code))]
    key_spans: Vec<(Vec<String>, usize, usize)>,
    /// Byte spans of element text and attribute values, keyed the same way.
    #[cfg_attr(not(feature = "schema-validation"), allow(dead_code))]
    value_spans: Vec<(Vec<String>, usize, usize)>,
    /// Attribute names per element, keyed by document path.
    #[cfg_attr(not(feature = "schema-validation"), allow(dead_code))]
    attr_names: Vec<(Vec<String>, usize, usize)>,
}

fn strings(path: &[Seg]) -> Vec<String> {
    path.iter()
        .map(|segment| match segment {
            Seg::Key(key) => key.clone(),
            Seg::Index(index) => index.to_string(),
        })
        .collect()
}

fn extend(path: &[Seg], segment: Seg) -> Vec<Seg> {
    let mut extended = path.to_vec();
    extended.push(segment);
    extended
}

/// Child path for the next `<name>` inside `parent`: the first child keeps the
/// bare name, later repeated siblings are numbered (`item`, `item/1`, ...).
/// The count table lives on the frame so counts reset per parent element.
fn child_path(frame: &mut Frame, name: &str) -> Vec<Seg> {
    let seen = match frame
        .child_counts
        .iter_mut()
        .find(|(child, _)| child == name)
    {
        Some((_, count)) => {
            let seen = *count;
            *count += 1;
            seen
        }
        None => {
            frame.child_counts.push((name.to_string(), 1));
            0
        }
    };
    if seen == 0 {
        extend(&frame.path, Seg::Key(name.to_string()))
    } else {
        extend(
            &extend(&frame.path, Seg::Key(name.to_string())),
            Seg::Index(seen),
        )
    }
}

/// Walks the whole document, tolerating invalid syntax, and captures the parser
/// state at `cursor` (a byte offset).
fn analyze(source: &str, cursor: usize) -> Scan {
    let tokens = tokenize(source);
    let partial_index = tokens.iter().position(|token| {
        contains_caret(source, token, cursor)
            && !matches!(token.kind, Kind::Special | Kind::Comment)
    });
    let mut frames: Vec<Frame> = Vec::new();
    let mut next_id = 0usize;
    let mut scan = Scan::default();

    // Records a `<name>` start tag: its name span, the sibling key used for
    // duplicate reporting, and the new open frame (`empty` skips the frame for
    // `<name/>`).
    let mut open_element = |scan: &mut Scan,
                            frames: &mut Vec<Frame>,
                            name: &str,
                            tag: &Token,
                            is_partial: bool,
                            empty: bool| {
        next_id += 1;
        let path = match frames.last_mut() {
            Some(frame) => child_path(frame, name),
            None => vec![Seg::Key(name.to_string())],
        };
        scan.key_spans.push((
            strings(&path),
            tag.start + 1,
            tag.start + 1 + name_len(source, tag),
        ));
        if !is_partial && let Some(frame) = frames.last() {
            scan.keys.push((frame.id, name.to_string(), tag.start + 1));
        }
        if !empty {
            frames.push(Frame {
                id: next_id,
                path,
                child_counts: Vec::new(),
            });
        }
    };

    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let is_partial = partial_index == Some(index);
        if scan.snapshot.is_none() && (is_partial || token.start >= cursor) {
            scan.snapshot = Some(capture_snapshot(source, &tokens, &frames, cursor));
        }
        match token.kind {
            Kind::OpenTag { .. } => {
                let self_closing = source[..token.end].ends_with("/>");
                let name = token_name(source, &token);
                // A bare `<` (or `< >`) opens nothing; its attributes, if any,
                // still belong to no element.
                if !name.is_empty() {
                    // Attribute names belong to the path this element will get.
                    let path = match frames.last_mut() {
                        Some(frame) => child_path_preview(frame, &name),
                        None => vec![Seg::Key(name.clone())],
                    };
                    for inner in tokenize_tag_inside(source, &token) {
                        if let Kind::Text { quoted: false } = inner.kind
                            && is_attr_name(source, &token, inner)
                            && !is_partial
                        {
                            scan.attr_names
                                .push((strings(&path), inner.start, inner.end));
                            scan.key_spans.push((
                                attr_key(&path, &source[inner.start..inner.end]),
                                inner.start,
                                inner.end,
                            ));
                        }
                    }
                    open_element(
                        &mut scan,
                        &mut frames,
                        &name,
                        &token,
                        is_partial,
                        self_closing,
                    );
                }
            }
            Kind::EndTag => {
                let name = token_name(source, &token);
                if frames
                    .last()
                    .is_some_and(|frame| last_name(&frame.path) == name)
                {
                    frames.pop();
                } else if let Some(position) = frames
                    .iter()
                    .rposition(|frame| last_name(&frame.path) == name)
                {
                    frames.truncate(position);
                }
            }
            Kind::Text { quoted } => {
                if quoted {
                    // Quoted attribute values are handled with their `=` below.
                } else if !is_partial {
                    let text = source[token.start..token.end].trim();
                    if !text.is_empty()
                        && let Some(frame) = frames.last()
                    {
                        scan.value_spans
                            .push((text_key(&frame.path), token.start, token.end));
                    }
                }
            }
            Kind::Comment | Kind::Special | Kind::Equals => {}
        }
        index += 1;
    }

    // Attribute values: re-walk open tags pairing `name = "value"`.
    record_attribute_values(source, &tokens, &mut scan);

    if scan.snapshot.is_none() {
        scan.snapshot = Some(capture_snapshot(source, &tokens, &frames, cursor));
    }
    scan
}

/// Byte length of the tag name: `<prefix:name` counts only after the colon.
fn name_len(source: &str, tag: &Token) -> usize {
    let name_end = match tag.kind {
        Kind::OpenTag { name_end, .. } => name_end,
        _ => tag.end,
    };
    let name_start = tag.start + 1;
    let full = &source[name_start..name_end.min(source.len())];
    full.rfind(':')
        .map_or(full.len(), |colon| full.len() - colon - 1)
}

fn last_name(path: &[Seg]) -> String {
    path.iter()
        .rev()
        .find_map(|segment| match segment {
            Seg::Key(key) => Some(key.clone()),
            Seg::Index(_) => None,
        })
        .unwrap_or_default()
}

fn text_key(path: &[Seg]) -> Vec<String> {
    let mut key = strings(path);
    key.push("#text".to_string());
    key
}

fn attr_key(path: &[Seg], name: &str) -> Vec<String> {
    let mut key = strings(path);
    key.push(format!("@{name}"));
    key
}

/// Preview of the path a child `<name>` would get, without advancing counts.
fn child_path_preview(frame: &Frame, name: &str) -> Vec<Seg> {
    let seen = frame
        .child_counts
        .iter()
        .find(|(child, _)| child == name)
        .map_or(0, |(_, count)| *count);
    if seen == 0 {
        extend(&frame.path, Seg::Key(name.to_string()))
    } else {
        extend(
            &extend(&frame.path, Seg::Key(name.to_string())),
            Seg::Index(seen),
        )
    }
}

/// Whether `inner` is an attribute name: an unquoted word directly inside an
/// open tag that is followed by `=`.
fn is_attr_name(source: &str, open: &Token, inner: Token) -> bool {
    let inner_tokens = tokenize_tag_inside(source, open);
    inner_tokens
        .iter()
        .position(|token| token.start == inner.start && token.end == inner.end)
        .and_then(|position| inner_tokens.get(position + 1))
        .is_some_and(|next| matches!(next.kind, Kind::Equals))
}

/// Whether `token` holds the caret. An open tag holds it strictly inside its
/// `<...>`, or at its end while still unclosed; a caret right after a closing
/// `>` is in the text that follows.
fn contains_caret(source: &str, token: &Token, cursor: usize) -> bool {
    if !(token.start < cursor && cursor <= token.end) {
        return false;
    }
    match token.kind {
        Kind::OpenTag { .. } => cursor < token.end || !source[..token.end].ends_with('>'),
        _ => true,
    }
}

/// Captures the caret state: the holding element, the token under the caret,
/// and the open tag around the caret when it sits inside one.
fn capture_snapshot(source: &str, tokens: &[Token], frames: &[Frame], cursor: usize) -> Snapshot {
    let partial = tokens
        .iter()
        .find(|token| {
            contains_caret(source, token, cursor)
                && !matches!(token.kind, Kind::Special | Kind::Comment)
        })
        .copied();
    let open_tag = partial.filter(|token| matches!(token.kind, Kind::OpenTag { .. }));
    let inner = open_tag.map_or_else(Vec::new, |tag| tokenize_tag_inside(source, &tag));
    let pending_attr = open_tag.and_then(|_| pending_attribute(source, &inner, cursor));
    let frame = frames.last().cloned();
    let path = frame
        .as_ref()
        .map_or_else(Vec::new, |frame| frame.path.clone());
    Snapshot {
        path,
        frame,
        partial,
        in_tag: open_tag.is_some(),
        open_tag,
        inner,
        pending_attr,
    }
}

/// The attribute name awaiting a value when the caret follows `name =` with
/// only blanks between (`name = |`).
fn pending_attribute(source: &str, inner: &[Token], cursor: usize) -> Option<String> {
    let equals = inner
        .iter()
        .rposition(|token| matches!(token.kind, Kind::Equals) && token.start < cursor)?;
    if !source[inner[equals].end..cursor].trim().is_empty() {
        return None;
    }
    let name = inner.get(equals.checked_sub(1)?)?;
    matches!(name.kind, Kind::Text { quoted: false })
        .then(|| source[name.start..name.end].to_string())
}

/// Re-walks every open tag pairing `name = "value"` so attribute values get
/// spans. Values record under `@name` on the element's eventual path, found
/// by matching names against the recorded attribute spans.
fn record_attribute_values(source: &str, tokens: &[Token], scan: &mut Scan) {
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if !matches!(token.kind, Kind::OpenTag { .. }) {
            index += 1;
            continue;
        }
        let inner = tokenize_tag_inside(source, &token);
        let mut position = 0;
        while position < inner.len() {
            let name_token = inner[position];
            let is_name = matches!(name_token.kind, Kind::Text { quoted: false })
                && inner
                    .get(position + 1)
                    .is_some_and(|next| matches!(next.kind, Kind::Equals));
            if !is_name {
                position += 1;
                continue;
            }
            let name = source[name_token.start..name_token.end].to_string();
            let value_token = inner
                .get(position + 2)
                .filter(|next| matches!(next.kind, Kind::Text { .. }));
            if let Some(value_token) = value_token {
                // Find the element path via the matching recorded attribute span.
                let path = scan
                    .attr_names
                    .iter()
                    .find(|(_, start, end)| *start == name_token.start && *end == name_token.end)
                    .map(|(path, _, _)| {
                        let mut key = path.clone();
                        key.push(format!("@{name}"));
                        key
                    });
                if let Some(path) = path {
                    scan.value_spans
                        .push((path, value_token.start, value_token.end));
                }
            }
            position += 1;
        }
        index += 1;
    }
}

fn utf16(text: &str) -> usize {
    text.encode_utf16().count()
}

/// One-based line and character column of a byte offset.
fn line_column(source: &str, byte: usize) -> (usize, usize) {
    let before = &source[..byte];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    (line, before[line_start..].chars().count() + 1)
}

fn xml_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// Parsing and spans
// ---------------------------------------------------------------------------

/// Parses the document with `quick-xml`, then rejects repeated sibling
/// elements (see `xml_duplicates`). Every error carries a one-based line and
/// character column.
fn xml_parse(source: &str) -> Result<Value, ParseError> {
    let document = build_value(source)?;
    match xml_duplicates(source) {
        Some(error) => Err(error),
        None => Ok(document),
    }
}

/// An element whose end tag has not been read yet.
struct OpenElement {
    name: String,
    map: Map<String, Value>,
    start: usize,
}

/// Builds the `@`/`#text` document model from `quick-xml` events. Structural
/// errors (mismatched or unclosed tags, stray text, several roots, duplicate
/// attributes) are reported here at the offending markup.
fn build_value(source: &str) -> Result<Value, ParseError> {
    if source.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<OpenElement> = Vec::new();
    let mut roots = Map::new();
    let mut pending_text = String::new();
    loop {
        // `buffer_position` before a read is where the next event begins.
        let event_start = reader.buffer_position() as usize;
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event,
            Err(error) => {
                let position = reader.error_position() as usize;
                return Err(syntax_error(source, position, error.to_string()));
            }
        };
        match event {
            Event::Eof => break,
            Event::Start(element) => {
                flush_text(&mut pending_text, &mut stack);
                check_root_order(source, event_start, stack.is_empty(), &roots)?;
                stack.push(open_element(source, &element, event_start)?);
            }
            Event::Empty(element) => {
                flush_text(&mut pending_text, &mut stack);
                check_root_order(source, event_start, stack.is_empty(), &roots)?;
                let open = open_element(source, &element, event_start)?;
                push_child(&mut stack, &mut roots, open.name, Value::Object(open.map));
            }
            Event::End(element) => {
                flush_text(&mut pending_text, &mut stack);
                let name = element.local_name().into_inner().to_string();
                let start = markup_start(source, event_start);
                let Some(open) = stack.pop() else {
                    return Err(syntax_error(
                        source,
                        start,
                        format!("unexpected closing tag \"{name}\""),
                    ));
                };
                if open.name != name {
                    return Err(syntax_error(
                        source,
                        start,
                        format!(
                            "mismatched closing tag: expected \"{}\", found \"{name}\"",
                            open.name
                        ),
                    ));
                }
                push_child(&mut stack, &mut roots, open.name, Value::Object(open.map));
            }
            Event::Text(text) => {
                let content = text.xml10_content();
                add_text(
                    source,
                    event_start,
                    &content,
                    &mut pending_text,
                    stack.is_empty(),
                )?;
            }
            Event::CData(data) => {
                let content = data.xml10_content();
                add_text(
                    source,
                    event_start,
                    &content,
                    &mut pending_text,
                    stack.is_empty(),
                )?;
            }
            Event::GeneralRef(reference) => {
                let name = reference.into_inner().into_owned();
                let content = resolve_reference(&name).unwrap_or_else(|| format!("&{name};"));
                add_text(
                    source,
                    event_start,
                    &content,
                    &mut pending_text,
                    stack.is_empty(),
                )?;
            }
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
        buf.clear();
    }
    if let Some(open) = stack.last() {
        return Err(syntax_error(
            source,
            open.start,
            format!("unclosed tag \"{}\"", open.name),
        ));
    }
    Ok(Value::Object(roots))
}

/// Reads an element's name and attributes. Duplicate attributes are an error.
fn open_element(
    source: &str,
    element: &BytesStart<'_>,
    event_start: usize,
) -> Result<OpenElement, ParseError> {
    let start = markup_start(source, event_start);
    let name = element.local_name().into_inner().to_string();
    let mut map = Map::new();
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|error| match error {
            AttrError::Duplicated(..) => {
                syntax_error(source, start, format!("duplicate attribute in <{name}>"))
            }
            other => syntax_error(source, start, other.to_string()),
        })?;
        let key = attribute.key.into_inner().to_string();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| syntax_error(source, start, error.to_string()))?;
        map.insert(format!("@{key}"), Value::String(value.into_owned()));
    }
    Ok(OpenElement { name, map, start })
}

/// A second root element is an error; `quick-xml` accepts it silently.
fn check_root_order(
    source: &str,
    event_start: usize,
    at_root: bool,
    roots: &Map<String, Value>,
) -> Result<(), ParseError> {
    if at_root && !roots.is_empty() {
        let at = markup_start(source, event_start);
        return Err(syntax_error(source, at, "multiple root elements"));
    }
    Ok(())
}

/// Collects text inside an element; non-blank text outside the root element
/// is an error.
fn add_text(
    source: &str,
    event_start: usize,
    content: &str,
    pending: &mut String,
    at_root: bool,
) -> Result<(), ParseError> {
    if !at_root {
        pending.push_str(content);
    } else if !content.trim().is_empty() {
        let at = markup_start(source, event_start);
        return Err(syntax_error(source, at, "text outside the root element"));
    }
    Ok(())
}

/// Appends pending text to the innermost open element as `#text`. Text split
/// by child elements or entity references is joined in document order.
fn flush_text(pending: &mut String, stack: &mut [OpenElement]) {
    let text = std::mem::take(pending);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if let Some(open) = stack.last_mut() {
        match open.map.get_mut("#text") {
            Some(Value::String(existing)) => existing.push_str(trimmed),
            _ => {
                open.map
                    .insert("#text".to_string(), Value::String(trimmed.to_string()));
            }
        }
    }
}

/// Stores a finished element under its parent, or as a root. Repeated names
/// are rejected separately by `xml_duplicates`, so a later one simply replaces
/// the earlier entry in the value tree.
fn push_child(
    stack: &mut [OpenElement],
    roots: &mut Map<String, Value>,
    name: String,
    value: Value,
) {
    match stack.last_mut() {
        Some(parent) => {
            parent.map.insert(name, value);
        }
        None => {
            roots.insert(name, value);
        }
    }
}

/// XML entity references: the five predefined entities and character
/// references. Unknown entities stay literal.
fn resolve_reference(name: &str) -> Option<String> {
    if let Some(character) = named_entity(name) {
        return Some(character.to_string());
    }
    let numeric = numeric_entity(name.strip_prefix('#')?)?;
    Some(numeric.to_string())
}

/// Error at a byte offset, converted to a one-based line and character column.
fn syntax_error(source: &str, byte: usize, message: impl Into<String>) -> ParseError {
    let (line, column) = line_column(source, byte.min(source.len()));
    ParseError {
        message: message.into(),
        line: Some(line),
        column: Some(column),
    }
}

/// The byte where markup starting at or after `from` begins: the first
/// non-whitespace character, so errors point at `<` rather than the blank run
/// before it.
fn markup_start(source: &str, from: usize) -> usize {
    let from = from.min(source.len());
    source[from..]
        .find(|character: char| !character.is_whitespace())
        .map_or(from, |offset| from + offset)
}

/// Rejects repeated sibling elements with the same name under one parent. The
/// tolerant scan sees every occurrence, so the error points at the repeat.
fn xml_duplicates(source: &str) -> Option<ParseError> {
    let scan = analyze(source, 0);
    let mut seen = HashSet::new();
    for (owner, key, start) in &scan.keys {
        if !seen.insert((*owner, key.as_str())) {
            return Some(syntax_error(
                source,
                *start,
                format!("duplicate element \"{key}\""),
            ));
        }
    }
    None
}

#[cfg(feature = "schema-validation")]
fn span_between(source: &str, start: usize, end: usize) -> (usize, usize, usize) {
    let (line, column) = line_column(source, start);
    (line, column, column + source[start..end].chars().count())
}

#[cfg(feature = "schema-validation")]
fn xml_name_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = attribute_aware_segments(&pointer_segments(path))?;
    analyze(source, 0)
        .key_spans
        .into_iter()
        .find(|(segments, _, _)| *segments == wanted)
        .map(|(_, start, end)| span_between(source, start, end))
}

#[cfg(feature = "schema-validation")]
fn xml_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = attribute_aware_segments(&pointer_segments(path))?;
    let (_, start, end) = analyze(source, 0)
        .value_spans
        .into_iter()
        .find(|(segments, _, _)| *segments == wanted)?;
    Some(span_between(source, start, end))
}

/// Splits `name/index` pointer tails so repeated siblings address correctly:
/// the pointer for the second `<item>` is `item/1`, matching the scan's path.
#[cfg(feature = "schema-validation")]
fn attribute_aware_segments(segments: &[String]) -> Option<Vec<String>> {
    Some(segments.to_vec())
}

#[cfg(feature = "schema-validation")]
fn first_content_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

fn completion_context(source: &str, cursor: usize) -> CompletionContext {
    let cursor = utf16_to_byte_index(source, cursor);
    let mut scan = analyze(source, cursor);
    let Some(snapshot) = scan.snapshot.take() else {
        unreachable!("analyze always captures a snapshot");
    };
    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let base_indent: String = source[line_start..]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let path: Vec<SchemaPathSegment> = snapshot
        .path
        .iter()
        .map(|segment| match segment {
            Seg::Key(key) => SchemaPathSegment::Key(key.clone()),
            Seg::Index(index) => SchemaPathSegment::ArrayItem(*index),
        })
        .collect();

    // Inside a tag: completing a name, an attribute, or an attribute value.
    if snapshot.in_tag {
        return tag_completion_context(source, cursor, &snapshot, path, base_indent);
    }

    // In text: the typed fragment runs from the last `>` (or line start) to
    // the caret, extended through a partial `<name` when the caret sits right
    // after it (`<roo|` completes the element `root`).
    let (typed, replace_start, typed_from_tag) = text_typed(
        source,
        cursor,
        snapshot.partial.map(|token| (token.start, token.end)),
    );
    let explicit_entry_start = typed_from_tag;
    let entry_prefix = "";
    CompletionContext {
        path,
        typed,
        replace_start,
        replace_end: cursor_bytes(source, cursor),
        value_key: None,
        value_needs_space: false,
        is_array_item: false,
        base_indent,
        entry_prefix,
        explicit_entry_start,
        entry_suffix: "",
    }
}

/// Completion inside `<...>`: an element name, attribute name, or attribute
/// value. `pending_attr` (`name = |`) and a caret inside quotes complete the
/// value; a bare word completes the attribute; `<` or `</` plus a name
/// completes the element.
fn tag_completion_context(
    source: &str,
    cursor: usize,
    snapshot: &Snapshot,
    path: Vec<SchemaPathSegment>,
    base_indent: String,
) -> CompletionContext {
    let replace_end = cursor_bytes(source, cursor);
    // The element whose attributes the caret is in: the open tag's own path.
    let holder = snapshot.open_tag.map_or_else(Vec::new, |tag| {
        extend(&snapshot.path, Seg::Key(token_name(source, &tag)))
    });
    // Caret inside quotes: complete the attribute's value.
    if let Some((name, start, end)) = quoted_value_at(source, &snapshot.inner, cursor) {
        let value_path = attribute_value_path(&holder, &name);
        let typed = source[start.min(cursor)..cursor].to_string();
        return CompletionContext {
            path: value_path.0,
            typed,
            replace_start: cursor_bytes(source, start.min(cursor)),
            replace_end: cursor_bytes(source, end.max(cursor).min(source.len())),
            value_key: value_path.1,
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: "",
            explicit_entry_start: false,
            entry_suffix: "",
        };
    }
    // `name = |`: the value is empty; complete the attribute's values.
    if let Some(name) = snapshot.pending_attr.clone() {
        let value_path = attribute_value_path(&holder, &name);
        return CompletionContext {
            path: value_path.0,
            typed: String::new(),
            replace_start: replace_end,
            replace_end,
            value_key: value_path.1,
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: "",
            explicit_entry_start: false,
            entry_suffix: "",
        };
    }
    // Just after the name of a complete tag (`<host|>`) the context is the
    // element's own value, addressed from its parent: hover reads schema
    // documentation there.
    if let Some(tag) = snapshot.open_tag
        && matches!(tag.kind, Kind::OpenTag { name_end, .. } if name_end == cursor)
        && source[cursor..]
            .trim_start_matches([' ', '\t'])
            .starts_with(['>', '/'])
    {
        return CompletionContext {
            path,
            typed: String::new(),
            replace_start: replace_end,
            replace_end,
            value_key: Some(token_name(source, &tag)),
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: "",
            explicit_entry_start: false,
            entry_suffix: "",
        };
    }
    // The element name (the caret is within `<name`) completes the element. The
    // insert carries its own `<`, so the replaced range starts at it; the typed
    // query is the name typed so far.
    if let Some(tag) = snapshot
        .open_tag
        .filter(|tag| is_element_name_position(tag, cursor))
    {
        let name_start = (tag.start + 1).min(cursor);
        return CompletionContext {
            path,
            typed: source[name_start..cursor].to_string(),
            replace_start: cursor_bytes(source, tag.start.min(cursor)),
            replace_end,
            value_key: None,
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: "",
            explicit_entry_start: true,
            entry_suffix: "",
        };
    }
    // A bare word inside the tag completes an attribute name.
    let word = word_at(&snapshot.inner, cursor);
    // Otherwise this is an attribute name position.
    let (typed, replace_start) = match word {
        Some((start, end)) => (
            source[start..cursor.min(end)].to_string(),
            cursor_bytes(source, start),
        ),
        None => (String::new(), replace_end),
    };
    CompletionContext {
        path: attribute_holder_path(&holder),
        typed,
        replace_start,
        replace_end,
        value_key: None,
        value_needs_space: false,
        is_array_item: false,
        base_indent,
        entry_prefix: "",
        explicit_entry_start: false,
        entry_suffix: "",
    }
}

/// Holder path plus `value_key` for completing an attribute's value. As for a
/// JSON property, the path is the element holding the attribute and the key
/// (`@name` in the document model) addresses the value beneath it.
fn attribute_value_path(path: &[Seg], name: &str) -> (Vec<SchemaPathSegment>, Option<String>) {
    (attribute_holder_path(path), Some(format!("@{name}")))
}

/// Attribute names complete against the element's own properties: the holder
/// path is the element path itself.
fn attribute_holder_path(path: &[Seg]) -> Vec<SchemaPathSegment> {
    path.iter()
        .map(|segment| match segment {
            Seg::Key(key) => SchemaPathSegment::Key(key.clone()),
            Seg::Index(index) => SchemaPathSegment::ArrayItem(*index),
        })
        .collect()
}

/// The quoted attribute value around the caret, if any: `(name, start, end)`
/// where `start`/`end` exclude the quotes.
fn quoted_value_at(source: &str, inner: &[Token], cursor: usize) -> Option<(String, usize, usize)> {
    for (position, token) in inner.iter().enumerate() {
        if !matches!(token.kind, Kind::Text { quoted: true }) {
            continue;
        }
        if !(token.start < cursor && cursor <= token.end) {
            continue;
        }
        // The attribute name is `name = "value"`: word, `=`, then this quote.
        let name = (position >= 2
            && matches!(inner[position - 1].kind, Kind::Equals)
            && matches!(inner[position - 2].kind, Kind::Text { quoted: false }))
        .then(|| source[inner[position - 2].start..inner[position - 2].end].to_string())?;
        let quote_len = source[token.start..]
            .chars()
            .next()
            .map_or(0, char::len_utf8);
        let start = token.start + quote_len;
        let ends_with_quote = token.end > start
            && source[..token.end.min(source.len())]
                .chars()
                .next_back()
                .is_some_and(|character| character == '"' || character == '\'');
        let end = token.end - usize::from(ends_with_quote);
        return Some((name, start.min(cursor), end.max(start)));
    }
    None
}

/// The unquoted word around the caret inside a tag, if any.
fn word_at(inner: &[Token], cursor: usize) -> Option<(usize, usize)> {
    inner.iter().find_map(|token| {
        if !matches!(token.kind, Kind::Text { quoted: false }) {
            return None;
        }
        (token.start < cursor && cursor <= token.end).then_some((token.start, token.end))
    })
}

/// Whether the caret is within the element name of `tag` (`<na|me`), as opposed
/// to after it, where it completes an attribute.
fn is_element_name_position(tag: &Token, cursor: usize) -> bool {
    match tag.kind {
        Kind::OpenTag { name_end, .. } => tag.start < cursor && cursor <= name_end,
        _ => false,
    }
}

/// Text around the caret outside tags. Returns the typed fragment, its UTF-16
/// start, and whether the caret follows a partial `<name` (entry start).
fn text_typed(
    source: &str,
    cursor: usize,
    partial: Option<(usize, usize)>,
) -> (String, usize, bool) {
    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let segment_start = source[..cursor]
        .rfind('>')
        .map_or(line_start, |index| index + 1);
    if let Some((start, end)) = partial
        && source[start..].starts_with('<')
        && start >= segment_start
    {
        // The insert carries its own `<`, so the replaced range starts at it.
        return (
            source[start + 1..cursor.min(end)].to_string(),
            cursor_bytes(source, start),
            true,
        );
    }
    (
        source[segment_start..cursor].trim_start().to_string(),
        cursor_bytes(source, segment_start),
        false,
    )
}

fn cursor_bytes(source: &str, cursor: usize) -> usize {
    source[..cursor.min(source.len())].encode_utf16().count()
}

/// Element names already present under the element at `path`, excluding the
/// partly typed tag. Filters suggestions so existing siblings are not offered
/// again; attributes do not filter elements.
fn existing_elements(source: &str, cursor_byte: usize) -> Option<HashSet<String>> {
    let mut scan = analyze(source, cursor_byte);
    let snapshot = scan.snapshot.take()?;
    let naming_element = snapshot
        .open_tag
        .is_some_and(|tag| is_element_name_position(&tag, cursor_byte));
    if snapshot.in_tag && !naming_element {
        // Attribute positions filter attributes already on the tag instead.
        return snapshot
            .pending_attr
            .is_none()
            .then(|| existing_attributes(source, &snapshot));
    }
    let frame = snapshot.frame?;
    // Keys are recorded per owner element id, and the partly typed tag is never
    // recorded, so every key with the caret element's id is a direct sibling.
    let names = scan
        .keys
        .iter()
        .filter(|(owner, _, _)| *owner == frame.id)
        .map(|(_, key, _)| key.clone())
        .collect();
    Some(names)
}

/// Attribute names already on the tag under the caret.
fn existing_attributes(source: &str, snapshot: &Snapshot) -> HashSet<String> {
    let mut names = HashSet::new();
    for (position, token) in snapshot.inner.iter().enumerate() {
        if !matches!(token.kind, Kind::Text { quoted: false }) {
            continue;
        }
        let is_name = snapshot
            .inner
            .get(position + 1)
            .is_some_and(|next| matches!(next.kind, Kind::Equals));
        if is_name {
            names.insert(source[token.start..token.end].to_string());
        }
    }
    names
}

/// Insert text for a property suggestion, with the caret placed where the user
/// will type. Elements expand to `<key>|</key>` (or a block with required
/// children); attributes insert `name="|"`.
fn property_insert(
    key: &str,
    property: &Value,
    root: &Value,
    base_indent: &str,
    unit: &str,
) -> Option<(String, usize)> {
    if let Some(name) = key.strip_prefix('@') {
        return attribute_insert(name, property, root);
    }
    let mut insert = format!("<{key}>");
    let mut caret = None;
    if let Some(value) = schema_const(property, root) {
        insert.push_str(&escape_text(&xml_text(value)));
    } else if array_branch(property, root).is_some() {
        match schema_default(property, root)
            .filter(|value| !value.as_array().is_some_and(Vec::is_empty))
        {
            Some(default) => insert.push_str(&escape_text(&xml_text(default))),
            None => {
                insert.push_str(&format_child_placeholder(base_indent, unit));
                caret = Some(insert.len());
            }
        }
    } else if let Some(branch) = object_branch(property, root) {
        match schema_default(property, root)
            .filter(|value| !value.as_object().is_some_and(Map::is_empty))
        {
            Some(default) => insert.push_str(&escape_text(&xml_text(default))),
            None => {
                let children = required_children(branch);
                if children.is_empty() {
                    // No required children: leave the caret between the tags.
                    caret = Some(insert.len());
                } else {
                    let child_indent = format!("{base_indent}{unit}");
                    insert.push('\n');
                    for (name, child) in &children {
                        if caret.is_none() {
                            caret = Some(insert.len() + child_indent.len() + name.len() + 2);
                        }
                        insert.push_str(&child_indent);
                        insert.push_str(&format!("<{name}>"));
                        if let Some(default) = schema_default(child, root) {
                            insert.push_str(&escape_text(&xml_text(default)));
                        }
                        insert.push_str(&format!("</{name}>\n"));
                    }
                    insert.push_str(base_indent);
                }
            }
        }
    } else if let Some(default) = schema_default(property, root) {
        insert.push_str(&escape_text(&xml_text(default)));
    }
    insert.push_str(&format!("</{key}>"));
    let caret = utf16(&insert[..caret.unwrap_or(insert.len())]);
    Some((insert, caret))
}

/// Insert text for an attribute suggestion: `name="|"` with the caret inside
/// the quotes, or with the default/const filled in.
fn attribute_insert(name: &str, property: &Value, root: &Value) -> Option<(String, usize)> {
    let mut insert = format!("{name}=\"");
    if let Some(value) = schema_const(property, root) {
        insert.push_str(&escape_attribute(&xml_text(value)));
    } else if let Some(default) = schema_default(property, root) {
        insert.push_str(&escape_attribute(&xml_text(default)));
    }
    let caret = utf16(&insert);
    insert.push('"');
    Some((insert, caret))
}

/// The required properties of an object schema that have a schema of their
/// own, in the order the schema lists them.
fn required_children(branch: &Value) -> Vec<(String, &Value)> {
    let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    branch
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|name| {
            let key = name.strip_prefix('@').unwrap_or(name);
            properties.get(key).map(|child| (key.to_string(), child))
        })
        .collect()
}

/// Placeholder for the first array item: `<item>|</item>` on its own line.
fn format_child_placeholder(base_indent: &str, unit: &str) -> String {
    format!("\n{base_indent}{unit}")
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;")
}

fn escape_attribute(text: &str) -> String {
    escape_text(text).replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

/// The element or attribute name under `column` (a character index) on a
/// single line, with its starting column. Comment-only lines yield `None`.
fn hover_name(line: &str, column: usize) -> Option<(String, usize)> {
    let tokens = tokenize(line);
    for token in &tokens {
        match token.kind {
            Kind::OpenTag { name_end, .. } => {
                let start = token.start + 1;
                if start >= name_end {
                    continue;
                }
                let start_column = line[..start].chars().count();
                let end_column = line[..name_end.min(line.len())].chars().count();
                if (start_column..end_column).contains(&column) {
                    return Some((decode_entities(&line[start..name_end]), start_column));
                }
                for inner in tokenize_tag_inside(line, token) {
                    if !matches!(inner.kind, Kind::Text { quoted: false }) {
                        continue;
                    }
                    let start_column = line[..inner.start].chars().count();
                    let end_column = line[..inner.end].chars().count();
                    if (start_column..end_column).contains(&column) {
                        return Some((line[inner.start..inner.end].to_string(), start_column));
                    }
                }
            }
            Kind::EndTag => {
                let name = token_name(line, token);
                if name.is_empty() {
                    continue;
                }
                let start = token.start + 2;
                let start_column = line[..start].chars().count();
                let end_column = start_column
                    + line[start..]
                        .chars()
                        .take_while(|character| !matches!(character, '>' | '/' | ' ' | '\t'))
                        .count();
                if (start_column..end_column).contains(&column) {
                    return Some((name, start_column));
                }
            }
            Kind::Comment => return None,
            _ => {}
        }
    }
    None
}

/// Byte offset within `line` where the hover context is read: just past the
/// first `>` after the name at `column`, so one line holding a whole subtree
/// resolves the hover under the right path.
fn hover_context_end(line: &str, column: usize) -> usize {
    let byte = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index);
    let name_start = hover_name(line, column).map_or(byte, |(_, start_column)| {
        line.char_indices()
            .nth(start_column)
            .map_or(line.len(), |(index, _)| index)
    });
    line[name_start..]
        .find(|character: char| matches!(character, '>' | '/' | '=') || character.is_whitespace())
        .map_or(line.len(), |relative| name_start + relative)
}

// ---------------------------------------------------------------------------
// Editing and highlighting
// ---------------------------------------------------------------------------

/// Enter inside an element indents one level deeper; otherwise it keeps the
/// current indentation.
fn newline_with_indent(source: &str, cursor: usize) -> String {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let indent: String = source[line_start..byte]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let before = source[..byte].trim_end_matches([' ', '\t']);
    if before.ends_with('>') && !before.ends_with("/>") && is_open_tag_tail(source, byte) {
        format!("\n{indent}{}", indent_unit(source))
    } else {
        format!("\n{indent}")
    }
}

/// Whether the `>` before the caret closes an opening tag (not a closing or
/// self-closing one and not inside a comment).
fn is_open_tag_tail(source: &str, byte: usize) -> bool {
    let before = &source[..byte];
    let Some(tag_start) = before.rfind('<') else {
        return false;
    };
    let tag = &before[tag_start..];
    !(tag.starts_with("</") || tag.starts_with("<!--") || tag.trim_end().ends_with('/'))
}

/// Enter between `<tag>|</tag>` opens a block: the caret lands on an indented
/// middle line and the closing tag stays on its own line below.
fn newline_insertion(source: &str, cursor: usize) -> (String, usize) {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let indent: String = source[line_start..byte]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let before = source[..byte].trim_end_matches([' ', '\t']);
    let after = source[byte..].trim_start_matches([' ', '\t']);
    if before.ends_with('>')
        && !before.ends_with("/>")
        && after.starts_with("</")
        && is_open_tag_tail(source, byte)
    {
        let unit = indent_unit(source);
        // The caret sits after the newline and one unit, on the middle line.
        let caret = 1 + indent.len() + unit.len();
        (format!("\n{indent}{unit}\n{indent}"), caret)
    } else {
        let text = newline_with_indent(source, cursor);
        let caret = text.len();
        (text, caret)
    }
}

/// Toggles `<!-- -->` comments over the selection. Lines already inside a
/// comment are unwrapped; otherwise each selected line is wrapped.
fn toggle_xml_comments(
    source: &str,
    selection_start: usize,
    selection_end: usize,
) -> Option<(String, usize, usize)> {
    let start = utf16_to_byte_index(source, selection_start);
    let end = utf16_to_byte_index(source, selection_end);
    let first_line = source[..start]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let newline_count = source[..end].bytes().filter(|byte| *byte == b'\n').count();
    let last_line =
        newline_count.saturating_sub(usize::from(start < end && source[..end].ends_with('\n')));

    let lines: Vec<_> = source.split_inclusive('\n').collect();
    if first_line >= lines.len() {
        return None;
    }
    let selected = &lines[first_line..=last_line.min(lines.len() - 1)];
    let uncomment = selected
        .iter()
        .filter_map(|line| {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            let trimmed = body.trim();
            (!trimmed.is_empty()).then(|| trimmed.starts_with("<!--") && trimmed.ends_with("-->"))
        })
        .all(|commented| commented);
    // Only unwrap when every selected line is wrapped on its own line; a
    // partially commented selection gets wrapped instead.
    if !uncomment
        && selected.iter().any(|line| {
            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r').trim();
            !trimmed.is_empty() && trimmed.starts_with("<!--") && trimmed.ends_with("-->")
        })
    {
        return wrap_xml_selection(source, start, end);
    }
    let mut edits: Vec<(usize, usize, usize)> = Vec::new();
    let mut offset = 0;
    let mut updated = String::with_capacity(source.len() + lines.len() * 9);

    for (line_index, line) in lines.iter().enumerate() {
        if (first_line..=last_line).contains(&line_index) {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            if body.trim().is_empty() {
                updated.push_str(line);
            } else if uncomment {
                let trimmed = body.trim();
                let indent_len = body.len() - body.trim_start().len();
                let inner = trimmed
                    .strip_prefix("<!--")
                    .and_then(|rest| rest.strip_suffix("-->"))
                    .map(|inner| inner.strip_prefix(' ').unwrap_or(inner))
                    .map(|inner| inner.strip_suffix(' ').unwrap_or(inner))
                    .unwrap_or(trimmed);
                edits.push((offset + indent_len, trimmed.len(), inner.len()));
                updated.push_str(&line[..indent_len]);
                updated.push_str(inner);
                updated.push_str(&line[body.len()..]);
            } else {
                let indent_len = body.len() - body.trim_start().len();
                let trimmed = body.trim();
                edits.push((offset + indent_len, trimmed.len(), trimmed.len() + 9));
                updated.push_str(&line[..indent_len]);
                updated.push_str("<!-- ");
                updated.push_str(trimmed);
                updated.push_str(" -->");
                updated.push_str(&line[body.len()..]);
            }
        } else {
            updated.push_str(line);
        }
        offset += line.len();
    }

    let map_offset = |position: usize, right_biased: bool| {
        let mut delta = 0isize;
        for (edit_position, removed, inserted) in &edits {
            if position < *edit_position {
                break;
            }
            if position == *edit_position && *removed == 0 && !right_biased {
                return (*edit_position as isize + delta) as usize;
            }
            if position <= *edit_position + *removed {
                return (*edit_position as isize + delta + *inserted as isize) as usize;
            }
            delta += *inserted as isize - *removed as isize;
        }
        (position as isize + delta) as usize
    };
    let updated_start = map_offset(start, start == end).min(updated.len());
    let updated_end = map_offset(end, true).min(updated.len());
    let updated_selection_start = updated[..updated_start].encode_utf16().count();
    let updated_selection_end = updated[..updated_end].encode_utf16().count();
    Some((updated, updated_selection_start, updated_selection_end))
}

/// Wraps the raw selection in one `<!-- -->` pair (used when the selection
/// already contains wrapped lines, where per-line unwrapping would corrupt
/// the document).
fn wrap_xml_selection(source: &str, start: usize, end: usize) -> Option<(String, usize, usize)> {
    let mut updated = String::with_capacity(source.len() + 9);
    updated.push_str(&source[..start]);
    updated.push_str("<!-- ");
    updated.push_str(&source[start..end]);
    updated.push_str(" -->");
    updated.push_str(&source[end..]);
    // The selection runs from the opening marker through the closing `-->`.
    let selection_start = updated[..start].encode_utf16().count();
    let selection_end = updated[..end + 9].encode_utf16().count();
    Some((updated, selection_start, selection_end))
}

/// The open quote or unclosed comment at `cursor`, if the caret is inside a
/// quoted attribute value, plus whether it sits inside a comment. Drives the
/// auto-pairing guard.
fn xml_lexical_state(source: &str, cursor: usize) -> (Option<char>, bool) {
    let byte = utf16_to_byte_index(source, cursor);
    let tokens = tokenize(source);
    // Inside a comment span (or an unterminated one reaching past the caret).
    for token in &tokens {
        if matches!(token.kind, Kind::Comment) && token.start < byte && byte <= token.end {
            return (None, true);
        }
    }
    // Inside a quoted attribute value of the tag around the caret.
    for token in &tokens {
        if !matches!(token.kind, Kind::OpenTag { .. }) {
            continue;
        }
        let inner = tokenize_tag_inside(source, token);
        for inner_token in &inner {
            if matches!(inner_token.kind, Kind::Text { quoted: true })
                && inner_token.start < byte
                && byte <= inner_token.end
            {
                let quote = source[inner_token.start..].chars().next();
                return (quote, false);
            }
        }
    }
    (None, false)
}

/// Heuristic language score: a leading `<tag ...>` (optionally after an XML
/// declaration or whitespace) scores high; comment-only or plain text scores
/// near zero.
fn xml_confidence(source: &str) -> u8 {
    let trimmed = source.trim_start();
    let body = trimmed
        .strip_prefix("<?xml")
        .and_then(|rest| rest.find("?>").map(|end| &rest[end + 2..]))
        .unwrap_or(trimmed)
        .trim_start();
    if body.starts_with('<')
        && body[1..].chars().next().is_some_and(|character| {
            character.is_alphabetic() || character == '/' || character == '!' || character == '?'
        })
    {
        200
    } else {
        0
    }
}

/// Syntax-highlighted rendering of one line. Every byte of the line is
/// rendered exactly once, so the overlay lines up with the textarea.
fn highlight_line(line: &str) -> AnyView {
    let mut parts: Vec<AnyView> = Vec::new();
    let mut position = 0;
    for token in tokenize(line) {
        if token.start > position {
            parts.push(plain_span(&line[position..token.start]));
        }
        match token.kind {
            Kind::Comment => parts.push(styled(
                "config-editor__comment",
                &line[token.start..token.end],
            )),
            Kind::Special => parts.push(styled(
                "config-editor__punctuation",
                &line[token.start..token.end],
            )),
            Kind::OpenTag { name_end, .. } => highlight_open_tag(line, token, name_end, &mut parts),
            Kind::EndTag => highlight_end_tag(line, token, &mut parts),
            Kind::Text { .. } | Kind::Equals => {
                parts.push(highlight_text_segment(&line[token.start..token.end]));
            }
        }
        position = token.end;
    }
    if position < line.len() {
        parts.push(plain_span(&line[position..]));
    }
    view! { <span>{parts}</span> }.into_any()
}

/// `<name attr="value" ...>` or `<name ...` while typing: the bracket and name,
/// attribute names and quoted values, then the closing `>` or `/>`.
fn highlight_open_tag(line: &str, token: Token, name_end: usize, parts: &mut Vec<AnyView>) {
    parts.push(styled("config-editor__punctuation", "<"));
    let name_end = name_end.min(line.len());
    if name_end > token.start + 1 {
        parts.push(styled(
            "config-editor__key",
            &line[token.start + 1..name_end],
        ));
    }
    let mut position = name_end;
    for inner in tokenize_tag_inside(line, &token) {
        if inner.start > position {
            parts.push(plain_span(&line[position..inner.start]));
        }
        let text = &line[inner.start..inner.end];
        parts.push(match inner.kind {
            Kind::Text { quoted: true } => styled("config-editor__string", text),
            Kind::Equals => styled("config-editor__punctuation", text),
            _ if is_attr_name(line, &token, inner) => styled("config-editor__key", text),
            _ => styled("config-editor__value", text),
        });
        position = inner.end;
    }
    if token.end > position {
        parts.push(styled(
            "config-editor__punctuation",
            &line[position..token.end],
        ));
    }
}

/// `</name>`: the `</`, the raw name (prefix included), then the `>`.
fn highlight_end_tag(line: &str, token: Token, parts: &mut Vec<AnyView>) {
    let name_start = (token.start + 2).min(token.end);
    let name_end = line[name_start..token.end]
        .find(|character: char| character.is_whitespace() || character == '>')
        .map_or(token.end, |offset| name_start + offset);
    parts.push(styled(
        "config-editor__punctuation",
        &line[token.start..name_start],
    ));
    parts.push(styled("config-editor__key", &line[name_start..name_end]));
    parts.push(styled(
        "config-editor__punctuation",
        &line[name_end..token.end],
    ));
}

/// A styled run of text with a fixed highlight class.
fn styled(class: &'static str, text: &str) -> AnyView {
    view! { <span class=class>{text.to_string()}</span> }.into_any()
}

fn plain_span(text: &str) -> AnyView {
    view! { <span>{text.to_string()}</span> }.into_any()
}

/// Text between tags: element text renders as a value.
fn highlight_text_segment(text: &str) -> AnyView {
    if text.is_empty() {
        return plain_span(text);
    }
    styled("config-editor__value", text)
}

#[cfg(test)]
mod tests {
    //! XML behavior tests: parsing, spans, completion, hover, and inserts.

    use super::super::super::completion::{apply_completion, completions, key_hover};
    #[cfg(feature = "schema-validation")]
    use super::super::super::diagnostics::validate;
    use super::super::super::diagnostics::{SchemaBundle, build_schema};
    use super::super::super::language::LanguageBehavior;
    use super::XmlBehavior;

    fn schema() -> SchemaBundle {
        build_schema(
            r#"{"type":"object","properties":{
                "server":{"type":"object","properties":{
                    "host":{"type":"string","description":"Hostname"},
                    "port":{"type":"integer","description":"Port"},
                    "tls":{"type":"object","required":["enabled"],"properties":{"enabled":{"type":"boolean","default":false}}}
                }},
                "@version":{"type":"string"}
            }}"#,
        )
        .unwrap()
    }

    fn complete(
        source: &str,
        schema: &SchemaBundle,
    ) -> Vec<super::super::super::completion::Completion> {
        completions(
            source,
            source.encode_utf16().count(),
            false,
            Some(schema),
            true,
            &XmlBehavior,
        )
    }

    /// The parse error for `source`; `ParseError` has no `Debug`, so the tests
    /// avoid `unwrap`/`expect` on the `Result`.
    fn parse_error(source: &str) -> super::super::super::language::ParseError {
        XmlBehavior
            .parse(source)
            .expect_err("the document should be rejected")
    }

    #[test]
    fn mismatched_tags_are_a_parse_error_with_a_position() {
        let error = parse_error("<server>\n  <host></port></server>");
        assert_eq!(error.line, Some(2));
        assert_eq!(error.column, Some(9));
        assert!(error.message.contains("port"), "{}", error.message);
    }

    #[test]
    fn unclosed_tags_are_a_parse_error_at_the_open_tag() {
        let error = parse_error("<server>\n  <host>");
        assert!(error.message.contains("unclosed"), "{}", error.message);
        assert_eq!(error.line, Some(2));
        assert_eq!(error.column, Some(3));
    }

    #[test]
    fn repeated_sibling_elements_are_a_parse_error() {
        let error = parse_error("<server>\n  <host>a</host>\n  <host>b</host>\n</server>");
        assert_eq!(error.line, Some(3));
        assert_eq!(error.column, Some(4));
        assert!(error.message.contains("duplicate"), "{}", error.message);
    }

    #[test]
    fn same_names_in_different_elements_are_not_duplicates() {
        let source = "<a><host>x</host><b><host>y</host></b></a>";
        assert!(XmlBehavior.parse(source).is_ok());
    }

    #[test]
    fn duplicate_attributes_are_a_parse_error() {
        let error = parse_error("<server version=\"1\" version=\"2\"/>");
        assert!(error.message.contains("duplicate"), "{}", error.message);
        assert_eq!(error.line, Some(1));
    }

    #[test]
    fn attributes_land_under_at_keys_and_text_under_text() {
        let document = XmlBehavior
            .parse("<server version=\"1\">up</server>")
            .ok()
            .expect("document should parse");
        assert_eq!(document["server"]["@version"], serde_json::json!("1"));
        assert_eq!(document["server"]["#text"], serde_json::json!("up"));
    }

    #[test]
    fn stray_text_outside_the_root_is_a_parse_error() {
        let error = parse_error("<server/>\nstray");
        assert_eq!(error.line, Some(2));
        assert_eq!(error.column, Some(1));
    }

    #[test]
    fn syntax_errors_report_a_position() {
        let error = parse_error("<server><host></server>");
        assert_eq!(error.line, Some(1));
        assert!(error.column.is_some());
    }

    #[test]
    fn elements_are_suggested_and_a_partial_name_is_replaced() {
        let source = "<server><ho";
        let option = complete(source, &schema())
            .into_iter()
            .find(|option| option.label == "host")
            .expect("host should be suggested");
        let (updated, _) = apply_completion(source, &option);
        assert_eq!(updated, "<server><host></host>");
    }

    #[test]
    fn elements_already_present_are_not_suggested_again() {
        let source = "<server><host>x</host><h";
        assert!(
            complete(source, &schema())
                .iter()
                .all(|option| option.label != "host")
        );
    }

    #[test]
    fn attribute_values_are_suggested_inside_quotes() {
        let local_schema = build_schema(
            r#"{"type":"object","properties":{"server":{"type":"object","properties":{"@mode":{"type":"string","enum":["fast","slow"]}}}}}"#,
        )
        .unwrap();
        let source = "<server mode=\"f";
        let option = complete(source, &local_schema)
            .into_iter()
            .find(|option| option.label == "fast")
            .expect("fast should be suggested");
        let (updated, _) = apply_completion(source, &option);
        assert_eq!(updated, "<server mode=\"fast");
    }

    #[test]
    fn hover_over_an_element_returns_its_schema_documentation() {
        let source = "<server>\n  <host>x</host>\n</server>";
        let info =
            key_hover(source, 1, 3, Some(&schema()), &XmlBehavior).expect("host should have docs");
        assert_eq!(info.key, "host");
        assert_eq!(info.description.as_deref(), Some("Hostname"));
    }

    #[test]
    fn toggle_comments_wraps_lines_in_xml_comments() {
        let (updated, _, _) = XmlBehavior
            .toggle_comments("<server>\n  <host>x</host>\n</server>", 0, 8)
            .unwrap();
        assert!(updated.starts_with("<!-- <server> -->"), "{updated}");
    }

    #[test]
    fn toggle_comments_unwraps_xml_comments() {
        let (updated, _, _) = XmlBehavior
            .toggle_comments("<!-- <server> -->", 0, 17)
            .unwrap();
        assert_eq!(updated, "<server>");
    }

    #[test]
    fn enter_between_tags_puts_the_caret_on_an_indented_middle_line() {
        let (text, caret) = XmlBehavior.newline_insertion("<a></a>", 3);
        assert_eq!(text, "\n  \n");
        assert_eq!(caret, 3);
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn a_wrong_value_is_reported_on_the_value() {
        let source = "<server>\n  <port>x</port>\n</server>";
        let diagnostics = validate(source, Some(&schema()), None, true, &XmlBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
    }

    #[test]
    fn comments_do_not_confuse_the_structure_scan() {
        let source = "<server><!-- <host> --><po";
        let option = complete(source, &schema())
            .into_iter()
            .find(|option| option.label == "port")
            .expect("port should be suggested despite the comment");
        let (updated, _) = apply_completion(source, &option);
        assert_eq!(updated, "<server><!-- <host> --><port></port>");
    }
}
