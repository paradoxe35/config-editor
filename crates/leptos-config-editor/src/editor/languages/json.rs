//! JSON text-layer behavior.
//!
//! Decisions: duplicate keys are a parse error, as in YAML; JSON has no
//! comments, so `Ctrl+/` does nothing.
//!
//! Syntax errors come from `serde_json`. Structure (key paths, key and value
//! spans, the container under the caret) comes from a token scan that tolerates
//! invalid input, the normal state while a key is being typed.

use super::super::commands::indent_unit;
use super::super::completion::{
    array_branch, object_branch, placeholder, schema_const, schema_default,
};
use super::super::highlight::{self, TokenKind, Tokens};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{bracket_newline, CompletionContext, Newline, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError, SchemaPathSegment,
    inline_block, unclosed_brackets,
};
use serde_json::{Map, Value};
use std::collections::HashSet;

pub(super) struct JsonBehavior;


impl ConfigLanguage for JsonBehavior {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "JSON",
            icon: "J",
            extensions: &["json"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        json_parse(source)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        json_key_span(source, pointer).map(Span::from)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        json_value_span(source, pointer).map(Span::from)
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
        _path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        existing_keys(source, cursor_byte)
    }

    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        property_insert(key, property, root, context.base_indent, context.unit)
    }

    fn format_value(&self, value: &Value) -> String {
        json_text(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column).map(|(_, key, start_column)| (key, start_column))
    }

    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        key_at_column(line, column).map_or(line.len(), |(token, _, _)| {
            let rest = &line[token.end..];
            // Skip whitespace to the colon, then step past it.
            token.end + (rest.len() - rest.trim_start().len()) + 1
        })
    }

    fn highlight(&self, line: &str) -> Vec<super::super::highlight::Token> {
        highlight(line)
    }

    fn newline(&self, source: &str, cursor: usize) -> Newline {
        bracket_newline(source, cursor, &[('{', '}'), ('[', ']')], |line| line)
    }

    fn toggle_comments(
        &self,
        _source: &str,
        _selection_start: usize,
        _selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        None
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = (open_string_quote(source, cursor), false);
        Lexical { quote, in_comment }
    }

    fn entry_started(&self, source: &str, cursor: usize) -> bool {
        completion_context(source, cursor).explicit_entry_start
    }

    /// Pretty JSON, indented like `previous` when it is given.
    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        use serde::Serialize;
        let unit = previous.map_or_else(|| "  ".to_string(), indent_unit);
        let mut out = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(unit.as_bytes());
        let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
        value
            .serialize(&mut serializer)
            .map_err(|error| error.to_string())?;
        let mut text = String::from_utf8(out).map_err(|error| error.to_string())?;
        text.push('\n');
        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// A string. `closed` is false while the closing quote is still missing.
    Str { closed: bool },
    /// A literal or number: `true`, `null`, `-1.5`, or an invalid bare word.
    Word,
    /// One of `{ } [ ] , :`.
    Punct(char),
}

#[derive(Clone, Copy, Debug)]
struct Token {
    start: usize,
    end: usize,
    kind: Kind,
}

/// Splits `source` into tokens. Never fails: a string ends at its closing quote,
/// at a newline (JSON strings cannot span lines), or at the end of the input.
fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some(&(start, character)) = chars.peek() {
        if character.is_whitespace() {
            chars.next();
        } else if character == '"' {
            chars.next();
            let mut end = source.len();
            let mut closed = false;
            let mut escaped = false;
            for (index, next) in chars.by_ref() {
                if next == '\n' {
                    end = index;
                    break;
                }
                if escaped {
                    escaped = false;
                } else if next == '\\' {
                    escaped = true;
                } else if next == '"' {
                    end = index + next.len_utf8();
                    closed = true;
                    break;
                }
            }
            tokens.push(Token {
                start,
                end,
                kind: Kind::Str { closed },
            });
        } else if matches!(character, '{' | '}' | '[' | ']' | ',' | ':') {
            chars.next();
            tokens.push(Token {
                start,
                end: start + character.len_utf8(),
                kind: Kind::Punct(character),
            });
        } else {
            let mut end = start;
            while let Some(&(index, next)) = chars.peek() {
                if next.is_whitespace() || matches!(next, '"' | '{' | '}' | '[' | ']' | ',' | ':') {
                    break;
                }
                end = index + next.len_utf8();
                chars.next();
            }
            tokens.push(Token {
                start,
                end,
                kind: Kind::Word,
            });
        }
    }
    tokens
}

/// The text of a string token without its quotes, unescaped when closed.
fn decode_string(text: &str, closed: bool) -> String {
    if closed {
        serde_json::from_str::<String>(text).unwrap_or_else(|_| text.trim_matches('"').to_string())
    } else {
        text.strip_prefix('"').unwrap_or(text).to_string()
    }
}

// ---------------------------------------------------------------------------
// Structure scan
// ---------------------------------------------------------------------------

/// One step of a document path: an object key or an array index.
#[derive(Clone, Debug)]
enum Seg {
    Key(String),
    Index(usize),
}

