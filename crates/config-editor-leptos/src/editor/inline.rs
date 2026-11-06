//! Inline containers shared by the brace languages: inline tables and arrays
//! (TOML), flow mappings and sequences (YAML), value maps and lists (HCL). A
//! language passes its key separator (`=` or `:`); quoting, comments, nesting,
//! and containers spanning lines are handled here once. Functions taking
//! `code` read one statement: a line, or from [`statement_start`] to the caret.

use super::language::{CompletionContext, SchemaPathSegment};

/// A `key <separator>` on one line: the key's byte range and the byte just past
/// the separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlineKey {
    /// Byte offset where the key starts.
    pub start: usize,
    /// Byte offset just past the key.
    pub end: usize,
    /// Byte offset just past the key's separator.
    pub after_separator: usize,
}

/// What the caret is on inside an inline container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineEntry {
    /// Typing a key that starts at byte `start`.
    Key {
        /// Byte offset where the key starts.
        start: usize,
    },
    /// Typing the value of `key`, which starts at byte `start`.
    Value {
        /// The key whose value is typed.
        key: String,
        /// Byte offset where the value starts.
        start: usize,
    },
    /// Typing item `index` of an array; it starts at byte `start`.
    Item {
        /// The item's index in its array.
        index: usize,
        /// Byte offset where the item starts.
        start: usize,
    },
}

/// The caret inside one or more inline containers opened on its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineCaret {
    /// Path from the line's own key down to the container holding the caret.
    pub path: Vec<SchemaPathSegment>,
    /// What the caret is on.
    pub entry: InlineEntry,
    /// After a complete value and a space with no comma yet
    /// (`{ a = 1 | }`), the caret starts a new key that needs a separator at
    /// this byte offset (just after the value).
    pub separator_at: Option<usize>,
}

/// Every key on a line of code: the line's own key and each key inside its
/// inline containers.
pub fn inline_keys(code: &str, separator: u8) -> Vec<InlineKey> {
    let mut keys = Vec::new();
    walk(code, separator, |event| {
        if let Event::Key(key) = event {
            keys.push(key);
        }
    });
    keys
}

/// The key under byte `at`, if any.
pub fn inline_key_at(code: &str, separator: u8, at: usize) -> Option<InlineKey> {
    inline_keys(code, separator)
        .into_iter()
        .find(|key| (key.start..key.end).contains(&at))
}

/// Where the caret at the end of `code_before` sits, when it is inside an
/// inline container opened on the line; `None` at the line's top level.
pub fn inline_caret(code_before: &str, separator: u8) -> Option<InlineCaret> {
    let mut frames: Vec<Frame> = Vec::new();
    let mut entry_key: Option<String> = None;
    let mut entry_start = 0;
    let mut value_start = None;
    walk(code_before, separator, |event| match event {
        Event::Key(key) => {
            entry_key = Some(unquote(&code_before[key.start..key.end]));
            value_start = Some(skip_blanks(code_before, key.after_separator));
        }
        Event::Open(bracket, at) => {
            let segment = match frames.last() {
                Some(Frame::Array { index, .. }) => Some(SchemaPathSegment::ArrayItem(*index)),
                _ => entry_key.take().map(SchemaPathSegment::Key),
            };
            frames.push(if bracket == b'{' {
                Frame::Table { segment }
            } else {
                Frame::Array { segment, index: 0 }
            });
            entry_key = None;
            value_start = None;
            entry_start = at + 1;
        }
        Event::Close(at) => {
            frames.pop();
            entry_key = None;
            // The closed container was the entry's value, so a line break
            // after it ends the entry.
            value_start = Some(at);
        }
        Event::Comma(at) => {
            if let Some(Frame::Array { index, .. }) = frames.last_mut() {
                *index += 1;
            }
            entry_key = None;
            value_start = None;
            entry_start = at + 1;
        }
        Event::Newline(at) => {
            let value_begun =
                value_start.is_some_and(|start| !code_before[start.min(at)..at].trim().is_empty());
            if matches!(frames.last(), Some(Frame::Table { .. })) && value_begun {
                entry_key = None;
                value_start = None;
                entry_start = at + 1;
            }
        }
    });
    if frames.is_empty() {
        return None;
    }
    let path = frames.iter().filter_map(Frame::segment).cloned().collect();
    // A complete value followed by whitespace in an object: a new entry.
    if let (Some(Frame::Table { .. }), Some(_), Some(start)) =
        (frames.last(), &entry_key, value_start)
    {
        let value = code_before[start.min(code_before.len())..].trim_end();
        let complete = !value.is_empty()
            && code_before.ends_with([' ', '\t'])
            && !value.ends_with(['=', ':', ','])
            && match value.chars().next() {
                Some(quote @ ('"' | '\'')) => value.len() > 1 && value.ends_with(quote),
                _ => !value.contains(char::is_whitespace),
            };
        if complete {
            return Some(InlineCaret {
                path,
                entry: InlineEntry::Key {
                    start: code_before.len(),
                },
                separator_at: Some(start + value.len()),
            });
        }
    }
    let entry = match (frames.last(), entry_key, value_start) {
        (_, Some(key), Some(start)) => InlineEntry::Value { key, start },
        (Some(Frame::Array { index, .. }), _, _) => InlineEntry::Item {
            index: *index,
            start: skip_blanks(code_before, entry_start),
        },
        _ => InlineEntry::Key {
            start: skip_blanks(code_before, entry_start),
        },
    };
    Some(InlineCaret {
        path,
        entry,
        separator_at: None,
    })
}

