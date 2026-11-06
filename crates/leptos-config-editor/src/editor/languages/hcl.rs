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

use super::super::commands::indent_unit;
use super::super::highlight::{Token as HighlightToken, TokenKind, Tokens};
use super::super::inline::{InlineEntry as CaretEntry, inline_caret, inline_key_at, inline_key_paths, inline_value_len, statement_start, statements, entry_follows};
use super::super::completion::{
    array_branch, object_branch, placeholder, schema_const, schema_default,
};
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::pointer_segments;
use super::super::language::{CompletionContext, InlineBlock, InsertContext, ConfigLanguage, LanguageInfo, Lexical, Span, ParseError, SchemaPathSegment,
    inline_block, unclosed_brackets,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) struct HclBehavior;


impl ConfigLanguage for HclBehavior {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "HCL",
            icon: "H",
            extensions: &["hcl", "tf", "tfvars"],
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
        property_insert(key, property, root, context.base_indent, context.unit)
    }

    fn format_value(&self, value: &Value) -> String {
        hcl_text(value)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        key_at_column(line, column)
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

    fn newline(&self, source: &str, cursor: usize) -> (String, usize) {
        newline_insertion(source, cursor)
    }

    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        toggle_hcl_line_comments(source, selection_start, selection_end)
    }

    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let (quote, in_comment) = hcl_lexical_state(source, cursor);
        Lexical { quote, in_comment }
    }


    /// An object whose keys are all identifiers becomes a block (`web { … }`),
    /// anything else an attribute. `hcl-rs` escapes `${` as `$${`.
    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        let Value::Object(map) = value else {
            return Err("an HCL document is a body: the top level must be an object".to_string());
        };
        hcl::format::to_string(&hcl_body(map)?).map_err(|error| error.to_string())
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

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `name`, `true`, `1.5`, or another bare word. Attribute names, block
    /// types, and unquoted label words all arrive as identifiers.
    Ident,
    /// A quoted string. `closed` is false while the closing quote is missing.
    Str { closed: bool },
    /// A heredoc (`<<EOT ... EOT`) consumed whole, so its body never scans as
    /// structure. Always spans at least the opener line.
    Heredoc,
    /// A `${ ... }` / `%{ ... }` template sequence outside strings, skipped as
    /// one opaque value token.
    Template,
    /// Any other character: `{ } [ ] ( ) = , .` and stray characters, which
    /// scan as their own punctuation so the walk cannot stall.
    Punct(char),
    /// A `#`, `//`, or `/* */` comment.
    Comment,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    start: usize,
    end: usize,
    kind: Kind,
    /// Whether a line break sits between this token and the previous one.
    nl_before: bool,
}

/// Splits `source` into tokens. Never fails: every token consumes at least one
/// byte, and unterminated constructs end at their line end or the input end.
fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut nl_before = true;
    let mut index = 0;
    while index < source.len() {
        let character = source[index..].chars().next().unwrap_or('\0');
        if character.is_whitespace() {
            nl_before |= character == '\n';
            index += character.len_utf8();
            continue;
        }
        let (end, kind, spans_lines) = scan_token(source, index);
        tokens.push(Token {
            start: index,
            end,
            kind,
            nl_before,
        });
        // A token that spans lines (block comment, heredoc) leaves the next
        // token with a line break before it.
        nl_before = spans_lines;
        index = end;
    }
    tokens
}

/// The token starting at `index`: its end byte, its kind, and whether it spans
/// a line break.
fn scan_token(source: &str, index: usize) -> (usize, Kind, bool) {
    let rest = &source[index..];
    let character = rest.chars().next().unwrap_or('\0');
    if character == '#' || rest.starts_with("//") {
        return (line_end(source, index), Kind::Comment, false);
    }
    if let Some(body) = rest.strip_prefix("/*") {
        let end = body
            .find("*/")
            .map_or(source.len(), |offset| index + 2 + offset + 2);
        return (end, Kind::Comment, source[index..end].contains('\n'));
    }
    if character == '"' {
        let (end, closed) = string_end(source, index);
        return (end, Kind::Str { closed }, false);
    }
    if rest.starts_with("<<")
        && let Some(end) = heredoc_end(source, index)
    {
        return (end, Kind::Heredoc, source[index..end].contains('\n'));
    }
    if rest.starts_with("${") || rest.starts_with("%{") {
        return (template_end(source, index), Kind::Template, false);
    }
    if character.is_ascii_alphabetic() || character == '_' {
        let end = run_end(source, index, |next| {
            next.is_ascii_alphanumeric() || matches!(next, '_' | '-')
        });
        return (end, Kind::Ident, false);
    }
    if character.is_ascii_digit() {
        let end = run_end(source, index, |next| {
            next.is_ascii_alphanumeric() || matches!(next, '.' | '_' | '+' | '-')
        });
        return (end, Kind::Ident, false);
    }
    (index + character.len_utf8(), Kind::Punct(character), false)
}

/// End byte of the line holding `start`, excluding the newline.
fn line_end(source: &str, start: usize) -> usize {
    source[start..]
        .find('\n')
        .map_or(source.len(), |offset| start + offset)
}

/// End byte of the run of characters from `start` that satisfy `keep`.
fn run_end(source: &str, start: usize, keep: impl Fn(char) -> bool) -> usize {
    source[start..]
        .char_indices()
        .find(|(_, character)| !keep(*character))
        .map_or(source.len(), |(offset, _)| start + offset)
}

/// End byte of the string opening at `start`, and whether its closing quote is
/// present. A string ends at its closing quote or at the line end.
fn string_end(source: &str, start: usize) -> (usize, bool) {
    let mut escaped = false;
    for (offset, character) in source[start + 1..].char_indices() {
        if character == '\n' {
            return (start + 1 + offset, false);
        }
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return (start + 1 + offset + 1, true);
        }
    }
    (source.len(), false)
}

/// Byte end of the heredoc opening at `start` (`<<MARKER` / `<<-MARKER`),
/// through the terminator line. Returns `None` when the opener has no marker,
/// so the caller falls back to punctuation tokens.
fn heredoc_end(source: &str, start: usize) -> Option<usize> {
    let mut rest = &source[start + 2..];
    if let Some(stripped) = rest.strip_prefix('-') {
        rest = stripped;
    }
    rest = rest.trim_start_matches([' ', '\t']);
    let marker_len = rest
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .map(char::len_utf8)
        .sum::<usize>();
    if marker_len == 0 {
        return None;
    }
    let marker = &rest[..marker_len];
    let opener_end = source[start..]
        .find('\n')
        .map_or(source.len(), |offset| start + offset + 1);
    let mut cursor = opener_end;
    while cursor <= source.len() {
        let line_end = source[cursor..]
            .find('\n')
            .map_or(source.len(), |offset| cursor + offset);
        if source[cursor..line_end].trim() == marker {
            return Some(line_end.min(source.len()) + usize::from(line_end < source.len()));
        }
        if line_end == source.len() {
            break;
        }
        cursor = line_end + 1;
    }
    Some(source.len())
}

/// Byte end of the `${ ... }` / `%{ ... }` template opening at `start`,
/// skipping nested braces and quoted strings.
fn template_end(source: &str, start: usize) -> usize {
    let mut depth = 0;
    let mut index = start;
    let mut quote = None;
    let mut escaped = false;
    while index < source.len() {
        let character = source[index..].chars().next().unwrap_or('\0');
        if let Some(active) = quote {
            if active == '"' && escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
        } else if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if character == '{' {
            depth += 1;
        } else if character == '}' {
            depth -= 1;
            if depth == 0 {
                return index + 1;
            }
        }
        index += character.len_utf8();
    }
    source.len()
}

/// The text of a quoted label token without its quotes, unescaped when closed.
fn decode_string(text: &str, closed: bool) -> String {
    if closed {
        serde_json::from_str::<String>(text).unwrap_or_else(|_| text.trim_matches('"').to_string())
    } else {
        text.strip_prefix('"').unwrap_or(text).to_string()
    }
}