/// Where an object is in its `key: value` cycle.
#[derive(Clone, Debug)]
enum Mode {
    /// Expecting a key (or `}`).
    Key,
    /// A key was read; expecting `:`.
    Colon(String),
    /// Expecting the value of this key.
    Value(String),
    /// A value was read; expecting `,` or the closing brace.
    After,
}

/// An open container. `slot` is the child currently being read, if any.
#[derive(Clone, Debug)]
enum Frame {
    Object {
        id: usize,
        mode: Mode,
        slot: Option<Seg>,
    },
    Array {
        id: usize,
        index: usize,
        awaiting: bool,
        slot: Option<Seg>,
    },
}

impl Frame {
    fn id(&self) -> usize {
        match self {
            Frame::Object { id, .. } | Frame::Array { id, .. } => *id,
        }
    }

    fn slot(&self) -> Option<Seg> {
        match self {
            Frame::Object { slot, .. } | Frame::Array { slot, .. } => slot.clone(),
        }
    }
}

/// The parser state at the caret.
struct Snapshot {
    /// Path of the container holding the caret.
    path: Vec<Seg>,
    /// The innermost open container, if the caret is inside one.
    frame: Option<Frame>,
    /// The string or word being typed under the caret, if any.
    partial: Option<Token>,
}

#[derive(Default)]
struct Scan {
    snapshot: Option<Snapshot>,
    /// Completed object keys as `(owner frame id, key, start byte)`. The key
    /// being typed at the caret is excluded.
    keys: Vec<(usize, String, usize)>,
    /// Byte spans of every key, keyed by its document path.
    key_spans: Vec<(Vec<String>, usize, usize)>,
    /// Byte spans of every value, keyed by its document path.
    value_spans: Vec<(Vec<String>, usize, usize)>,
}

fn with_segment(mut path: Vec<Seg>, segment: Seg) -> Vec<Seg> {
    path.push(segment);
    path
}

fn strings(path: &[Seg]) -> Vec<String> {
    path.iter()
        .map(|segment| match segment {
            Seg::Key(key) => key.clone(),
            Seg::Index(index) => index.to_string(),
        })
        .collect()
}

/// Path of the container that holds the innermost open frame.
fn container_path(frames: &[Frame]) -> Vec<Seg> {
    frames.split_last().map_or_else(Vec::new, |(_, parents)| {
        parents.iter().filter_map(Frame::slot).collect()
    })
}

/// Records a value starting at `start..end` in the innermost frame and moves that
/// frame past it. Scalars and containers both come through here.
fn take_value(frames: &mut [Frame], scan: &mut Scan, start: usize, end: usize) {
    let parent = container_path(frames);
    match frames.last_mut() {
        None => scan.value_spans.push((Vec::new(), start, end)),
        Some(Frame::Object { mode, slot, .. }) => {
            if let Mode::Value(key) = mode.clone() {
                let segment = Seg::Key(key);
                scan.value_spans.push((
                    strings(&with_segment(parent, segment.clone())),
                    start,
                    end,
                ));
                *slot = Some(segment);
                *mode = Mode::After;
            }
        }
        Some(Frame::Array {
            index,
            awaiting,
            slot,
            ..
        }) => {
            if *awaiting {
                let segment = Seg::Index(*index);
                scan.value_spans.push((
                    strings(&with_segment(parent, segment.clone())),
                    start,
                    end,
                ));
                *slot = Some(segment);
                *awaiting = false;
            }
        }
    }
}