/// The keys of the inline container holding byte `at` of a line of code,
/// before and after it; `None` when `at` is at the line's top level. Completion
/// uses it so a key already in the container is not offered again.
pub fn inline_sibling_keys(code: &str, separator: u8, at: usize) -> Option<Vec<String>> {
    let mut stack: Vec<usize> = Vec::new();
    let mut next_id = 1;
    let mut keys: Vec<(usize, String)> = Vec::new();
    let mut caret_container = None;
    let mut check_caret = |stack: &[usize], position: usize| {
        if caret_container.is_none() && position >= at {
            caret_container = Some(stack.last().copied().unwrap_or(0));
        }
    };
    walk(code, separator, |event| match event {
        // The key the caret is in is being typed or renamed: not a sibling.
        Event::Key(key) if key.start < at && at <= key.end => {}
        Event::Key(key) => {
            check_caret(&stack, key.start);
            keys.push((
                stack.last().copied().unwrap_or(0),
                unquote(&code[key.start..key.end]),
            ));
        }
        // A caret up to the bracket is outside; right after it, inside.
        Event::Open(_, position) => {
            check_caret(&stack, position);
            stack.push(next_id);
            next_id += 1;
            check_caret(&stack, position + 1);
        }
        Event::Close(position) => {
            check_caret(&stack, position);
            stack.pop();
        }
        Event::Comma(position) => check_caret(&stack, position),
        Event::Newline(_) => {}
    });
    let container = caret_container.unwrap_or_else(|| stack.last().copied().unwrap_or(0));
    (container != 0).then(|| {
        keys.into_iter()
            .filter(|(owner, _)| *owner == container)
            .map(|(_, key)| key)
            .collect()
    })
}

/// Whether the text after the caret begins another entry (`listen = 7835`
/// after `lim|`): accepting a key there must insert before that entry, on
/// its own line, rather than replace it.
pub fn entry_follows(rest: &str, separator: u8) -> bool {
    let rest = rest.trim_start_matches([' ', '\t']);
    inline_keys(rest, separator)
        .first()
        .is_some_and(|key| key.start == 0)
}

/// `context` for a key typed in an inline container right before more text
/// (`{k|nodelay: true}`, `rest` being the line after the caret). When an
/// entry follows, a suggestion completing its key renames it and any other
/// is inserted before it with a comma between; a bare word after the caret
/// is replaced.
pub fn in_front_of_inline_entry(
    mut context: CompletionContext,
    rest: &str,
    separator: u8,
) -> CompletionContext {
    let word: String = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
        .collect();
    if entry_follows(rest, separator) {
        context.entry_suffix = if word.is_empty() {
            ",".into()
        } else {
            ", ".into()
        };
        context.entry_after = Some(word);
    } else {
        context.replace_end += word.encode_utf16().count();
    }
    context
}