fn token_name(source: &str, token: &Token) -> Option<String> {
    match token.kind {
        Kind::Ident => Some(source[token.start..token.end].to_string()),
        Kind::Str { closed } => Some(decode_string(&source[token.start..token.end], closed)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Structure scan
// ---------------------------------------------------------------------------

/// One `type "label" ... { ... }` block: the body it opens and where it sits.
struct HeaderInfo {
    parent: usize,
    child: usize,
    block_type: String,
    labels: Vec<String>,
    /// Position among the same-`(type, labels)` headers of the parent body. The
    /// JSON mapping renders repeats as arrays, so the ordinal becomes an index
    /// segment (`/variable/name/1/...`).
    ordinal: usize,
    /// Read only by schema-path span lookups.
    type_span: (usize, usize),
    label_spans: Vec<(usize, usize)>,
}

/// One `name = value` attribute: the body it sits in and its byte spans.
struct AttributeInfo {
    body: usize,
    name: String,
    /// Key chain from the owning body-level `name = {` assignment down to this
    /// entry. Empty for attributes written directly in a body; `["acme"]` for
    /// `secret` in `clients = { acme = { secret = ... } }`. Lets completion and
    /// error spans descend into nested value maps.
    prefix: Vec<String>,
    /// Read only by schema-path span lookups.
    key_span: (usize, usize),
    value_span: (usize, usize),
}

struct Snapshot {
    body: usize,
    in_value: bool,
    value_key: Option<String>,
    /// Key chain from the owning body-level assignment into the map layer at
    /// the caret (`["clients", "acme"]` inside `clients = { acme = { | } }`).
    /// Completion descends through it into the map's schema.
    value_prefix: Vec<String>,
    /// Whether the caret sits inside `{ }` of a value map rather than at a
    /// plain value position or inside an array.
    in_map: bool,
    value_start: usize,
    partial: Option<Token>,
}

struct Scan {
    headers: Vec<HeaderInfo>,
    /// Header occurrence totals keyed by `(parent body, type, labels)`.
    totals: HashMap<(usize, String, String), usize>,
    attributes: Vec<AttributeInfo>,
    snapshot: Snapshot,
}

/// An attribute value being read. It ends at a newline at bracket depth zero,
/// or at a closing bracket that belongs to an enclosing body.
struct OpenValue {
    attribute: usize,
    depth: usize,
    started: bool,
    /// Open `{` layers inside the value (the value's own map is the first).
    braces: usize,
    /// Open `[` / `(` layers inside the value. Array items are not map keys,
    /// so key completion and nested key spans stay off inside them.
    containers: usize,
    /// Key chain below the owning attribute for the innermost open map layer
    /// (`["acme"]` inside `clients = { acme = { ... } }`).
    map_keys: Vec<String>,
    /// The `key =` just read inside the map, awaiting its `{`.
    pending_key: Option<String>,
    /// The nested attribute whose value is still being read, so its span can
    /// grow until the next key or closing brace.
    nested_value: Option<usize>,
}

impl Scan {
    /// Whether `header` repeats an earlier header with the same type and labels.
    fn repeats(&self, header: &HeaderInfo) -> bool {
        self.totals
            .get(&(
                header.parent,
                header.block_type.clone(),
                header.labels.join("\0"),
            ))
            .is_some_and(|total| *total > 1)
    }
}

/// Finalized path of `body`: the parent path plus each owning header's type and
/// labels, with an ordinal segment when the header repeats.
fn body_path(scan: &Scan, body: usize) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = body;
    while let Some(header) = scan.headers.iter().find(|header| header.child == current) {
        chain.push(header);
        current = header.parent;
    }
    let mut segments = Vec::new();
    for header in chain.into_iter().rev() {
        segments.push(header.block_type.clone());
        segments.extend(header.labels.iter().cloned());
        if scan.repeats(header) {
            segments.push(header.ordinal.to_string());
        }
    }
    segments
}

/// Schema path of an attribute: its body's path, its map prefix (empty at the
/// top level), plus its key.
fn attribute_path(scan: &Scan, attribute: &AttributeInfo) -> Vec<String> {
    let mut path = body_path(scan, attribute.body);
    path.extend(attribute.prefix.iter().cloned());
    path.push(attribute.name.clone());
    path
}

/// Index of the first non-comment token at or after `from`.
fn next_significant(tokens: &[Token], from: usize) -> Option<usize> {
    (from..tokens.len()).find(|&next| tokens[next].kind != Kind::Comment)
}

/// Map key chain for an entry written inside the value being read: the owning
/// attribute's own prefix and name, then each `key = {` layer below it.
fn map_prefix(attributes: &[AttributeInfo], value: &OpenValue) -> Vec<String> {
    let owner = &attributes[value.attribute];
    let mut prefix = owner.prefix.clone();
    prefix.push(owner.name.clone());
    prefix.extend(value.map_keys.iter().cloned());
    prefix
}

/// Captures the parser state at the caret: the innermost body, and the
/// attribute whose value the caret sits in, if any.
fn take_snapshot(
    attributes: &[AttributeInfo],
    stack: &[usize],
    open: Option<&OpenValue>,
    cursor: usize,
    partial: Option<Token>,
) -> Snapshot {
    let body = stack.last().copied().unwrap_or(0);
    match open {
        Some(value) => {
            let attribute = &attributes[value.attribute];
            let in_map = value.braces >= 1 && value.containers == 0;
            let value_prefix = if in_map {
                map_prefix(attributes, value)
            } else {
                Vec::new()
            };
            Snapshot {
                body,
                in_value: true,
                value_key: Some(attribute.name.clone()),
                value_prefix,
                in_map,
                value_start: attribute.value_span.0,
                partial,
            }
        }
        None => Snapshot {
            body,
            in_value: false,
            value_key: None,
            value_prefix: Vec::new(),
            in_map: false,
            value_start: cursor,
            partial,
        },
    }
}

/// Walks the whole document, tolerating invalid syntax, and captures the
/// parser state at `cursor` (a byte offset). Block headers and attributes are
/// recognized by token shape, so a half-typed document still yields structure.
fn analyze(source: &str, cursor: usize) -> Scan {
    let tokens = tokenize(source);
    let partial_index = tokens.iter().position(|token| {
        token.start < cursor
            && cursor <= token.end
            && matches!(token.kind, Kind::Ident | Kind::Str { .. })
    });
    let partial = partial_index.map(|index| tokens[index]);
    let mut body_count = 1;
    let mut headers: Vec<HeaderInfo> = Vec::new();
    let mut totals: HashMap<(usize, String, String), usize> = HashMap::new();
    let mut attributes: Vec<AttributeInfo> = Vec::new();
    let mut snapshot: Option<Snapshot> = None;
    // Open bodies, innermost last. A stray `{` repeats the current body so that
    // its `}` pops cleanly without opening anything.
    let mut stack: Vec<usize> = vec![0];
    let mut open: Option<OpenValue> = None;

    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        // A line break at bracket depth zero ends the value being read.
        if token.nl_before
            && token.kind != Kind::Comment
            && open
                .as_ref()
                .is_some_and(|value| value.started && value.depth == 0)
        {
            open = None;
        }
        if snapshot.is_none() && (Some(index) == partial_index || token.start >= cursor) {
            snapshot = Some(take_snapshot(
                &attributes,
                &stack,
                open.as_ref(),
                cursor,
                partial,
            ));
        }
        if token.kind == Kind::Comment {
            index += 1;
            continue;
        }
        // Inside a value only brackets matter, plus the map layering that key
        // completion and nested key spans need. A closing bracket at depth
        // zero belongs to an enclosing body, so it ends the value and is read
        // below.
        let mut closes_value = false;
        if let Some(value) = open.as_mut() {
            match token.kind {
                Kind::Punct('{') => {
                    value.depth += 1;
                    value.braces += 1;
                    // An inner `key = {` layer: entries below it belong to the
                    // key chain; the value's own brace has no pending key.
                    if let Some(key) = value.pending_key.take() {
                        value.map_keys.push(key);
                    }
                }
                Kind::Punct('[' | '(') => {
                    value.depth += 1;
                    value.containers += 1;
                }
                Kind::Punct('}' | ']' | ')') if value.depth > 0 => {
                    value.depth -= 1;
                    value.pending_key = None;
                    value.nested_value = None;
                    match token.kind {
                        Kind::Punct('}') if value.braces > 0 => {
                            value.braces -= 1;
                            // Still inside the value's map: the inner layer
                            // closed, so its key leaves the chain.
                            if value.braces >= 1 {
                                value.map_keys.pop();
                            }
                        }
                        _ => value.containers = value.containers.saturating_sub(1),
                    }
                }
                Kind::Punct('}') | Kind::Punct(']') | Kind::Punct(')') => closes_value = true,
                _ => {}
            }
            if !closes_value {
                value.started = true;
                attributes[value.attribute].value_span.1 = token.end;
                // A `key =` written inside a map braces records the nested
                // attribute, so its schema path and spans resolve there. The
                // `=` is skipped so the next token (its value or `{`) is read
                // with the key pending.
                if value.braces >= 1
                    && value.containers == 0
                    && matches!(token.kind, Kind::Ident | Kind::Str { .. })
                    && let Some(equals) = next_significant(&tokens, index + 1)
                        .filter(|next| tokens[*next].kind == Kind::Punct('='))
                    && let Some(name) = token_name(source, &token)
                {
                    let value_start = next_significant(&tokens, equals + 1)
                        .map_or(source.len(), |next| tokens[next].start);
                    attributes.push(AttributeInfo {
                        body: stack.last().copied().unwrap_or(0),
                        name: name.clone(),
                        prefix: map_prefix(&attributes, value),
                        key_span: (token.start, token.end),
                        value_span: (value_start, value_start),
                    });
                    value.nested_value = Some(attributes.len() - 1);
                    value.pending_key = Some(name);
                    index = equals;
                    index += 1;
                    continue;
                }
                if let Some(nested) = value.nested_value {
                    attributes[nested].value_span.1 = token.end;
                } else {
                    value.pending_key = None;
                }
                index += 1;
                continue;
            }
            open = None;
        }
        let body = stack.last().copied().unwrap_or(0);
        match token.kind {
            Kind::Punct('{') => stack.push(body),
            Kind::Punct('}') => {
                if stack.len() > 1 {
                    stack.pop();
                }
            }
            Kind::Ident | Kind::Str { .. } => {
                let equals = next_significant(&tokens, index + 1)
                    .filter(|next| tokens[*next].kind == Kind::Punct('='));
                if let (Some(equals), Some(name)) = (equals, token_name(source, &token)) {
                    let value_start = next_significant(&tokens, equals + 1)
                        .map_or(source.len(), |next| tokens[next].start);
                    attributes.push(AttributeInfo {
                        body,
                        name,
                        prefix: Vec::new(),
                        key_span: (token.start, token.end),
                        value_span: (value_start, value_start),
                    });
                    open = Some(OpenValue {
                        attribute: attributes.len() - 1,
                        depth: 0,
                        started: false,
                        braces: 0,
                        containers: 0,
                        map_keys: Vec::new(),
                        pending_key: None,
                        nested_value: None,
                    });
                    index = equals;
                } else if let Some(brace) = classify_header(&tokens, index) {
                    let mut names: Vec<(String, (usize, usize))> = tokens[index..brace]
                        .iter()
                        .filter_map(|name_token| {
                            token_name(source, name_token)
                                .map(|name| (name, (name_token.start, name_token.end)))
                        })
                        .collect();
                    if !names.is_empty() {
                        let (block_type, type_span) = names.remove(0);
                        let labels: Vec<String> =
                            names.iter().map(|(name, _)| name.clone()).collect();
                        let label_spans: Vec<(usize, usize)> =
                            names.iter().map(|(_, span)| *span).collect();
                        let count = totals
                            .entry((body, block_type.clone(), labels.join("\0")))
                            .or_insert(0);
                        let ordinal = *count;
                        *count += 1;
                        let child = body_count;
                        body_count += 1;
                        headers.push(HeaderInfo {
                            parent: body,
                            child,
                            block_type,
                            labels,
                            ordinal,
                            type_span,
                            label_spans,
                        });
                        stack.push(child);
                    }
                    index = brace;
                }
            }
            _ => {}
        }
        index += 1;
    }

    let snapshot = snapshot
        .unwrap_or_else(|| take_snapshot(&attributes, &stack, open.as_ref(), cursor, partial));
    Scan {
        headers,
        totals,
        attributes,
        snapshot,
    }
}

/// Index of the `{` opening a block header whose names start at `index`, or
/// `None` when the tokens do not form one. A header is one or more names on a
/// single line followed by `{` on that same line.
fn classify_header(tokens: &[Token], index: usize) -> Option<usize> {
    let mut names = 0;
    for (offset, token) in tokens[index..].iter().enumerate() {
        match token.kind {
            Kind::Comment => {}
            Kind::Ident | Kind::Str { .. } => {
                if names > 0 && token.nl_before {
                    return None;
                }
                names += 1;
            }
            Kind::Punct('{') => {
                return (names > 0 && !token.nl_before).then_some(index + offset);
            }
            _ => return None,
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Parsing and spans
// ---------------------------------------------------------------------------

/// References, interpolations, and function calls arrive as their
/// `${ ... }`-style source text.
fn hcl_parse(source: &str) -> Result<Value, ParseError> {
    hcl::from_str::<Value>(source).map_err(|error| {
        // Only syntax errors carry a location; others fall back to the top.
        let (line, column) = match &error {
            hcl::Error::Parse(parse) => {
                let location = parse.location();
                (location.line(), location.column())
            }
            _ => (1, 1),
        };
        let line_count = source.split('\n').count().max(1);
        ParseError {
            message: readable_error(&error.to_string()),
            line: Some(line.clamp(1, line_count)),
            column: Some(column.max(1)),
        }
    })
}

/// The parser's sentence without its ASCII-art source frame (`--> …`, `|`,
/// `^---`): the position is reported separately.
fn readable_error(message: &str) -> String {
    let notes: Vec<&str> = message
        .lines()
        .filter_map(|line| line.trim().strip_prefix("= "))
        .collect();
    if notes.is_empty() {
        message.lines().next().unwrap_or(message).trim().to_string()
    } else {
        notes.join("; ")
    }
}

fn span_between(source: &str, start: usize, end: usize) -> (usize, usize, usize) {
    let (line, column) = line_column(source, start);
    (
        line,
        column,
        column + source[start..end.min(source.len())].chars().count(),
    )
}

fn hcl_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    if wanted.is_empty() {
        return None;
    }
    let scan = analyze(source, 0);
    if let Some(attribute) = scan
        .attributes
        .iter()
        .find(|attribute| attribute_path(&scan, attribute) == wanted)
    {
        return Some(span_between(
            source,
            attribute.key_span.0,
            attribute.key_span.1,
        ));
    }
    // A block type or label address (`/resource`, `/resource/aws_x`): point at
    // the header word or label token.
    if let Some((start, end)) = header_span(&scan, &wanted) {
        return Some(span_between(source, start, end));
    }
    inline_key(source, &wanted).map(|(start, end, _)| span_between(source, start, end))
}

/// A key nested in inline containers (`dns = [{ wait_secs = 60 }]`), which
/// the block scan does not index: `(key start, key end, value start)`.
fn inline_key(source: &str, wanted: &[String]) -> Option<(usize, usize, usize)> {
    let segment = |segment: &SchemaPathSegment| match segment {
        SchemaPathSegment::Key(key) => key.clone(),
        SchemaPathSegment::ArrayItem(index) => index.to_string(),
    };
    for (start, end) in statements(source) {
        for (path, key) in inline_key_paths(&source[start..end], b'=') {
            let tail: Vec<String> = path.iter().map(segment).collect();
            if !wanted.ends_with(&tail) {
                continue;
            }
            // The path above the statement, from the block scan at its start.
            let outer = analyze(source, start);
            let mut full = body_path(&outer, outer.snapshot.body);
            if outer.snapshot.in_map {
                full.extend(outer.snapshot.value_prefix.iter().cloned());
            }
            full.extend(tail);
            if full == wanted {
                return Some((start + key.start, start + key.end, start + key.after_separator));
            }
        }
    }
    None
}

fn hcl_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    let scan = analyze(source, 0);
    let Some(attribute) = scan
        .attributes
        .iter()
        .find(|attribute| attribute_path(&scan, attribute) == wanted)
    else {
        let (_, _, value_start) = inline_key(source, &wanted)?;
        let start = value_start + (source[value_start..].len() - source[value_start..].trim_start().len());
        if source[start..].starts_with(['{', '[']) {
            let (line, column) = line_column(source, start);
            return Some((line, column, column + 1));
        }
        let end = start + inline_value_len(&source[start..]);
        return (end > start).then(|| span_between(source, start, end));
    };
    let (start, end) = attribute.value_span;
    if end <= start {
        return None;
    }
    if source[start..].starts_with(['{', '[']) {
        // A container is marked at its opening bracket, not its whole body.
        let (line, column) = line_column(source, start);
        return Some((line, column, column + 1));
    }
    Some(span_between(source, start, end))
}

/// Span of a block header word: the type identifier when the path names the
/// block type, or the label token when it names a label.
fn header_span(scan: &Scan, wanted: &[String]) -> Option<(usize, usize)> {
    for header in &scan.headers {
        let mut base = body_path(scan, header.parent);
        base.push(header.block_type.clone());
        if base == wanted {
            return Some(header.type_span);
        }
        for (level, span) in header.label_spans.iter().enumerate() {
            let mut prefix = base.clone();
            prefix.extend(header.labels[..=level].iter().cloned());
            if prefix == wanted {
                return Some(*span);
            }
        }
    }
    None
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
    let snapshot = &scan.snapshot;
    let path: Vec<SchemaPathSegment> = body_path(&scan, snapshot.body)
        .into_iter()
        .map(|segment| match segment.parse::<usize>() {
            Ok(index) => SchemaPathSegment::ArrayItem(index),
            Err(_) => SchemaPathSegment::Key(segment),
        })
        .collect();

    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let base_indent: String = source[line_start..]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let line_end = source[cursor..]
        .find('\n')
        .map_or(source.len(), |index| cursor + index);
    let line = &source[line_start..line_end];
    let caret_in_line = cursor.saturating_sub(line_start).min(line.len());
    let before_caret = &line[..caret_in_line];

    // The caret sits in a comment: offer nothing typed, keep the path.
    if line_comment_start(before_caret).is_some() {
        return CompletionContext {
            path,
            typed: String::new(),
            replace_start: utf16(&source[..cursor]),
            replace_end: utf16(&source[..cursor]),
            value_key: None,
            value_needs_space: false,
            is_array_item: false,
            base_indent,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
            ..Default::default()
        };
    }

    // Inside containers opened on this line (`a = { b = [{ … }, { | }] }`):
    // the path above the line comes from the scan at the line start, the
    // rest from the line itself.
    // The statement may span lines (`certificates = [` … the caret).
    if let Some(statement) = statement_start(&source[..cursor])
        && let Some(caret) = inline_caret(&source[statement..cursor], b'=')
    {
        let before_caret = &source[statement..cursor];
        let outer = analyze(source, statement);
        let mut full: Vec<SchemaPathSegment> = body_path(&outer, outer.snapshot.body)
            .into_iter()
            .map(|segment| match segment.parse::<usize>() {
                Ok(index) => SchemaPathSegment::ArrayItem(index),
                Err(_) => SchemaPathSegment::Key(segment),
            })
            .collect();
        if outer.snapshot.in_map {
            full.extend(outer.snapshot.value_prefix.iter().cloned().map(SchemaPathSegment::Key));
        }
        full.extend(caret.path);
        let value_end = cursor + inline_value_len(&source[cursor..line_end]);
        let typed = |start: usize| before_caret[start..].trim_end().trim_start_matches('"').to_string();
        return match caret.entry {
            CaretEntry::Key { start } => CompletionContext {
                typed: typed(start),
                replace_start: utf16(&source[..statement + start]),
                replace_end: utf16(&source[..swallow_existing_assignment(source, cursor)]),
                explicit_entry_start: true,
                block: hcl_block(source, statement + start, cursor),
                base_indent,
                path: full,
                ..Default::default()
            },
            CaretEntry::Value { key, start } => CompletionContext {
                typed: typed(start),
                replace_start: utf16(&source[..statement + start]),
                replace_end: utf16(&source[..value_end]),
                value_key: Some(key),
                value_needs_space: start == before_caret.len() && !before_caret.ends_with([' ', '\t']),
                base_indent,
                path: full,
                ..Default::default()
            },
            CaretEntry::Item { index, start } => {
                full.push(SchemaPathSegment::ArrayItem(index));
                CompletionContext {
                    typed: typed(start),
                    replace_start: utf16(&source[..statement + start]),
                    replace_end: utf16(&source[..value_end]),
                    is_array_item: true,
                    entry_wrap: Some(("{ ", " }")),
                    base_indent,
                    path: full,
                    ..Default::default()
                }
            }
        };
    }

    // Value position.
    if snapshot.in_value
        && let Some(key) = snapshot.value_key.clone()
    {
        // Inside `key = { … }` the caret completes a nested map key, at any depth.
        if snapshot.in_map {
            let mut map_path = path.clone();
            map_path.extend(
                snapshot
                    .value_prefix
                    .iter()
                    .cloned()
                    .map(SchemaPathSegment::Key),
            );
            // `mode = |` on its own line inside a multi-line map.
            if let Some((key, start)) = line_assignment(before_caret) {
                let typed_start = line_start + start;
                let value_end = cursor + inline_value_len(&source[cursor..line_end]);
                return CompletionContext {
                    typed: source[typed_start..cursor].trim_end().to_string(),
                    replace_start: utf16(&source[..typed_start]),
                    replace_end: utf16(&source[..value_end]),
                    value_key: Some(key),
                    value_needs_space: typed_start == cursor && !before_caret.ends_with([' ', '\t']),
                    base_indent,
                    path: map_path,
                    ..Default::default()
                };
            }
            let mut map_path = path.clone();
            map_path.extend(
                snapshot
                    .value_prefix
                    .iter()
                    .cloned()
                    .map(SchemaPathSegment::Key),
            );
            let (typed_start, typed) = match snapshot.partial {
                Some(token) if token.start < cursor => {
                    let text = source[token.start..cursor].to_string();
                    let text = match token.kind {
                        Kind::Str { .. } => text.strip_prefix('"').unwrap_or(&text).to_string(),
                        _ => text,
                    };
                    (token.start, text.trim_end().to_string())
                }
                _ => (cursor, String::new()),
            };
            let replace_end = swallow_existing_assignment(
                source,
                snapshot
                    .partial
                    .map_or(cursor, |token| token.end.max(cursor))
                    .min(source.len()),
            );
            return CompletionContext {
                block: hcl_block(source, typed_start, replace_end),
                typed,
                replace_start: utf16(&source[..typed_start]),
                replace_end: utf16(&source[..replace_end]),
                value_key: None,
                value_needs_space: false,
                is_array_item: false,
                base_indent,
                path: map_path,
                entry_prefix: String::new(),
                explicit_entry_start: true,
                entry_suffix: String::new(),
                ..Default::default()
            };
        }
        let typed_start = snapshot
            .partial
            .map_or(cursor, |token| token.start.max(snapshot.value_start))
            .min(cursor);
        let replace_end = snapshot
            .partial
            .map_or(cursor, |token| token.end.max(cursor))
            .min(line_end);
        return CompletionContext {
            typed: source[typed_start..cursor].trim_end().to_string(),
            replace_start: utf16(&source[..typed_start]),
            replace_end: utf16(&source[..replace_end]),
            value_key: Some(key),
            // Only directly after `=`; a space already typed is kept as is.
            value_needs_space: snapshot.partial.is_none() && source[..cursor].ends_with('='),
            is_array_item: false,
            base_indent,
            path,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
            ..Default::default()
        };
    }
    if let Some((key, value_begin)) = line_assignment(before_caret) {
        let typed_start = line_start + value_begin;
        let value_len = value_token_len(&source[cursor..line_end]);
        return CompletionContext {
            typed: source[typed_start..cursor].trim_end().to_string(),
            replace_start: utf16(&source[..typed_start]),
            replace_end: utf16(&source[..cursor + value_len]),
            value_key: Some(key),
            value_needs_space: source[typed_start..cursor].is_empty()
                && before_caret.trim_end().ends_with('='),
            is_array_item: false,
            base_indent,
            path,
            entry_prefix: String::new(),
            explicit_entry_start: false,
            entry_suffix: String::new(),
            ..Default::default()
        };
    }

    // Key position.
    let (typed_start, typed) = match snapshot.partial {
        Some(token) if token.start < cursor => {
            let text = source[token.start..cursor].to_string();
            let text = match token.kind {
                Kind::Str { .. } => text.strip_prefix('"').unwrap_or(&text).to_string(),
                _ => text,
            };
            (token.start, text.trim_end().to_string())
        }
        _ => (cursor, String::new()),
    };
    // Typed in front of an existing entry (`lim|listen = 1`): the new entry
    // goes before it, which moves to its own line (see `entry_after`).
    let rest = &source[cursor..line_end];
    let splits = entry_follows(rest, b'=');
    let word_after: String = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
        .collect();
    let replace_end = if splits {
        cursor
    } else {
        swallow_existing_assignment(
            source,
            snapshot
                .partial
                .map_or(cursor, |token| token.end.max(cursor))
                .min(source.len()),
        )
    };
    CompletionContext {
        block: hcl_block(source, typed_start, replace_end.min(source.len())),
        path,
        typed,
        replace_start: utf16(&source[..typed_start]),
        replace_end: utf16(&source[..replace_end.min(source.len())]),
        entry_suffix: if splits { format!("\n{base_indent}") } else { String::new() },
        entry_after: splits.then_some(word_after),
        base_indent,
        ..Default::default()
    }
}

/// `web = { le| }`: braces opened earlier on the caret's line are still open,
/// so a multi-line insert moves onto its own line inside them.
fn hcl_block(source: &str, typed_start: usize, replace_end: usize) -> Option<InlineBlock> {
    let line_start = source[..typed_start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    inline_block(
        source,
        typed_start,
        replace_end,
        unclosed_brackets(&source[line_start..typed_start]),
        &indent_unit(source),
        |rest| rest.starts_with(['}', ']']),
        ",",
    )
}

/// The assignment on a line fragment: the key before the first `=` outside
/// strings and comments, plus the byte offset (relative to the line start)
/// where the value begins. Comment-only fragments yield `None`.
fn line_assignment(line: &str) -> Option<(String, usize)> {
    let code = code_before_comment(line)?;
    let equals = assignment_equals(code)?;
    let raw = code[..equals].trim();
    if raw.is_empty() || !is_ident(raw) {
        return None;
    }
    let after = &code[equals + 1..];
    let value_begin = equals + 1 + (after.len() - after.trim_start_matches([' ', '\t']).len());
    Some((raw.to_string(), value_begin))
}

/// Extends a key replacement end past an `=` already typed after the key
/// (`key| = value`): the key insert supplies its own `" = "` and value, so the
/// old separator and value would otherwise be duplicated.
fn swallow_existing_assignment(source: &str, replace_end: usize) -> usize {
    let rest = &source[replace_end..];
    let trimmed = rest.trim_start_matches([' ', '\t']);
    let Some(after_equals) = trimmed.strip_prefix('=') else {
        return replace_end;
    };
    let spaces = after_equals.len() - after_equals.trim_start_matches([' ', '\t']).len();
    let value_start = replace_end + (rest.len() - trimmed.len()) + 1 + spaces;
    let value_line_end = source[value_start..]
        .find('\n')
        .map_or(source.len(), |index| value_start + index);
    value_start + value_token_len(&source[value_start..value_line_end])
}

/// Byte index of the `=` separating key from value, ignoring `=` inside
/// quoted strings and `==`, `=>`, `!=`, `<=`, `>=` operators.
fn assignment_equals(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let bytes = line.as_bytes();
    for (index, character) in line.char_indices() {
        if let Some(active) = quote {
            if active == '"' && escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
            continue;
        }
        if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if character == '=' {
            let prev = index
                .checked_sub(1)
                .and_then(|prev| bytes.get(prev))
                .copied();
            let next = bytes.get(index + 1).copied();
            if matches!(prev, Some(b'=' | b'!' | b'<' | b'>'))
                || next == Some(b'=')
                || next == Some(b'>')
            {
                continue;
            }
            return Some(index);
        }
    }
    None
}

fn is_ident(text: &str) -> bool {
    let mut characters = text.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && characters
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

/// Length of the scalar value starting at the caret: runs to the comment or
/// line end for bare scalars, to the closing quote for strings.
fn value_token_len(text: &str) -> usize {
    if text.trim_start().starts_with('"') {
        let trimmed = text.trim_start();
        let mut escaped = false;
        for (index, character) in trimmed.char_indices().skip(1) {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                return text.len() - trimmed.len() + index + 1;
            }
        }
        return trimmed.len();
    }
    match code_before_comment(text) {
        Some(code) => code.trim_end().len(),
        None => 0,
    }
}

/// Keys already present in the body holding the caret: attribute names plus,
/// at a label level, the labels already taken there. The partly typed key on
/// the active line is ignored so it can still be completed.
fn existing_body_keys(
    source: &str,
    cursor_byte: usize,
    path: &[SchemaPathSegment],
) -> Option<HashSet<String>> {
    let scan = analyze(source, cursor_byte);
    let wanted: Vec<String> = path
        .iter()
        .map(|segment| match segment {
            SchemaPathSegment::Key(key) => key.clone(),
            SchemaPathSegment::ArrayItem(index) => index.to_string(),
        })
        .collect();
    let mut keys = HashSet::new();
    for attribute in &scan.attributes {
        // The attribute's container: its body's path plus the value-map chain
        // it is nested in (`clients = { acme = { … } }`).
        let mut container = body_path(&scan, attribute.body);
        container.extend(attribute.prefix.iter().cloned());
        if container == wanted {
            keys.insert(attribute.name.clone());
        }
    }
    for header in &scan.headers {
        let parent = body_path(&scan, header.parent);
        if parent == wanted {
            // A block type present in this body shadows an attribute of that name.
            keys.insert(header.block_type.clone());
        }
        let mut base = parent;
        base.push(header.block_type.clone());
        // Labels already taken at the level the path points into.
        if let Some(rest) = wanted.strip_prefix(base.as_slice()) {
            let level = rest.len();
            if level < header.labels.len() && rest == &header.labels[..level] {
                keys.insert(header.labels[level].clone());
            }
        }
    }
    if keys.is_empty() {
        return None;
    }
    // The key being typed on the active line stays completable.
    let line_start = source[..cursor_byte.min(source.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |index| line_start + index);
    if let Some(current) = current_line_key(&source[line_start..line_end]) {
        keys.remove(&current);
    }
    Some(keys)
}

/// The key or label being typed on a single line: the assignment key, the
/// block type word, or a quoted label under the caret region.
fn current_line_key(line: &str) -> Option<String> {
    let code = code_before_comment(line)?;
    if let Some(equals) = assignment_equals(code) {
        let raw = code[..equals].trim();
        return is_ident(raw).then(|| raw.to_string());
    }
    let mut tokens = tokenize(code)
        .into_iter()
        .filter(|token| matches!(token.kind, Kind::Ident | Kind::Str { .. }));
    let first = token_name(code, &tokens.next()?)?;
    // A header line contributes its type word only when no label follows it on
    // this line; labels are completed at their own level.
    if tokens.next().is_none() {
        return Some(first);
    }
    None
}

/// Scalars render `key = value`; objects expand to a block over their
/// required children.
fn property_insert(
    key: &str,
    property: &Value,
    root: &Value,
    base_indent: &str,
    unit: &str,
) -> Option<(String, usize)> {
    if key.is_empty() || !is_ident(key) {
        return None;
    }
    let mut insert = format!("{key} = ");
    if let Some(value) = schema_const(property, root) {
        insert.push_str(&hcl_text(value));
        let caret = utf16(&insert);
        return Some((insert, caret));
    }
    if array_branch(property, root).is_some() {
        match schema_default(property, root)
            .filter(|value| !value.as_array().is_some_and(Vec::is_empty))
        {
            Some(default) => insert.push_str(&hcl_text(default)),
            None => {
                insert.push_str("[]");
                let caret = utf16(&insert) - 1;
                return Some((insert, caret));
            }
        }
        let caret = utf16(&insert);
        return Some((insert, caret));
    }
    if object_branch(property, root).is_some() {
        if let Some(default) = schema_default(property, root)
            .filter(|value| !value.as_object().is_some_and(serde_json::Map::is_empty))
        {
            insert.push_str(&hcl_text(default));
            let caret = utf16(&insert);
            return Some((insert, caret));
        }
        let child_indent = format!("{base_indent}{unit}");
        insert.push_str("{\n");
        let mut caret = None;
        if let Some(branch) = object_branch(property, root)
            && let Some(required) = required_children(branch)
        {
            for (name, child) in required {
                insert.push_str(&child_indent);
                insert.push_str(&name);
                insert.push_str(" = ");
                // The first child takes the caret; the others get a valid
                // placeholder so the document stays parseable.
                let takes_caret = caret.is_none();
                if takes_caret {
                    caret = Some(utf16(&insert));
                }
                match schema_default(child, root) {
                    Some(default) => insert.push_str(&hcl_text(default)),
                    None if !takes_caret => insert.push_str(&hcl_text(&placeholder(child, root))),
                    // The caret's own child: a string starts quoted, caret inside.
                    None if placeholder(child, root).is_string() => {
                        insert.push_str("\"\"");
                        caret = caret.map(|position| position + 1);
                    }
                    None => {}
                }
                insert.push('\n');
            }
        }
        if caret.is_none() {
            // No required children: the caret waits on an indented line inside.
            insert.push_str(&child_indent);
            caret = Some(utf16(&insert));
            insert.push('\n');
        }
        insert.push_str(base_indent);
        insert.push('}');
        return Some((insert, caret.unwrap_or_default()));
    }
    if let Some(default) = schema_default(property, root) {
        insert.push_str(&hcl_text(default));
    } else if placeholder(property, root).is_string() {
        // A string value starts quoted, with the caret inside.
        insert.push_str("\"\"");
        let caret = utf16(&insert) - 1;
        return Some((insert, caret));
    }
    let caret = utf16(&insert);
    Some((insert, caret))
}

/// The required properties of an object schema that have their own schema, in
/// the order the schema lists them.
fn required_children(branch: &Value) -> Option<Vec<(String, &Value)>> {
    let properties = branch.get("properties").and_then(Value::as_object)?;
    let required = branch.get("required").and_then(Value::as_array)?;
    let children: Vec<(String, &Value)> = required
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|name| properties.get(name).map(|child| (name.to_string(), child)))
        .collect();
    (!children.is_empty()).then_some(children)
}

/// Renders a schema value as HCL text. Strings always quote: HCL has no bare
/// string scalars, and a bare word would parse back as a reference.
fn hcl_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => serde_json::to_string(text).unwrap_or_else(|_| text.clone()),
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(hcl_text).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(map) => {
            let pairs: Vec<String> = map
                .iter()
                .filter(|(key, _)| is_ident(key))
                .map(|(key, child)| format!("{key} = {}", hcl_text(child)))
                .collect();
            format!("{{ {} }}", pairs.join(", "))
        }
    }
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

/// The mapping key under `column` (a character index) on a single line: the
/// attribute name before `=`. Block header words yield `None`: hover context
/// reads values, and headers open no value. Comment-only lines yield `None`.
fn key_at_column(line: &str, column: usize) -> Option<(String, usize)> {
    let code = code_before_comment(line)?;
    let byte = byte_at_column(line, column);
    if let Some(key) = inline_key_at(code, b'=', byte) {
        let raw = line[key.start..key.end].trim_matches('"');
        return (!raw.is_empty()).then(|| (raw.to_string(), line[..key.start].chars().count()));
    }
    // A block header (`letsencrypt {`, `services "web" {`): its type word or
    // a label.
    let header = code.trim_end().strip_suffix('{')?;
    let mut start = 0;
    for word in header.split_whitespace() {
        let at = start + header[start..].find(word)?;
        start = at + word.len();
        if (at..start).contains(&byte) {
            let name = word.trim_matches('"');
            return (!name.is_empty() && !name.contains('='))
                .then(|| (name.to_string(), line[..at + usize::from(word.starts_with('"'))].chars().count()));
        }
    }
    None
}

/// The byte offset of character `column` (0-based) in `line`.
fn byte_at_column(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index)
}

// ---------------------------------------------------------------------------
// Editing and highlighting
// ---------------------------------------------------------------------------

/// Enter inside a block indents one level deeper; otherwise it keeps the
/// current indentation.
fn newline_with_indent(source: &str, cursor: usize) -> String {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let indent: String = source[line_start..byte]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let before = source[..byte].trim_end_matches([' ', '\t']);
    let code = code_before_comment(before).unwrap_or(before);
    if code.trim_end().ends_with('{') {
        format!("\n{indent}{}", indent_unit(source))
    } else {
        format!("\n{indent}")
    }
}

/// Enter between a matching pair (`{}`) opens a block: the caret lands on an
/// indented middle line and the closing brace stays on its own line below.
fn newline_insertion(source: &str, cursor: usize) -> (String, usize) {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let indent: String = source[line_start..byte]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let before = source[..byte].trim_end_matches([' ', '\t']);
    let after = source[byte..].trim_start_matches([' ', '\t']);
    if before.ends_with('{') && after.starts_with('}') {
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

/// Comment toggling over whole lines with `#`, mirroring the TOML toggler's
/// selection mapping: blank lines are skipped, commented selections uncomment.
fn toggle_hcl_line_comments(
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

    let selected_lines = &lines[first_line..=last_line.min(lines.len() - 1)];
    let uncomment = selected_lines
        .iter()
        .filter_map(|line| {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
            let content = &body[indent..];
            (!content.is_empty()).then_some(content.starts_with('#'))
        })
        .all(|commented| commented);
    let mut edits: Vec<(usize, usize, usize)> = Vec::new();
    let mut offset = 0;
    let mut updated = String::with_capacity(source.len() + lines.len() * 2);

    for (line_index, line) in lines.iter().enumerate() {
        if (first_line..=last_line).contains(&line_index) {
            let body = line.trim_end_matches('\n').trim_end_matches('\r');
            let indent = body.len() - body.trim_start_matches([' ', '\t']).len();
            let content = &body[indent..];
            if !content.is_empty() {
                if uncomment && content.starts_with('#') {
                    let removed = 1 + usize::from(content.starts_with("# "));
                    edits.push((offset + indent, removed, 0));
                    updated.push_str(&line[..indent]);
                    updated.push_str(&content[removed..]);
                    updated.push_str(&line[body.len()..]);
                } else if !uncomment {
                    edits.push((offset + indent, 0, 2));
                    updated.push_str(&line[..indent]);
                    updated.push_str("# ");
                    updated.push_str(&line[indent..]);
                } else {
                    updated.push_str(line);
                }
            } else {
                updated.push_str(line);
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

/// Start of a `#` or `//` comment in a line fragment, ignoring markers inside
/// quoted strings. A marker starts a comment at the fragment start or after
/// whitespace.
fn line_comment_start(line: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut index = 0;
    let bytes = line.as_bytes();
    while index < line.len() {
        let character = line[index..].chars().next().unwrap_or('\0');
        if let Some(active) = quote {
            if active == '"' && escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
        } else if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if (character == '#' || (character == '/' && bytes.get(index + 1) == Some(&b'/')))
            && (index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return Some(index);
        }
        index += character.len_utf8();
    }
    None
}

/// Code before a trailing `#` / `//` comment, or `None` when the fragment is
/// comment-only. Block comments are handled by the tokenizer, not here.
fn code_before_comment(line: &str) -> Option<&str> {
    match line_comment_start(line) {
        Some(start) => {
            let code = &line[..start];
            (!code.trim().is_empty()).then_some(code)
        }
        None => (!line.trim().is_empty()).then_some(line),
    }
}

/// The unclosed string quote at the caret, or comment state: the pairing
/// guard. Heredoc bodies count as quoted so pairs never expand inside them.
fn hcl_lexical_state(source: &str, cursor: usize) -> (Option<char>, bool) {
    let byte = utf16_to_byte_index(source, cursor);
    // Inside a heredoc body, suppress pairing without reporting a comment.
    for token in tokenize(&source[..byte]) {
        if matches!(token.kind, Kind::Heredoc) && token.start < byte && byte < token.end {
            return (Some('"'), false);
        }
    }
    // A `/*` opened before the caret without a closer keeps the caret in a
    // comment. Strings cannot span lines, so per-line quote tracking plus a
    // heredoc skip is enough.
    let mut in_block_comment = false;
    let mut skip_to = 0;
    for token in tokenize(&source[..byte]) {
        if token.start < skip_to {
            continue;
        }
        match token.kind {
            Kind::Heredoc => skip_to = token.end,
            Kind::Comment
                if source[token.start..token.end].starts_with("/*")
                    && !source[token.start..token.end].contains("*/") =>
            {
                in_block_comment = !in_block_comment;
            }
            _ => {}
        }
    }
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..byte];
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if in_block_comment {
            if line[index..].starts_with("*/") {
                in_block_comment = false;
            }
            continue;
        }
        if let Some(active) = quote {
            if active == '"' && escaped {
                escaped = false;
            } else if active == '"' && character == '\\' {
                escaped = true;
            } else if character == active {
                quote = None;
            }
        } else if matches!(character, '"' | '\'') {
            quote = Some(character);
        } else if (character == '#' || line[index..].starts_with("//"))
            && (index == 0
                || line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return (None, true);
        } else if character == '/' && line[index..].starts_with("/*") {
            in_block_comment = true;
        }
    }
    (quote, in_block_comment)
}


fn highlight(line: &str) -> Vec<HighlightToken> {
    let indent = line.len() - line.trim_start().len();
    let content = &line[indent..];
    let mut out = Tokens::new();
    // A comment starting the content (or the whole line) is one token.
    if let Some(comment_at) = line_comment_start(content)
        && content[..comment_at].trim().is_empty()
    {
        out.push(indent, line.len(), TokenKind::Comment);
        return out.finish();
    }
    let tokens = tokenize(content);
    // A leading identifier that opens an attribute (`key =`) or a block
    // (`type {`, `type "label" {`) is the line's key.
    let key_end = match tokens.first() {
        Some(first) if matches!(first.kind, Kind::Ident) => {
            let mut look = 1;
            while tokens
                .get(look)
                .is_some_and(|token| token.kind == Kind::Comment)
            {
                look += 1;
            }
            match tokens.get(look).map(|token| token.kind) {
                Some(Kind::Punct('=')) | Some(Kind::Punct('{')) => Some(first.end),
                Some(Kind::Ident) | Some(Kind::Str { .. })
                    if classify_header(&tokens, 0).is_some() =>
                {
                    Some(first.end)
                }
                _ => None,
            }
        }
        _ => None,
    };
    for token in &tokens {
        out.push(
            indent + token.start,
            indent + token.end,
            token_kind(content, token, key_end),
        );
    }
    out.finish()
}

fn token_kind(line: &str, token: &Token, key_end: Option<usize>) -> TokenKind {
    match token.kind {
        Kind::Comment => TokenKind::Comment,
        Kind::Punct(_) => TokenKind::Punctuation,
        Kind::Str { .. } | Kind::Heredoc | Kind::Template => TokenKind::String,
        Kind::Ident => {
            if key_end.is_some_and(|end| token.end <= end) {
                TokenKind::Key
            } else {
                let text = &line[token.start..token.end];
                if matches!(text, "true" | "false" | "null")
                    || text
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_digit())
                {
                    TokenKind::Literal
                } else {
                    TokenKind::Value
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------


fn hcl_body(map: &serde_json::Map<String, Value>) -> Result<hcl::Body, String> {
    let mut body = hcl::Body::builder();
    for (key, value) in map {
        let identifier = hcl::Identifier::new(key.as_str()).map_err(|_| {
            format!("\"{key}\" is not an HCL identifier, so it cannot be a top-level key")
        })?;
        match value {
            Value::Object(children)
                if !children.is_empty() && children.keys().all(|child| is_ident(child)) =>
            {
                body = body.add_block(hcl::Block {
                    identifier,
                    labels: Vec::new(),
                    body: hcl_body(children)?,
                });
            }
            _ => body = body.add_attribute(hcl::Attribute::new(identifier, hcl_expression(value))),
        }
    }
    Ok(body.build())
}

/// A document value as an HCL expression; identifier keys stay unquoted.
fn hcl_expression(value: &Value) -> hcl::Expression {
    use hcl::expr::{Expression, Object, ObjectKey};
    match value {
        Value::Null => Expression::Null,
        Value::Bool(flag) => Expression::Bool(*flag),
        Value::Number(number) => hcl::to_expression(number).unwrap_or(Expression::Null),
        Value::String(text) => Expression::String(text.clone()),
        Value::Array(items) => Expression::Array(items.iter().map(hcl_expression).collect()),
        Value::Object(map) => Expression::Object(
            map.iter()
                .map(|(key, value)| {
                    let key = match hcl::Identifier::new(key.as_str()) {
                        Ok(identifier) if is_ident(key) => ObjectKey::Identifier(identifier),
                        _ => ObjectKey::Expression(Expression::String(key.clone())),
                    };
                    (key, hcl_expression(value))
                })
                .collect::<Object<ObjectKey, Expression>>(),
        ),
    }
}

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
    };
    use super::HclBehavior;
    use super::super::super::highlight::{TokenKind, runs};

    fn schema() -> SchemaBundle {
        build_schema(
            r#"{"type":"object","properties":{
                "region":{"type":"string","description":"AWS region","default":"us-east-1"},
                "count":{"type":"integer"},
                "enabled":{"type":"boolean"},
                "tags":{"type":"object","properties":{"Name":{"type":"string"}},"required":["Name"]},
                "ingress":{"type":"array","items":{"type":"object"}},
                "variable":{"type":"object","additionalProperties":{"type":"object",
                    "properties":{"default":{"type":"string"},"description":{"type":"string"}},"required":["default"]}}
            },"additionalProperties":false}"#,
        )
        .unwrap()
    }

    fn parse_ok(source: &str) -> serde_json::Value {
        match HclBehavior.parse(source) {
            Ok(document) => document,
            Err(error) => panic!("{}", error.message),
        }
    }

    fn complete(source: &str, schema: &SchemaBundle) -> Vec<Completion> {
        completions(
            source,
            source.encode_utf16().count(),
            false,
            Some(schema),
            true,
            &HclBehavior,
        )
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
    fn a_nested_object_accepted_inside_an_inline_map_is_formatted_as_a_block() {
        let schema = nested_schema();
        assert_eq!(
            accept_marked("web = {le|}\n", "letsencrypt", &schema, &HclBehavior),
            "web = {\n  letsencrypt = {\n    email = \"|\"\n  }\n}\n"
        );
    }

    #[test]
    fn a_nested_block_before_a_following_map_entry_keeps_it_on_its_own_line() {
        let schema = nested_schema();
        assert_eq!(
            accept_marked(
                "web = { le|, http = 80 }\n",
                "letsencrypt",
                &schema,
                &HclBehavior
            ),
            "web = {\n  letsencrypt = {\n    email = \"|\"\n  },\n  http = 80 }\n"
        );
    }

    #[test]
    fn a_scalar_accepted_inside_an_inline_map_stays_inline() {
        let schema = nested_schema();
        assert_eq!(
            accept_marked("web = { ht| }\n", "http", &schema, &HclBehavior),
            "web = { http = | }\n"
        );
    }

    #[test]
    fn a_nested_object_inside_a_block_body_is_indented_from_its_line() {
        let schema = nested_schema();
        assert_eq!(
            accept_marked("web {\n  le|\n}\n", "letsencrypt", &schema, &HclBehavior),
            "web {\n  letsencrypt = {\n    email = \"|\"\n  }\n}\n"
        );
    }

    #[test]
    fn duplicate_attributes_are_a_parse_error_on_the_repeated_line() {
        let error = HclBehavior
            .parse("region = \"a\"\nregion = \"b\"")
            .unwrap_err();
        assert_eq!(error.line, Some(2));
        assert!(error.message.contains("unique"), "{}", error.message);
    }

    #[test]
    fn repeated_blocks_merge_instead_of_erroring() {
        let document = parse_ok("ingress {\n  port = 1\n}\ningress {\n  port = 2\n}");
        assert_eq!(
            document["ingress"],
            serde_json::json!([{"port": 1}, {"port": 2}])
        );
    }

    #[test]
    fn same_key_in_different_blocks_is_not_a_duplicate() {
        let source =
            "variable \"a\" {\n  default = \"x\"\n}\nvariable \"b\" {\n  default = \"y\"\n}";
        assert!(HclBehavior.parse(source).is_ok());
    }

    #[test]
    fn syntax_errors_report_a_position() {
        let error = HclBehavior.parse("region = ").unwrap_err();
        assert_eq!(error.line, Some(1));
        assert!(error.column.is_some());
    }

    #[test]
    fn syntax_errors_report_the_parser_line_and_column() {
        // The line comes from hcl-rs's structured error, not its message.
        let error = HclBehavior.parse("region = \"a\"\ncount = (").unwrap_err();
        assert_eq!(error.line, Some(2));
        assert!(error.column.is_some());
    }

    #[test]
    fn references_and_interpolations_parse_as_text() {
        let document = parse_ok("a = var.foo\nb = \"hi ${var.name}\"\n");
        assert_eq!(document["a"], serde_json::json!("${var.foo}"));
        assert_eq!(document["b"], serde_json::json!("hi ${var.name}"));
    }

    #[test]
    fn keys_are_suggested_and_a_partial_key_is_replaced() {
        let source = "reg";
        let options = complete(source, &schema());
        let option = options
            .iter()
            .find(|option| option.label == "region")
            .expect("region should be suggested");
        let (updated, _, _) = apply_completion(source, option);
        assert_eq!(updated, "region = \"us-east-1\"");
    }

    #[test]
    fn keys_already_in_the_body_are_not_suggested_again() {
        let source = "region = \"a\"\nreg";
        assert!(
            complete(source, &schema())
                .iter()
                .all(|option| option.label != "region")
        );
    }

    #[test]
    fn keys_inside_a_block_are_suggested_from_the_schema() {
        let source = "variable \"name\" {\ndef";
        let option = complete(source, &schema())
            .into_iter()
            .find(|option| option.label == "default")
            .expect("default should be suggested inside the block");
        let (updated, _, _) = apply_completion(source, &option);
        assert!(updated.ends_with("default = \"\""), "{updated}");
    }

    #[test]
    fn keys_inside_a_nested_value_map_are_suggested_from_the_schema() {
        // `clients = { acme = { … } }` descends through both maps: the caret is
        // completing `ClientEntry` keys, not top-level attributes.
        let source = "variable = {\n  acme = {\n    de";
        let option = complete(source, &schema())
            .into_iter()
            .find(|option| option.label == "default")
            .expect("default should be suggested inside the nested map");
        let (updated, _, _) = apply_completion(source, &option);
        assert!(updated.ends_with("default = \"\""), "{updated}");
    }

    #[test]
    fn an_empty_nested_value_map_suggests_its_keys() {
        let source = "variable = {\n  acme = {";
        assert!(
            complete(source, &schema())
                .iter()
                .any(|option| option.label == "default"),
            "a key should be suggested right after `{{`"
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_error_inside_a_nested_value_map_is_reported_on_its_own_line() {
        let source = "variable = {\n  acme = {\n    default = 1\n  }\n}";
        let diagnostics = validate(source, Some(&schema()), None, true, &HclBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(3));
        assert_eq!(diagnostics[0].column, Some(15));
    }

    #[test]
    fn boolean_values_are_suggested_after_equals() {
        let source = "enabled = t";
        let option = complete(source, &schema())
            .into_iter()
            .find(|option| option.label == "true")
            .expect("true should be suggested");
        let (updated, _, _) = apply_completion(source, &option);
        assert_eq!(updated, "enabled = true");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn a_wrong_value_is_reported_on_the_value() {
        let source = "count = \"x\"";
        let diagnostics = validate(source, Some(&schema()), None, true, &HclBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(1));
        assert_eq!(diagnostics[0].column, Some(9));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_unknown_key_is_reported_on_its_key() {
        let source = "bogus = 1";
        let diagnostics = validate(source, Some(&schema()), None, true, &HclBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(1));
        assert_eq!(diagnostics[0].column, Some(1));
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn an_error_inside_a_block_is_reported_on_its_own_line() {
        let source = "variable \"name\" {\n  default = 1\n}";
        let diagnostics = validate(source, Some(&schema()), None, true, &HclBehavior);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, Some(2));
    }

    #[test]
    fn hovering_a_key_returns_its_schema_documentation() {
        let source = "region = \"a\"";
        let info = key_hover(source, 0, 1, Some(&schema()), &HclBehavior)
            .expect("region should have docs");
        assert_eq!(info.key, "region");
        assert_eq!(info.description.as_deref(), Some("AWS region"));
    }

    #[test]
    fn completing_a_key_before_its_existing_equals_swallows_the_equals() {
        let source = "reg = \"a\"";
        let cursor = "reg".encode_utf16().count();
        let option = completions(source, cursor, false, Some(&schema()), true, &HclBehavior)
            .into_iter()
            .find(|option| option.label == "region")
            .expect("region should be suggested");
        let (updated, _, _) = apply_completion(source, &option);
        assert_eq!(updated, "region = \"us-east-1\"");
    }

    /// Every caret position in messy and malformed documents must produce
    /// completions, hovers, and edits without panicking or slicing mid-character.
    #[cfg(feature = "schema-validation")]
    #[test]
    fn every_caret_position_in_messy_documents_is_handled_without_panicking() {
        let schema = schema();
        let samples = [
            "",
            "{",
            "}",
            "[[ ]]",
            "region = ",
            "variable \"na",
            "variable \"name\" {\n  def",
            "a = \"${unclosed}",
            "/* unclosed comment",
            "a = <<EOT\nhello",
            "resource \"aws_x\" \"y\" {\n  count = 2",
            "région = \"wörld\" co",
            "# comment\nregion = \"a\" # trailing",
            "a = [1, {\"b\" = ",
            "a = 1 b = 2",
        ];
        for source in samples {
            let caret_bytes = source
                .char_indices()
                .map(|(index, _)| index)
                .chain(std::iter::once(source.len()));
            for byte in caret_bytes {
                let cursor = source[..byte].encode_utf16().count();
                let _ = HclBehavior.completion_context(source, cursor);
                let _ = HclBehavior.newline(source, cursor);
                let _ = HclBehavior.lexical_state(source, cursor);
                for force in [false, true] {
                    for option in
                        completions(source, cursor, force, Some(&schema), true, &HclBehavior)
                    {
                        let _ = apply_completion(source, &option);
                    }
                }
            }
            let _ = HclBehavior.parse(source);
            let _ = validate(source, Some(&schema), None, true, &HclBehavior);
            for (line_index, line) in source.split('\n').enumerate() {
                for column in 0..12 {
                    let _ = HclBehavior.hover_cursor(line, column);
                    let _ = key_hover(source, line_index, column, Some(&schema), &HclBehavior);
                }
            }
        }
    }

    #[test]
    fn enter_inside_a_block_indents_one_level() {
        assert_eq!(
            HclBehavior.newline("variable \"a\" {", 15).0,
            "\n  "
        );
        assert_eq!(HclBehavior.newline("region = 1", 10).0, "\n");
    }

    #[test]
    fn enter_between_braces_puts_the_caret_on_an_indented_middle_line() {
        let (text, caret) = HclBehavior.newline("{}", 1);
        assert_eq!(text, "\n  \n");
        assert_eq!(caret, 3);
    }

    #[test]
    fn hash_comments_toggle_over_the_selection() {
        let (updated, _, _) = HclBehavior.toggle_comments("region = 1\n", 0, 10).unwrap();
        assert_eq!(updated, "# region = 1\n");
        let (restored, _, _) = HclBehavior
            .toggle_comments(&updated, 0, updated.encode_utf16().count())
            .unwrap();
        assert_eq!(restored, "region = 1\n");
    }

    #[test]
    fn an_open_string_is_reported_to_the_pairing_guard() {
        assert_eq!(HclBehavior.lexical_state("region = \"x", 11).quote, Some('"'));
        assert_eq!(HclBehavior.lexical_state("region = \"x\"", 12).quote, None);
        assert!(HclBehavior.lexical_state("# region", 4).in_comment);
        assert!(HclBehavior.lexical_state("/* region", 9).in_comment);
    }


    #[test]
    fn an_object_property_expands_to_a_block_with_the_caret_on_the_first_value() {
        let schema = schema();
        let tags = &schema.schema["properties"]["tags"];
        let (insert, caret) = HclBehavior
            .property_insert("tags", tags, &schema.schema, &INSERT_AT_ROOT)
            .unwrap();
        assert_eq!(insert, "tags = {\n  Name = \"\"\n}");
        // Inside the string's quotes.
        assert_eq!(caret, "tags = {\n  Name = \"".encode_utf16().count());
    }

    fn highlighted(line: &str) -> Vec<(Option<TokenKind>, String)> {
        runs(line, &super::highlight(line))
    }

    fn run(kind: Option<TokenKind>, text: &str) -> (Option<TokenKind>, String) {
        (kind, text.to_string())
    }

    #[test]
    fn block_headers_color_the_type_as_key_and_labels_as_strings() {
        use TokenKind::*;
        assert_eq!(
            highlighted(r#"resource "aws_x" "y" {"#),
            vec![
                run(Some(Key), "resource"),
                run(None, " "),
                run(Some(String), r#""aws_x""#),
                run(None, " "),
                run(Some(String), r#""y""#),
                run(None, " "),
                run(Some(Punctuation), "{"),
            ]
        );
        assert_eq!(
            highlighted("  ingress {"),
            vec![
                run(None, "  "),
                run(Some(Key), "ingress"),
                run(None, " "),
                run(Some(Punctuation), "{"),
            ]
        );
        assert_eq!(
            highlighted("module web {"),
            vec![
                run(Some(Key), "module"),
                run(None, " "),
                run(Some(Value), "web"),
                run(None, " "),
                run(Some(Punctuation), "{"),
            ]
        );
    }

    #[test]
    fn attributes_color_key_equals_and_value() {
        use TokenKind::*;
        assert_eq!(
            highlighted("  name = var.name"),
            vec![
                run(None, "  "),
                run(Some(Key), "name"),
                run(None, " "),
                run(Some(Punctuation), "="),
                run(None, " "),
                run(Some(Value), "var"),
                run(Some(Punctuation), "."),
                run(Some(Value), "name"),
            ]
        );
    }

    #[test]
    fn a_leading_word_without_equals_or_brace_is_a_value() {
        use TokenKind::*;
        assert_eq!(
            highlighted("name"),
            vec![run(Some(Value), "name")]
        );
    }

    #[test]
    fn strings_heredocs_and_templates_color_as_strings() {
        use TokenKind::*;
        assert_eq!(
            highlighted(r#"a = "x\"y""#),
            vec![
                run(Some(Key), "a"),
                run(None, " "),
                run(Some(Punctuation), "="),
                run(None, " "),
                run(Some(String), r#""x\"y""#),
            ]
        );
        assert_eq!(
            highlighted("a = <<EOT"),
            vec![
                run(Some(Key), "a"),
                run(None, " "),
                run(Some(Punctuation), "="),
                run(None, " "),
                run(Some(String), "<<EOT"),
            ]
        );
        assert_eq!(
            highlighted("a = ${var.x}"),
            vec![
                run(Some(Key), "a"),
                run(None, " "),
                run(Some(Punctuation), "="),
                run(None, " "),
                run(Some(String), "${var.x}"),
            ]
        );
    }

    #[test]
    fn numbers_booleans_and_null_are_literals() {
        use TokenKind::*;
        assert_eq!(
            highlighted("x = [1.5, true, null, false]"),
            vec![
                run(Some(Key), "x"),
                run(None, " "),
                run(Some(Punctuation), "="),
                run(None, " "),
                run(Some(Punctuation), "["),
                run(Some(Literal), "1.5"),
                run(Some(Punctuation), ","),
                run(None, " "),
                run(Some(Literal), "true"),
                run(Some(Punctuation), ","),
                run(None, " "),
                run(Some(Literal), "null"),
                run(Some(Punctuation), ","),
                run(None, " "),
                run(Some(Literal), "false"),
                run(Some(Punctuation), "]"),
            ]
        );
    }

    #[test]
    fn braces_and_brackets_are_punctuation() {
        use TokenKind::*;
        assert_eq!(
            highlighted("  }]"),
            vec![
                run(None, "  "),
                run(Some(Punctuation), "}"),
                run(Some(Punctuation), "]"),
            ]
        );
    }

    #[test]
    fn a_comment_only_line_is_one_comment_after_the_indent() {
        use TokenKind::*;
        for comment in ["# note \"x\" { = }", "// note"] {
            let line = format!("  {comment}");
            assert_eq!(
                highlighted(&line),
                vec![run(None, "  "), run(Some(Comment), comment)]
            );
        }
    }

    #[test]
    fn trailing_and_block_comments_are_comments() {
        use TokenKind::*;
        assert_eq!(
            highlighted("a = 1 # one"),
            vec![
                run(Some(Key), "a"),
                run(None, " "),
                run(Some(Punctuation), "="),
                run(None, " "),
                run(Some(Literal), "1"),
                run(None, " "),
                run(Some(Comment), "# one"),
            ]
        );
        assert_eq!(
            highlighted("a /* k */ = 1 // one"),
            vec![
                run(Some(Key), "a"),
                run(None, " "),
                run(Some(Comment), "/* k */"),
                run(None, " "),
                run(Some(Punctuation), "="),
                run(None, " "),
                run(Some(Literal), "1"),
                run(None, " "),
                run(Some(Comment), "// one"),
            ]
        );
        assert_eq!(
            highlighted("/* open"),
            vec![run(Some(Comment), "/* open")]
        );
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        use TokenKind::*;
        assert_eq!(
            highlighted(r#"a = "x # y""#),
            vec![
                run(Some(Key), "a"),
                run(None, " "),
                run(Some(Punctuation), "="),
                run(None, " "),
                run(Some(String), r#""x # y""#),
            ]
        );
    }

    #[test]
    fn tricky_lines_cover_every_character() {
        for line in [
            "",
            "   ",
            "# only a comment",
            "  // only a comment",
            r#"name = "héllo ✓""#,
            r#"name = "unterminated"#,
            r#"a = "x # y" # z"#,
            "a = 1   ",
            "ключ = \"значение\" # ✓",
            "a = ${ \"}\" ",
            "<< = é",
        ] {
            let text: std::string::String =
                highlighted(line).into_iter().map(|(_, text)| text).collect();
            assert_eq!(text, line);
        }
    }
}