/// Walks the whole document, tolerating invalid syntax, and captures the parser
/// state at `cursor` (a byte offset).
fn analyze(source: &str, cursor: usize) -> Scan {
    let tokens = tokenize(source);
    let partial_index = tokens.iter().position(|token| {
        token.start < cursor
            && cursor <= token.end
            && matches!(token.kind, Kind::Str { .. } | Kind::Word)
    });
    let mut frames: Vec<Frame> = Vec::new();
    let mut next_id = 0usize;
    let mut scan = Scan::default();

    for (index, token) in tokens.iter().enumerate() {
        let is_partial = partial_index == Some(index);
        if scan.snapshot.is_none() && (is_partial || token.start >= cursor) {
            scan.snapshot = Some(Snapshot {
                path: container_path(&frames),
                frame: frames.last().cloned(),
                partial: is_partial.then_some(*token),
            });
        }
        match token.kind {
            Kind::Str { closed } => {
                let name = decode_string(&source[token.start..token.end], closed);
                let in_key_slot = matches!(
                    frames.last(),
                    Some(Frame::Object {
                        mode: Mode::Key,
                        ..
                    })
                );
                if in_key_slot {
                    let path = with_segment(container_path(&frames), Seg::Key(name.clone()));
                    scan.key_spans
                        .push((strings(&path), token.start, token.end));
                    if !is_partial && let Some(frame) = frames.last() {
                        scan.keys.push((frame.id(), name.clone(), token.start));
                    }
                    if let Some(Frame::Object { mode, .. }) = frames.last_mut() {
                        *mode = Mode::Colon(name);
                    }
                } else {
                    take_value(&mut frames, &mut scan, token.start, token.end);
                }
            }
            Kind::Word => take_value(&mut frames, &mut scan, token.start, token.end),
            Kind::Punct(character @ ('{' | '[')) => {
                take_value(&mut frames, &mut scan, token.start, token.start + 1);
                next_id += 1;
                frames.push(if character == '{' {
                    Frame::Object {
                        id: next_id,
                        mode: Mode::Key,
                        slot: None,
                    }
                } else {
                    Frame::Array {
                        id: next_id,
                        index: 0,
                        awaiting: true,
                        slot: None,
                    }
                });
            }
            Kind::Punct('}' | ']') => {
                frames.pop();
            }
            Kind::Punct(':') => {
                if let Some(Frame::Object { mode, .. }) = frames.last_mut()
                    && let Mode::Colon(name) = mode.clone()
                {
                    *mode = Mode::Value(name);
                }
            }
            Kind::Punct(',') => match frames.last_mut() {
                Some(Frame::Object { mode, slot, .. }) => {
                    *mode = Mode::Key;
                    *slot = None;
                }
                Some(Frame::Array {
                    index,
                    awaiting,
                    slot,
                    ..
                }) => {
                    *index += 1;
                    *awaiting = true;
                    *slot = None;
                }
                None => {}
            },
            Kind::Punct(_) => {}
        }
    }

    if scan.snapshot.is_none() {
        scan.snapshot = Some(Snapshot {
            path: container_path(&frames),
            frame: frames.last().cloned(),
            partial: None,
        });
    }
    scan
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

fn json_text(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Parsing and spans
// ---------------------------------------------------------------------------

/// Parses with `serde_json` for syntax, then rejects duplicate keys in the same
/// object, which `serde_json` would otherwise silently keep the last of.
fn json_parse(source: &str) -> Result<Value, ParseError> {
    let document: Value = serde_json::from_str(source).map_err(|error| {
        let line_count = source.split('\n').count().max(1);
        let (mut line, mut column) = (error.line().clamp(1, line_count), error.column().max(1));
        // The marker shows the position, so the message drops serde's
        // " at line N column M".
        let full = error.to_string();
        let message = full
            .rsplit_once(" at line ")
            .map_or(full.as_str(), |(text, _)| text)
            .to_string();
        // A trailing comma is reported at the closer; point at the comma.
        if message.starts_with("trailing comma") {
            let line_start: usize = source.split_inclusive('\n').take(line - 1).map(str::len).sum();
            let at = line_start
                + source[line_start..]
                    .char_indices()
                    .nth(column - 1)
                    .map_or(source.len() - line_start, |(index, _)| index);
            if let Some(comma) = source[..at.min(source.len())].rfind(',') {
                (line, column) = line_column(source, comma);
            }
        }
        ParseError {
            message,
            line: Some(line),
            column: Some(column),
        }
    })?;
    let mut seen = HashSet::new();
    for (owner, key, start) in analyze(source, 0).keys {
        if !seen.insert((owner, key.clone())) {
            let (line, column) = line_column(source, start);
            return Err(ParseError {
                message: format!("duplicate key \"{key}\""),
                line: Some(line),
                column: Some(column),
            });
        }
    }
    Ok(document)
}

fn span_between(source: &str, start: usize, end: usize) -> (usize, usize, usize) {
    let (line, column) = line_column(source, start);
    (line, column, column + source[start..end].chars().count())
}

fn json_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    analyze(source, 0)
        .key_spans
        .into_iter()
        .find(|(segments, _, _)| *segments == wanted)
        .map(|(_, start, end)| span_between(source, start, end))
}

fn json_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    let (_, start, end) = analyze(source, 0)
        .value_spans
        .into_iter()
        .find(|(segments, _, _)| *segments == wanted)?;
    if source[start..].starts_with(['{', '[']) {
        // A container is marked at its opening bracket, not its whole body.
        let (line, column) = line_column(source, start);
        return Some((line, column, column + 1));
    }
    Some(span_between(source, start, end))
}