/// The byte just past the quoted string starting at `start`, or the end.
pub fn skip_quoted(code: &str, start: usize) -> usize {
    let quote = code.as_bytes()[start];
    let mut index = start + 1;
    while index < code.len() {
        match code.as_bytes()[index] {
            b'\\' if quote == b'"' => index += 2,
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    code.len()
}

/// The byte end of the value the caret at `cursor` is in, up to `line_end`:
/// inside an open string (`tls = "|", hsts = true`) the value ends at its
/// closing quote; otherwise as [`inline_value_len`] reads the rest.
pub fn caret_value_end(source: &str, cursor: usize, line_end: usize) -> usize {
    string_end_at_caret(source, cursor, line_end)
        .unwrap_or_else(|| cursor + inline_value_len(&source[cursor..line_end.max(cursor)]))
}

/// When the caret at `cursor` is inside a string opened earlier on its line,
/// the byte end of that string (after its closing quote, or the caret when it
/// has none before `line_end`).
pub fn string_end_at_caret(source: &str, cursor: usize, line_end: usize) -> Option<usize> {
    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let quote = open_quote(&source[line_start..cursor])?;
    let rest = &source[cursor..line_end.max(cursor)];
    Some(closing_quote(rest, quote).map_or(cursor, |index| cursor + index + 1))
}

/// The quote left open at the end of `text`, if any.
fn open_quote(text: &str) -> Option<char> {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for character in text.chars() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if character == '\\' => escaped = true,
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None => {}
        }
    }
    quote
}

/// The byte offset of the first unescaped `quote` in `text`.
fn closing_quote(text: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' && quote == '"' {
            escaped = true;
        } else if character == quote {
            return Some(index);
        }
    }
    None
}

/// The byte offset of the bracket closing the one `text` starts with, on the
/// same line, outside strings.
pub fn matching_close(text: &str, close: char) -> Option<usize> {
    let open = text.chars().next()?;
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut previous = ' ';
    for (index, character) in text.char_indices() {
        match quote {
            Some(open_quote) if character == open_quote && previous != '\\' => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == open => depth += 1,
            None if character == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            None => {}
        }
        previous = character;
    }
    None
}

/// Length of an inline value starting at the caret: through its closing quote
/// when quoted, else up to the next `,` or closer, without trailing blanks.
pub fn inline_value_len(text: &str) -> usize {
    if text.starts_with(['"', '\'']) {
        return skip_quoted(text, 0);
    }
    let end = text.find([',', '}', ']']).unwrap_or(text.len());
    text[..end].trim_end().len()
}

enum Frame {
    Table {
        segment: Option<SchemaPathSegment>,
    },
    Array {
        segment: Option<SchemaPathSegment>,
        index: usize,
    },
}

impl Frame {
    fn segment(&self) -> Option<&SchemaPathSegment> {
        match self {
            Frame::Table { segment } | Frame::Array { segment, .. } => segment.as_ref(),
        }
    }
}

enum Event {
    Key(InlineKey),
    Open(u8, usize),
    Close(usize),
    Comma(usize),
    /// A line break outside quotes; inside an object it ends an entry whose
    /// value has begun (HCL separates entries with newlines).
    Newline(usize),
}

/// The byte range in `code` of what `path` names inside the inline container
/// `code` starts with (`{…}` or `[…]`, spanning lines if needed): the value,
/// or with `key` the key. Keys are separated from values by `:` or `=`.
/// Diagnostics use it to point inside flow collections and inline tables.
pub fn inline_path_span(
    code: &str,
    path: &[SchemaPathSegment],
    key: bool,
) -> Option<(usize, usize)> {
    let start = code.len() - code.trim_start().len();
    path_span(code, start, path, key)
}

fn path_span(
    code: &str,
    at: usize,
    path: &[SchemaPathSegment],
    key: bool,
) -> Option<(usize, usize)> {
    let at = skip_space(code, at, true);
    let Some((segment, rest)) = path.split_first() else {
        return Some((at, value_end(code, at)));
    };
    let bytes = code.as_bytes();
    match (bytes.get(at)?, segment) {
        (b'[', SchemaPathSegment::ArrayItem(index)) => {
            let mut position = at + 1;
            for _ in 0..*index {
                position = skip_space(code, position, true);
                position = value_end(code, position);
                position = skip_space(code, position, true);
                if bytes.get(position) != Some(&b',') {
                    return None;
                }
                position += 1;
            }
            let position = skip_space(code, position, true);
            if matches!(bytes.get(position), Some(b']') | None) {
                return None;
            }
            path_span(code, position, rest, key)
        }
        (b'{', SchemaPathSegment::Key(wanted)) => {
            let mut position = at + 1;
            loop {
                position = skip_space(code, position, true);
                while bytes.get(position) == Some(&b',') {
                    position = skip_space(code, position + 1, true);
                }
                let key_start = position;
                let key_end = match bytes.get(position)? {
                    b'}' => return None,
                    b'"' | b'\'' => skip_quoted(code, position),
                    _ => {
                        position
                            + code[position..]
                                .find(|character: char| {
                                    character.is_whitespace()
                                        || matches!(character, ':' | '=' | ',' | '}')
                                })
                                .unwrap_or(code.len() - position)
                    }
                };
                let separator = skip_space(code, key_end, false);
                if !matches!(bytes.get(separator), Some(b':' | b'=')) {
                    return None;
                }
                let value = separator + 1;
                if unquote(&code[key_start..key_end]) == *wanted {
                    if rest.is_empty() && key {
                        return Some((key_start, key_end));
                    }
                    return path_span(code, value, rest, key);
                }
                position = value_end(code, skip_space(code, value, true));
            }
        }
        _ => None,
    }
}

fn skip_space(code: &str, mut at: usize, newlines: bool) -> usize {
    let bytes = code.as_bytes();
    while let Some(byte) = bytes.get(at) {
        match byte {
            b' ' | b'\t' | b'\r' => at += 1,
            b'\n' if newlines => at += 1,
            b'#' => at = line_end(code, at),
            _ => break,
        }
    }
    at
}