fn first_line_span(source: &str) -> Option<(usize, usize, usize)> {
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
    let scan = analyze(source, cursor);
    let Some(snapshot) = scan.snapshot else {
        unreachable!("analyze always captures a snapshot");
    };
    let typed_start = snapshot.partial.map_or(cursor, |token| token.start);
    let mut replace_end = snapshot.partial.map_or(cursor, |token| match token.kind {
        // An unclosed string has no closing quote to replace; stop at the caret.
        Kind::Str { closed: false } => cursor,
        _ => token.end.max(cursor),
    });
    // Completing a key whose colon is already typed (`"key"|: value`): swallow
    // the existing colon and its spaces, since the key insert supplies its own
    // `": "`.
    let completing_key = matches!(
        snapshot.frame,
        Some(
            Frame::Object {
                mode: Mode::Key,
                ..
            } | Frame::Object {
                mode: Mode::Colon(_),
                ..
            }
        )
    );
    if completing_key {
        let rest = &source[replace_end..];
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if let Some(after_colon) = trimmed.strip_prefix(':') {
            let spaces = after_colon.len() - after_colon.trim_start_matches([' ', '\t']).len();
            replace_end += rest.len() - trimmed.len() + 1 + spaces;
        }
    }
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
    // Past a complete value (object awaiting `,` or `}`, array not awaiting an
    // item), a new entry needs a separating comma.
    let needs_separator = matches!(
        snapshot.frame,
        Some(
            Frame::Object {
                mode: Mode::After,
                ..
            } | Frame::Array {
                awaiting: false,
                ..
            }
        )
    );
    // After the root value closes nothing can follow, so nothing is offered.
    if snapshot.frame.is_none() && !source[..typed_start].trim().is_empty() {
        return CompletionContext {
            no_properties: true,
            ..Default::default()
        };
    }
    // A typed opening quote is the start of a key, so the key list opens now.
    let explicit_entry_start = snapshot.partial.is_some() && source[typed_start..].starts_with('"');
    let (path, value_key, is_array_item) = match snapshot.frame {
        None => (path, None, false),
        Some(Frame::Object {
            mode: Mode::Value(key),
            ..
        }) => (path, Some(key), false),
        Some(Frame::Object { .. }) => (path, None, false),
        Some(Frame::Array { index, .. }) => {
            let mut path = path;
            path.push(SchemaPathSegment::ArrayItem(index));
            (path, None, true)
        }
    };
    // `"web": {"le|}`: the brace opened on this line is still open, so a
    // multi-line insert moves onto its own line inside it.
    let block = value_key.is_none().then(|| {
        inline_block(
            source,
            typed_start,
            replace_end,
            unclosed_brackets(&source[line_start..typed_start]),
            &indent_unit(source),
            |rest| rest.starts_with(['}', ']']),
            ",",
        )
    });
    // The missing comma goes right after the previous value, not before the
    // new key: `"a": 1|` + Enter + key gives `"a": 1,\n  "b": …`.
    let previous_end = source[..typed_start].trim_end().len();
    let (replace_start, entry_prefix) = if needs_separator {
        let gap = &source[previous_end..typed_start];
        let prefix = if gap.contains('\n') {
            format!(",{gap}")
        } else {
            ", ".to_string()
        };
        (previous_end, prefix)
    } else {
        (typed_start, String::new())
    };
    let following = &source[replace_end..];
    let entry_suffix = match following.trim_start().starts_with('"') {
        // A following key on its own line keeps it; no trailing space.
        true if following[..following.len() - following.trim_start().len()].contains('\n') => {
            ",".to_string()
        }
        true => ", ".to_string(),
        false => String::new(),
    };
    // An empty document has no root object yet: the first key opens it.
    let empty_document = source.trim().is_empty();
    CompletionContext {
        entry_wrap: if empty_document {
            Some(("{\n  ", "\n}\n"))
        } else {
            is_array_item.then_some(("{", "}"))
        },
        block: block.flatten(),
        path,
        typed: source[typed_start..cursor].to_string(),
        replace_start: utf16(&source[..replace_start]),
        replace_end: utf16(&source[..replace_end]),
        value_key,
        value_needs_space: snapshot.partial.is_none() && source[..cursor].ends_with(':'),
        is_array_item,
        base_indent,
        entry_prefix,
        explicit_entry_start,
        entry_suffix,
        ..Default::default()
    }
}

/// Keys already present in the object holding the caret. The partly typed key
/// is not counted, so it can still be completed.
fn existing_keys(source: &str, cursor_byte: usize) -> Option<HashSet<String>> {
    let scan = analyze(source, cursor_byte);
    let Some(Frame::Object { id, .. }) = scan.snapshot?.frame else {
        return None;
    };
    Some(
        scan.keys
            .iter()
            .filter(|(owner, _, _)| *owner == id)
            .map(|(_, key, _)| key.clone())
            .collect(),
    )
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
        .filter_map(|name| properties.get(name).map(|child| (name.to_string(), child)))
        .collect()
}