/// Where the value starting at `at` ends: before the `,`, closer, or line
/// break that follows it outside brackets and quotes.
fn value_end(code: &str, at: usize) -> usize {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut index = at;
    let mut end = at;
    while index < bytes.len() {
        match bytes[index] {
            b'"' | b'\'' => {
                index = skip_quoted(code, index);
                end = index;
                continue;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' if depth == 0 => break,
            b'}' | b']' => depth -= 1,
            b',' | b'\n' if depth == 0 => break,
            b' ' | b'\t' | b'\r' => {
                index += 1;
                continue;
            }
            _ => {}
        }
        index += 1;
        end = index;
    }
    end
}

/// Walks `code` once, reporting keys, brackets, and separating commas outside
/// quotes. A key is a bare word or quoted string at an entry start followed by
/// `separator`.
fn walk(code: &str, separator: u8, mut on: impl FnMut(Event)) {
    let bytes = code.as_bytes();
    let mut index = 0;
    let mut at_entry = true;
    while index < bytes.len() {
        match bytes[index] {
            b'{' | b'[' => {
                on(Event::Open(bytes[index], index));
                at_entry = true;
                index += 1;
            }
            b'}' | b']' => {
                on(Event::Close(index));
                at_entry = false;
                index += 1;
            }
            b',' => {
                on(Event::Comma(index));
                at_entry = true;
                index += 1;
            }
            b' ' | b'\t' | b'\r' => index += 1,
            // A line break also starts an entry: HCL objects separate
            // entries with newlines, not only commas.
            b'\n' => {
                on(Event::Newline(index));
                at_entry = true;
                index += 1;
            }
            b'#' if is_comment_start(code, index) => index = line_end(code, index),
            b'/' if code[index..].starts_with("//") && is_comment_start(code, index) => {
                index = line_end(code, index);
            }
            b'"' | b'\'' if !at_entry => index = skip_quoted(code, index),
            _ if at_entry => {
                at_entry = false;
                let start = index;
                let end = if matches!(bytes[index], b'"' | b'\'') {
                    skip_quoted(code, index)
                } else {
                    index
                        + code[index..]
                            .find(|character: char| {
                                !(character.is_alphanumeric()
                                    || matches!(character, '_' | '-' | '.' | '/' | '@'))
                            })
                            .unwrap_or(code.len() - index)
                };
                let after = skip_blanks(code, end);
                let separated = bytes.get(after) == Some(&separator)
                    && !(separator == b'=' && bytes.get(after + 1) == Some(&b'='));
                if end > start && separated {
                    on(Event::Key(InlineKey {
                        start,
                        end,
                        after_separator: after + 1,
                    }));
                    index = after + 1;
                } else {
                    index = end.max(index + 1);
                }
            }
            _ => index += 1,
        }
    }
}

/// Every key in `code` (one statement) with its path from the statement's
/// first key: `a = [{ b = 1 }]` yields `a` at `[a]` and `b` at `[a, 0, b]`.
/// Diagnostics use it to place a problem inside nested inline containers.
pub fn inline_key_paths(code: &str, separator: u8) -> Vec<(Vec<SchemaPathSegment>, InlineKey)> {
    let mut found = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut entry_key: Option<String> = None;
    walk(code, separator, |event| match event {
        Event::Key(key) => {
            let name = unquote(&code[key.start..key.end]);
            let mut path: Vec<SchemaPathSegment> =
                frames.iter().filter_map(Frame::segment).cloned().collect();
            if let Some(Frame::Array { index, .. }) = frames.last() {
                path.push(SchemaPathSegment::ArrayItem(*index));
            }
            path.push(SchemaPathSegment::Key(name.clone()));
            found.push((path, key));
            entry_key = Some(name);
        }
        Event::Open(bracket, _) => {
            let segment = match frames.last() {
                Some(Frame::Array { index, .. }) => Some(SchemaPathSegment::ArrayItem(*index)),
                _ => entry_key.take().map(SchemaPathSegment::Key),
            };
            frames.push(if bracket == b'{' {
                Frame::Table { segment }
            } else {
                Frame::Array { segment, index: 0 }
            });
        }
        Event::Close(_) => {
            frames.pop();
            entry_key = None;
        }
        Event::Comma(_) => {
            if let Some(Frame::Array { index, .. }) = frames.last_mut() {
                *index += 1;
            }
            entry_key = None;
        }
        Event::Newline(_) => {}
    });
    found
}

/// Byte ranges of the statements in `source` that open inline containers:
/// from the start of the line opening the outermost container to just past
/// its closer (or the end of `source`). Same opener rules as
/// [`statement_start`].
pub fn statements(source: &str) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut ranges = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut index = 0;
    let mut previous = b'\n';
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' | b'\'' if depth > 0 || matches!(previous, b':' | b'=' | b',' | b'[' | b'{') => {
                index = skip_quoted(source, index);
                previous = b'"';
                continue;
            }
            b'#' if is_comment_start(source, index) => {
                index = line_end(source, index);
                continue;
            }
            b'{' | b'[' if matches!(previous, b':' | b'=' | b',' | b'[' | b'{' | b'-' | b'\n') => {
                if depth == 0 {
                    start = source[..index].rfind('\n').map_or(0, |newline| newline + 1);
                }
                depth += 1;
            }
            b'}' | b']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    ranges.push((start, index + 1));
                }
            }
            _ => {}
        }
        if !matches!(byte, b' ' | b'\t') {
            previous = byte;
        }
        index += 1;
    }
    if depth > 0 {
        ranges.push((start, source.len()));
    }
    ranges
}

/// Where the statement holding the end of `before_caret` starts: the line
/// that opened its outermost still-open container, or `None` when the caret
/// is at the top level. Only an opener at a value position counts (after
/// `:`, `=`, `,`, `-`, another opener, or a line's indentation), so brackets
/// in prose (a YAML block scalar) do not open a container.
pub fn statement_start(before_caret: &str) -> Option<usize> {
    let bytes = before_caret.as_bytes();
    let mut depth = 0usize;
    let mut start = None;
    let mut index = 0;
    let mut previous = b'\n';
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'"' | b'\'' if depth > 0 || matches!(previous, b':' | b'=' | b',' | b'[' | b'{') => {
                index = skip_quoted(before_caret, index);
                previous = b'"';
                continue;
            }
            b'#' if is_comment_start(before_caret, index) => {
                index = line_end(before_caret, index);
                continue;
            }
            b'{' | b'[' if matches!(previous, b':' | b'=' | b',' | b'[' | b'{' | b'-' | b'\n') => {
                if depth == 0 {
                    start = Some(
                        before_caret[..index]
                            .rfind('\n')
                            .map_or(0, |newline| newline + 1),
                    );
                }
                depth += 1;
            }
            b'}' | b']' if depth > 0 => depth -= 1,
            _ => {}
        }
        if !matches!(byte, b' ' | b'\t') {
            previous = byte;
        }
        index += 1;
    }
    start.filter(|_| depth > 0)
}