/// Insert text for a property suggestion, with the caret placed where the user
/// will type the value. Objects with required children expand into a block.
fn property_insert(
    key: &str,
    property: &Value,
    root: &Value,
    base_indent: &str,
    unit: &str,
) -> Option<(String, usize)> {
    let mut insert = format!("{}: ", serde_json::to_string(key).ok()?);
    let mut caret = None;
    if let Some(value) = schema_const(property, root) {
        insert.push_str(&json_text(value));
    } else if let Some(array) = array_branch(property, root) {
        match schema_default(property, root)
            .filter(|value| !value.as_array().is_some_and(Vec::is_empty))
        {
            Some(default) => insert.push_str(&json_text(default)),
            // A list of strings starts with a quoted item, so typing a value
            // keeps the document valid.
            None if array
                .get("items")
                .is_some_and(|items| placeholder(items, root).is_string()) =>
            {
                insert.push_str("[\"\"]");
                caret = Some(insert.len() - 2);
            }
            None => {
                insert.push_str("[]");
                caret = Some(insert.len() - 1);
            }
        }
    } else if let Some(branch) = object_branch(property, root) {
        match schema_default(property, root)
            .filter(|value| !value.as_object().is_some_and(Map::is_empty))
        {
            Some(default) => insert.push_str(&json_text(default)),
            None => {
                let children = required_children(branch);
                if children.is_empty() {
                    insert.push_str("{}");
                    caret = Some(insert.len() - 1);
                } else {
                    let child_indent = format!("{base_indent}{unit}");
                    insert.push_str("{\n");
                    for (index, (name, child)) in children.iter().enumerate() {
                        if index > 0 {
                            insert.push_str(",\n");
                        }
                        insert.push_str(&child_indent);
                        insert.push_str(&serde_json::to_string(name).ok()?);
                        insert.push_str(": ");
                        // The first child takes the caret; the others get a valid
                        // placeholder so the document stays parseable.
                        let takes_caret = caret.is_none();
                        if takes_caret {
                            caret = Some(insert.len());
                        }
                        match schema_default(child, root) {
                            Some(default) => insert.push_str(&json_text(default)),
                            None if !takes_caret => insert.push_str(&json_text(&placeholder(child, root))),
                            // The caret's own child: a string starts quoted, caret inside.
                            None if placeholder(child, root).is_string() => {
                                insert.push_str("\"\"");
                                caret = caret.map(|position| position + 1);
                            }
                            None => {}
                        }
                    }
                    insert.push('\n');
                    insert.push_str(base_indent);
                    insert.push('}');
                }
            }
        }
    } else if let Some(default) = schema_default(property, root) {
        insert.push_str(&json_text(default));
    } else if placeholder(property, root).is_string() {
        // A string value starts quoted, with the caret inside.
        insert.push_str("\"\"");
        caret = Some(insert.len() - 1);
    }
    let caret = caret.unwrap_or(insert.len());
    let caret_units = utf16(&insert[..caret]);
    Some((insert, caret_units))
}

/// Key under `column` (a character index) on a single line, if the string there
/// is followed by a colon. Returns the token, the unescaped key, and its start
/// column.
fn key_at_column(line: &str, column: usize) -> Option<(Token, String, usize)> {
    tokenize(line).into_iter().find_map(|token| {
        if !matches!(token.kind, Kind::Str { closed: true }) {
            return None;
        }
        let start_column = line[..token.start].chars().count();
        let end_column = start_column + line[token.start..token.end].chars().count();
        let followed_by_colon = line[token.end..].trim_start().starts_with(':');
        if !(start_column..end_column).contains(&column) || !followed_by_colon {
            return None;
        }
        let key = decode_string(&line[token.start..token.end], true);
        Some((token, key, start_column))
    })
}

// ---------------------------------------------------------------------------
// Editing and highlighting
// ---------------------------------------------------------------------------

/// The unclosed string quote at `cursor` on its line, if the caret is inside a
/// string. Drives the auto-pairing guard.
fn open_string_quote(source: &str, cursor: usize) -> Option<char> {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let mut in_string = false;
    let mut escaped = false;
    for character in source[line_start..byte].chars() {
        if !in_string {
            in_string = character == '"';
        } else if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            in_string = false;
        }
    }
    in_string.then_some('"')
}