fn is_comment_start(code: &str, index: usize) -> bool {
    index == 0 || code.as_bytes()[index - 1].is_ascii_whitespace()
}

fn line_end(code: &str, from: usize) -> usize {
    code[from..]
        .find('\n')
        .map_or(code.len(), |offset| from + offset)
}

/// Past the whitespace at `from`, line breaks included: an entry or value
/// in a multi-line container starts on its own line.
fn skip_blanks(code: &str, from: usize) -> usize {
    let from = from.min(code.len());
    from + (code[from..].len() - code[from..].trim_start().len())
}

fn unquote(key: &str) -> String {
    let trimmed = key.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = trimmed
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use SchemaPathSegment::{ArrayItem, Key};

    fn key(name: &str) -> SchemaPathSegment {
        Key(name.to_string())
    }

    #[test]
    fn keys_are_found_on_the_line_and_inside_its_containers() {
        let code = "acme = { secret = \"a, b = c\", max = 3 }";
        let names: Vec<&str> = inline_keys(code, b'=')
            .iter()
            .map(|found| &code[found.start..found.end])
            .collect();
        assert_eq!(names, ["acme", "secret", "max"]);
        let yaml = "acme: { secret: x, max: 3 }";
        let names: Vec<&str> = inline_keys(yaml, b':')
            .iter()
            .map(|found| &yaml[found.start..found.end])
            .collect();
        assert_eq!(names, ["acme", "secret", "max"]);
    }

    #[test]
    fn the_caret_resolves_keys_values_and_items_at_any_depth() {
        assert_eq!(
            inline_caret("acme = { se", b'='),
            Some(InlineCaret {
                path: vec![key("acme")],
                entry: InlineEntry::Key { start: 9 },
                separator_at: None,
            })
        );
        assert_eq!(
            inline_caret("clients = { acme = { max = 1, mode = f", b'='),
            Some(InlineCaret {
                path: vec![key("clients"), key("acme")],
                entry: InlineEntry::Value {
                    key: "mode".to_string(),
                    start: 37
                },
                separator_at: None,
            })
        );
        assert_eq!(
            inline_caret("a: { b: { c: 1 }, d", b':'),
            Some(InlineCaret {
                path: vec![key("a")],
                entry: InlineEntry::Key { start: 18 },
                separator_at: None,
            })
        );
        assert_eq!(
            inline_caret("hosts = [\"a\", \"b", b'='),
            Some(InlineCaret {
                path: vec![key("hosts")],
                entry: InlineEntry::Item {
                    index: 1,
                    start: 14
                },
                separator_at: None,
            })
        );
        assert_eq!(
            inline_caret("clients = [{ name = \"a\" }, { na", b'='),
            Some(InlineCaret {
                path: vec![key("clients"), ArrayItem(1)],
                entry: InlineEntry::Key { start: 29 },
                separator_at: None,
            })
        );
    }

    #[test]
    fn the_top_level_of_a_line_is_not_inline() {
        assert_eq!(inline_caret("acme = ", b'='), None);
        assert_eq!(inline_caret("acme = { a = 1 }", b'='), None);
    }

    #[test]
    fn separators_inside_quotes_and_comparisons_are_not_keys() {
        assert!(inline_keys("a == b", b'=').is_empty());
        let code = "x = { \"k:=\" = 1 }";
        let found = inline_keys(code, b'=');
        assert_eq!(&code[found[1].start..found[1].end], "\"k:=\"");
    }

    #[test]
    fn sibling_keys_are_those_of_the_caret_container() {
        let code = "a = { x = 1, y = { z = 2 }, w = 3 }";
        let caret = code.find("w").unwrap() - 2;
        assert_eq!(
            inline_sibling_keys(code, b'=', caret),
            Some(vec!["x".into(), "y".into(), "w".into()])
        );
        let inner = code.find("z").unwrap();
        assert_eq!(
            inline_sibling_keys(code, b'=', inner),
            Some(vec!["z".into()])
        );
        assert_eq!(inline_sibling_keys(code, b'=', 1), None);
    }

    #[test]
    fn statements_span_the_lines_of_an_open_container() {
        let source = "a = 1\ncerts = [\n  { hosts = [] },\n  ";
        assert_eq!(statement_start(source), Some(6));
        let statement = &source[6..];
        assert_eq!(
            inline_caret(statement, b'=').map(|caret| caret.path),
            Some(vec![key("certs")])
        );
        assert_eq!(statement_start("a = [1]\nb = 2\n"), None);
        // Prose brackets and comments do not open containers.
        assert_eq!(statement_start("note: |\n  see [docs\nb: 1\n"), None);
        assert_eq!(statement_start("a = 1 # [x\nb = "), None);
    }

    #[test]
    fn key_paths_include_array_indexes() {
        let code = "dns = [\n  { domains = [\"a\"] },\n  {\n    x = 1\n    wait_secs = 60\n  }\n]";
        let paths: Vec<(Vec<SchemaPathSegment>, &str)> = inline_key_paths(code, b'=')
            .into_iter()
            .map(|(path, key)| (path, &code[key.start..key.end]))
            .collect();
        assert_eq!(
            paths,
            vec![
                (vec![key("dns")], "dns"),
                (vec![key("dns"), ArrayItem(0), key("domains")], "domains"),
                (vec![key("dns"), ArrayItem(1), key("x")], "x"),
                (
                    vec![key("dns"), ArrayItem(1), key("wait_secs")],
                    "wait_secs"
                ),
            ]
        );
        assert_eq!(statements("a = 1\nb = [\n 1\n]\nc = 2\n"), vec![(6, 16)]);
    }

    #[test]
    fn an_entry_after_the_caret_is_detected() {
        assert!(entry_follows("listen = 7835", b'='));
        assert!(entry_follows(" listen: 1", b':'));
        assert!(!entry_follows(" = 7835", b'='));
        assert!(!entry_follows("", b'='));
        assert!(!entry_follows("}", b'='));
    }

    #[test]
    fn inline_values_end_at_a_comma_or_closer() {
        assert_eq!(inline_value_len("fast, b = 1 }"), 4);
        assert_eq!(inline_value_len("\"a, b\" }"), 6);
        assert_eq!(inline_value_len("12 }"), 2);
    }
}

#[cfg(test)]
mod path_span_tests {
    use super::super::language::SchemaPathSegment::{ArrayItem, Key};
    use super::inline_path_span;

    fn text<'a>(code: &'a str, path: &[super::SchemaPathSegment], key: bool) -> &'a str {
        let (start, end) = inline_path_span(code, path, key).unwrap();
        &code[start..end]
    }

    #[test]
    fn finds_items_and_keys_inside_inline_containers() {
        assert_eq!(text("[a, 5]", &[ArrayItem(1)], false), "5");
        assert_eq!(
            text(
                "{allow: [a, 5]}",
                &[Key("allow".into()), ArrayItem(1)],
                false
            ),
            "5"
        );
        assert_eq!(
            text(
                "{hosts: [a], cert: c, bogus: 1}",
                &[Key("bogus".into())],
                true
            ),
            "bogus"
        );
        assert_eq!(
            text(
                "[\n  { hosts = [\"a\"], key = \"k\" },\n]",
                &[ArrayItem(0), Key("key".into())],
                false
            ),
            "\"k\""
        );
        assert_eq!(
            text(
                "{ a = { b = [1, \"x, y\", 3] } }",
                &[Key("a".into()), Key("b".into()), ArrayItem(2)],
                false
            ),
            "3"
        );
        assert!(inline_path_span("[1]", &[ArrayItem(3)], false).is_none());
    }
}