fn highlight(line: &str) -> Vec<highlight::Token> {
    let mut tokens = Tokens::new();
    for token in tokenize(line) {
        let kind = match token.kind {
            Kind::Punct(_) => TokenKind::Punctuation,
            Kind::Str { .. } if line[token.end..].trim_start().starts_with(':') => TokenKind::Key,
            Kind::Str { .. } => TokenKind::String,
            Kind::Word => {
                let text = &line[token.start..token.end];
                if matches!(text, "true" | "false" | "null") || text.parse::<f64>().is_ok() {
                    TokenKind::Literal
                } else {
                    TokenKind::Value
                }
            }
        };
        tokens.push(token.start, token.end, kind);
    }
    tokens.finish()
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------


#[cfg(test)]
mod tests {
    use super::super::super::completion::{
        Completion, accept_marked, apply_completion, completions, key_hover,
    };
    #[cfg(feature = "schema-validation")]
    use super::super::super::diagnostics::validate;
    use super::super::super::diagnostics::{SchemaBundle, build_schema};
    use super::super::super::language::{InsertContext, ConfigLanguage};

    const INSERT_AT_ROOT: InsertContext<'static> = InsertContext {
        base_indent: "",
        unit: "  ",
        position: super::super::super::language::EntryPosition::Block,
        path: &[],
        array_item: false,
    };
    use super::super::super::highlight::{TokenKind, runs};
    use super::{JsonBehavior, highlight};

    fn schema() -> SchemaBundle {
        build_schema(
            r#"{"type":"object","properties":{
                "listen":{"type":"integer","description":"Port"},
                "insecure_plaintext":{"type":"boolean"},
                "auth":{"type":"object","required":["enabled"],"properties":{"enabled":{"type":"boolean","default":false}}},
                "clients":{"type":"object","additionalProperties":{"type":"object",
                    "properties":{"secret":{"type":"string"}},"required":["secret"]}}
            },"additionalProperties":false}"#,
        )
        .unwrap()
    }

    fn complete(source: &str, schema: &SchemaBundle) -> Vec<Completion> {
        completions(
            source,
            source.encode_utf16().count(),
            false,
            Some(schema),
            true,
            &JsonBehavior,
        )
    }

    fn accept_at(marked: &str, label: &str, schema: &SchemaBundle) -> String {
        accept_marked(marked, label, schema, &JsonBehavior)
    }

    fn nested_schema() -> SchemaBundle {
        build_schema(
            r#"{"type":"object","properties":{"web":{"type":"object","properties":{
                "http":{"type":"integer"},
                "letsencrypt":{"type":"object","required":["email"],
                    "properties":{"email":{"type":"string"}}}}}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_nested_object_accepted_after_an_inline_brace_is_formatted_as_a_block() {
        let schema = nested_schema();
        assert_eq!(
            accept_at("{\n  \"web\": {\"le|}\n}", "letsencrypt", &schema),
            "{\n  \"web\": {\n    \"letsencrypt\": {\n      \"email\": \"|\"\n    }\n  }\n}"
        );
    }

    #[test]
    fn a_nested_block_before_a_following_entry_keeps_it_on_its_own_line() {
        let schema = nested_schema();
        assert_eq!(
            accept_at(
                "{\n  \"web\": {\"le|\", \"http\": 80}\n}",
                "letsencrypt",
                &schema
            ),
            "{\n  \"web\": {\n    \"letsencrypt\": {\n      \"email\": \"|\"\n    },\n    \"http\": 80}\n}"
        );
    }

    #[test]
    fn a_scalar_accepted_after_an_inline_brace_stays_inline() {
        let schema = nested_schema();
        assert_eq!(
            accept_at("{\n  \"web\": {\"ht|}\n}", "http", &schema),
            "{\n  \"web\": {\"http\": |}\n}"
        );
    }

    #[test]
    fn a_nested_object_on_its_own_line_is_unchanged() {
        let schema = nested_schema();
        assert_eq!(
            accept_at("{\n  \"web\": {\n    \"le|\n  }\n}", "letsencrypt", &schema),
            "{\n  \"web\": {\n    \"letsencrypt\": {\n      \"email\": \"|\"\n    }\n  }\n}"
        );
    }

    #[test]
    fn duplicate_keys_are_a_parse_error_on_the_repeated_line() {
        let error = JsonBehavior
            .parse("{\n  \"listen\": 1,\n  \"listen\": 2\n}")
            .unwrap_err();
        assert_eq!(error.line, Some(3));
        assert!(error.message.contains("duplicate"), "{}", error.message);
    }

    #[test]
    fn same_key_in_different_objects_is_not_a_duplicate() {
        let source = r#"{"clients": {"a": {"secret": "x"}}, "auth": {"secret": "y"}}"#;
        assert!(JsonBehavior.parse(source).is_ok());
    }

    #[test]
    fn syntax_errors_report_a_position() {
        let error = JsonBehavior.parse("{\"listen\": }").unwrap_err();
        assert_eq!(error.line, Some(1));
        assert!(error.column.is_some());
    }

    #[test]
    fn keys_are_suggested_and_a_partial_key_is_replaced_with_the_quoted_key() {
        let source = "{\n  \"lis";
        let options = complete(source, &schema());
        let option = options
            .iter()
            .find(|option| option.label == "listen")
            .expect("listen should be suggested");
        let (updated, _, _) = apply_completion(source, option);
        assert_eq!(updated, "{\n  \"listen\": ");
    }

    #[test]
    fn keys_already_in_the_object_are_not_suggested_again() {
        let source = "{\n  \"listen\": 1,\n  \"li";
        assert!(
            complete(source, &schema())
                .iter()
                .all(|option| option.label != "listen")
        );
    }

    #[test]
    fn boolean_values_are_suggested_after_the_colon() {
        let source = "{\"insecure_plaintext\": t";
        let option = complete(source, &schema())
            .into_iter()
            .find(|option| option.label == "true")
            .expect("true should be suggested");
        let (updated, _, _) = apply_completion(source, &option);
        assert_eq!(updated, "{\"insecure_plaintext\": true");
    }

    #[test]
    fn keys_inside_an_inline_object_are_suggested_from_the_schema() {
        let source = "{\"clients\": {\"acme\": {\"sec";
        let option = complete(source, &schema())
            .into_iter()
            .find(|option| option.label == "secret")
            .expect("a key typed inside an inline object should be suggested");
        let (updated, _, _) = apply_completion(source, &option);
        assert_eq!(updated, "{\"clients\": {\"acme\": {\"secret\": \"\"");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn a_wrong_value_is_reported_on_the_value() {
        let source = "{\n  \"listen\": \"x\"\n}";
        let diagnostics = validate(source, Some(&schema()), None, true, &JsonBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
        assert_eq!(diagnostics[0].column, Some(13));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_unknown_key_is_reported_on_its_key() {
        let source = "{\n  \"bogus\": 1\n}";
        let diagnostics = validate(source, Some(&schema()), None, true, &JsonBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
        assert_eq!(diagnostics[0].column, Some(3));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_error_inside_an_inline_object_is_reported_on_its_own_line() {
        let source = "{\n  \"clients\": {\"acme\": {\"secret\": 1}}\n}";
        let diagnostics = validate(source, Some(&schema()), None, true, &JsonBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
        let line = source.lines().nth(1).unwrap();
        assert_eq!(diagnostics[0].column, Some(line.find('1').unwrap() + 1));
    }

    #[test]
    fn hovering_a_key_returns_its_schema_documentation() {
        let source = "{\n  \"listen\": 1\n}";
        let info = key_hover(source, 1, 3, Some(&schema()), &JsonBehavior)
            .expect("listen should have docs");
        assert_eq!(info.key, "listen");
        assert_eq!(info.description.as_deref(), Some("Port"));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn a_new_entry_after_a_value_without_a_comma_gets_one() {
        let source = r#"{"listen": 1"ins"#;
        let option = complete(source, &schema())
            .into_iter()
            .find(|option| option.label == "insecure_plaintext")
            .expect("insecure_plaintext should be suggested");
        let (updated, _, _) = apply_completion(source, &option);
        assert_eq!(updated, r#"{"listen": 1, "insecure_plaintext": "#);
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn typing_an_opening_quote_in_an_object_lists_every_key() {
        let options = complete("{\n  \"", &schema());
        assert!(options.iter().any(|option| option.label == "listen"));
        assert!(options.iter().any(|option| option.label == "clients"));
    }

    #[test]
    fn enter_between_braces_puts_the_caret_on_an_indented_middle_line() {
        let super::Newline { text, caret, .. } = JsonBehavior.newline("{}", 1);
        assert_eq!(text, "\n  \n");
        assert_eq!(caret, 3);
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn a_key_inserted_before_an_existing_key_gets_a_comma_after_it() {
        let source = r#"{"listen": 1}"#;
        let option = completions(source, 1, true, Some(&schema()), true, &JsonBehavior)
            .into_iter()
            .find(|option| option.label == "insecure_plaintext")
            .expect("insecure_plaintext should be suggested");
        let (updated, _, caret) = apply_completion(source, &option);
        assert_eq!(updated, r#"{"insecure_plaintext": , "listen": 1}"#);
        assert_eq!(caret, r#"{"insecure_plaintext": "#.encode_utf16().count());
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn completing_a_key_before_its_existing_colon_swallows_the_colon() {
        let source = r#"{"insecure_pla": false}"#;
        let cursor = r#"{"insecure_pla"#.encode_utf16().count();
        let context = JsonBehavior.completion_context(source, cursor);
        assert!(context.replace_start <= context.replace_end);
        let option = completions(source, cursor, false, Some(&schema()), true, &JsonBehavior)
            .into_iter()
            .find(|option| option.label == "insecure_plaintext")
            .expect("insecure_plaintext should be suggested");
        let (updated, _, _) = apply_completion(source, &option);
        assert_eq!(updated, r#"{"insecure_plaintext": false}"#);
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn the_reported_text_is_flagged_and_its_keys_get_separators() {
        let source =
            "{\n  \"listen\": 7835,\n  \"clients\": {\"secrets\": []\"max_connections\":324}\n}";
        let error = JsonBehavior.parse(source).unwrap_err();
        assert_eq!(error.line, Some(3));
        let cursor = "{\n  \"listen\": 7835,\n  \"clients\": {\"secrets\": []\"max"
            .encode_utf16()
            .count();
        let context = JsonBehavior.completion_context(source, cursor);
        assert_eq!(context.entry_prefix, ", ");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn every_caret_position_in_messy_documents_is_handled_without_panicking() {
        let schema = schema();
        let samples = [
            "",
            "{",
            "}",
            "]]",
            "{\"a\": [1, {\"b\": ",
            "{\n  \"héllo\": \"wörld\" \"x\"\n}",
            "[[[]]]",
            "{\"a\":1,,\"b\"::}",
            "{\"clients\": {\"secrets\": []\"max_connections\":324}}",
            "\"unterminated\n{\"a\": \"",
            "{\"k\": tru\n",
            "{\n  \"listen\": 7835,\n  \"clients\": {\"alex\": {\"secret\": \"x\"}\n}",
            "{\"emoji🙂\": [\"🙂\", 1 2 3]}",
        ];
        for source in samples {
            let caret_bytes = source
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(source.len()));
            for byte in caret_bytes {
                let cursor = source[..byte].encode_utf16().count();
                let _ = JsonBehavior.completion_context(source, cursor);
                let _ = JsonBehavior.newline(source, cursor);
                let _ = JsonBehavior.lexical_state(source, cursor);
                for force in [false, true] {
                    for option in
                        completions(source, cursor, force, Some(&schema), true, &JsonBehavior)
                    {
                        let _ = apply_completion(source, &option);
                    }
                }
            }
            let _ = JsonBehavior.parse(source);
            let _ = validate(source, Some(&schema), None, true, &JsonBehavior);
            for (line_index, line) in source.split('\n').enumerate() {
                for column in 0..12 {
                    let _ = JsonBehavior.hover_cursor(line, column);
                    let _ = key_hover(source, line_index, column, Some(&schema), &JsonBehavior);
                }
            }
        }
    }

    #[test]
    fn enter_inside_braces_indents_one_level() {
        // Between `{}` the closer moves down and the caret sits on the middle line.
        assert_eq!(JsonBehavior.newline("{}", 1), super::Newline::with_caret("\n  \n", 3));
        assert_eq!(
            JsonBehavior.newline("{\n  \"a\": 1,", 12).text,
            "\n  "
        );
    }

    #[test]
    fn json_has_no_comments_so_toggling_is_unsupported() {
        assert!(JsonBehavior.toggle_comments("{}", 0, 2).is_none());
    }

    #[test]
    fn an_open_string_is_reported_to_the_pairing_guard() {
        assert_eq!(JsonBehavior.lexical_state("{\"a\": \"x", 8).quote, Some('"'));
        assert_eq!(JsonBehavior.lexical_state("{\"a\": \"x\"", 9).quote, None);
    }

    #[test]
    fn an_object_with_required_children_expands_with_the_caret_on_the_first_value() {
        let schema = schema();
        let auth = &schema.schema["properties"]["auth"];
        let (insert, caret) = JsonBehavior
            .property_insert("auth", auth, &schema.schema, &INSERT_AT_ROOT)
            .unwrap();
        assert_eq!(insert, "\"auth\": {\n  \"enabled\": false\n}");
        assert_eq!(caret, "\"auth\": {\n  \"enabled\": ".encode_utf16().count());
    }

    fn hl(line: &str) -> Vec<(Option<TokenKind>, String)> {
        runs(line, &highlight(line))
    }

    fn run(kind: Option<TokenKind>, text: &str) -> (Option<TokenKind>, String) {
        (kind, text.to_string())
    }

    #[test]
    fn a_string_followed_by_a_colon_is_a_key() {
        assert_eq!(
            hl(r#"  "name" : "x","#),
            vec![
                run(None, "  "),
                run(Some(TokenKind::Key), r#""name""#),
                run(None, " "),
                run(Some(TokenKind::Punctuation), ":"),
                run(None, " "),
                run(Some(TokenKind::String), r#""x""#),
                run(Some(TokenKind::Punctuation), ","),
            ]
        );
    }

    #[test]
    fn numbers_booleans_and_null_are_literals_and_other_words_are_values() {
        assert_eq!(
            hl("[1, -2.5e3, true, false, null, nope]"),
            vec![
                run(Some(TokenKind::Punctuation), "["),
                run(Some(TokenKind::Literal), "1"),
                run(Some(TokenKind::Punctuation), ","),
                run(None, " "),
                run(Some(TokenKind::Literal), "-2.5e3"),
                run(Some(TokenKind::Punctuation), ","),
                run(None, " "),
                run(Some(TokenKind::Literal), "true"),
                run(Some(TokenKind::Punctuation), ","),
                run(None, " "),
                run(Some(TokenKind::Literal), "false"),
                run(Some(TokenKind::Punctuation), ","),
                run(None, " "),
                run(Some(TokenKind::Literal), "null"),
                run(Some(TokenKind::Punctuation), ","),
                run(None, " "),
                run(Some(TokenKind::Value), "nope"),
                run(Some(TokenKind::Punctuation), "]"),
            ]
        );
    }

    #[test]
    fn every_bracket_and_separator_is_punctuation() {
        assert_eq!(
            hl("{}[],:"),
            "{}[],:"
                .chars()
                .map(|c| run(Some(TokenKind::Punctuation), &c.to_string()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        assert_eq!(
            hl(r#""a\"b": "c\"""#),
            vec![
                run(Some(TokenKind::Key), r#""a\"b""#),
                run(Some(TokenKind::Punctuation), ":"),
                run(None, " "),
                run(Some(TokenKind::String), r#""c\"""#),
            ]
        );
    }

    #[test]
    fn an_unterminated_string_runs_to_the_line_end() {
        assert_eq!(
            hl(r#""k": "open, [1"#),
            vec![
                run(Some(TokenKind::Key), r#""k""#),
                run(Some(TokenKind::Punctuation), ":"),
                run(None, " "),
                run(Some(TokenKind::String), r#""open, [1"#),
            ]
        );
    }

    #[test]
    fn highlighting_covers_every_character_of_tricky_lines() {
        for line in [
            "",
            "   ",
            r#"{"clé": "日本語 🎉"}  "#,
            r#""unterminated \""#,
            r#""esc\\": "q\"x""#,
            "\t1,\t",
            "🎉: wörd",
        ] {
            let text: String = hl(line).into_iter().map(|(_, text)| text).collect();
            assert_eq!(text, line);
        }
    }
}
