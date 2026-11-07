//! Adding a language: implement [`ConfigLanguage`] and wrap it with
//! [`Language::new`](crate::Language::new).
//!
//! The built-in languages implement the same trait, so everything they do is
//! available to an external language too: diagnostics positions, schema
//! completion, hover documentation, highlighting, editing, and conversion.
//! The helpers re-exported here are the building blocks they use.
//!
//! ```
//! use config_editor_leptos::Language;
//! use config_editor_leptos::language::{
//!     ConfigLanguage, LanguageInfo, ParseError, Token, TokenKind, Tokens,
//! };
//! use serde_json::{Map, Value};
//!
//! /// `KEY=value` lines.
//! struct DotEnv;
//!
//! impl ConfigLanguage for DotEnv {
//!     fn info(&self) -> LanguageInfo {
//!         LanguageInfo { label: "ENV", icon: "E", extensions: &["env"] }
//!     }
//!
//!     fn parse(&self, source: &str) -> Result<Value, ParseError> {
//!         let mut map = Map::new();
//!         for (index, line) in source.lines().enumerate() {
//!             if line.trim().is_empty() {
//!                 continue;
//!             }
//!             let (key, value) = line.split_once('=').ok_or_else(|| ParseError {
//!                 message: "expected KEY=value".to_string(),
//!                 line: Some(index + 1),
//!                 column: Some(1),
//!             })?;
//!             map.insert(key.trim().to_string(), Value::String(value.trim().to_string()));
//!         }
//!         Ok(Value::Object(map))
//!     }
//!
//!     fn highlight(&self, line: &str) -> Vec<Token> {
//!         let mut tokens = Tokens::new();
//!         if let Some(equals) = line.find('=') {
//!             tokens.push(0, equals, TokenKind::Key);
//!             tokens.push(equals, equals + 1, TokenKind::Punctuation);
//!         }
//!         tokens.finish()
//!     }
//! }
//!
//! static DOTENV: DotEnv = DotEnv;
//! let env = Language::new(&DOTENV);
//! assert_eq!(env.parse("PORT=7835\n").unwrap()["PORT"], "7835");
//! // A minimal language already converts into every built-in one.
//! # #[cfg(feature = "yaml")]
//! assert_eq!(env.convert("PORT=7835\n", Language::YAML).unwrap().text, "PORT: \"7835\"\n");
//! ```

pub use super::codec::first_pointer;
pub use super::commands::indent_unit;
pub use super::completion::{
    array_branch, object_branch, placeholder, schema_const, schema_default,
};
pub use super::highlight::{Token, TokenKind, Tokens};
pub use super::inline::{
    InlineCaret, InlineEntry, InlineKey, caret_value_end, entry_follows, in_front_of_inline_entry,
    inline_caret, inline_key_at, inline_key_paths, inline_keys, inline_sibling_keys,
    inline_value_len, skip_quoted, statement_start, statements, string_end_at_caret,
};
pub use super::keyboard::utf16_to_byte_index;
use serde_json::Value;
use std::collections::HashSet;

/// A syntax error in a document, with its one-based position when the
/// language reports one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// What is wrong, as the language parser reports it.
    pub message: String,
    /// One-based line.
    pub line: Option<usize>,
    /// One-based character column.
    pub column: Option<usize>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => {
                write!(formatter, "{} (line {line}, column {column})", self.message)
            }
            (Some(line), None) => write!(formatter, "{} (line {line})", self.message),
            _ => formatter.write_str(&self.message),
        }
    }
}

impl std::error::Error for ParseError {}

/// One step of a path into the parsed document, used for schema navigation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SchemaPathSegment {
    /// A mapping key.
    Key(String),
    /// An array index.
    ArrayItem(usize),
}

/// How an entry inserted at the caret must be written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EntryPosition {
    /// A child entry in a multi-line body (the normal case).
    #[default]
    Block,
    /// Inside a container that must stay on one line, such as a TOML inline
    /// table: nested objects are written inline too.
    Inline,
    /// An XML-style attribute on the current tag (`name="value"`).
    Attribute,
    /// Only the key's name, such as a segment of a TOML table header
    /// (`[web.lets|]`): no separator or value.
    Name,
}

/// What [`ConfigLanguage::property_insert`] needs to lay out an insert.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct InsertContext<'a> {
    /// Indentation of the line the entry starts on.
    pub base_indent: &'a str,
    /// One indentation level, as the document writes it.
    pub unit: &'a str,
    /// How the entry must be written.
    pub position: EntryPosition,
    /// Schema path of the container receiving the entry.
    pub path: &'a [SchemaPathSegment],
    /// The entry is an array item: with [`EntryPosition::Name`], a TOML
    /// `[[array]]` header rather than a `[table]` one.
    pub array_item: bool,
}

/// How a multi-line insert is laid out when the caret follows containers
/// opened on the same line (`"web": {"le|}`): the insert moves onto its own
/// line at `indent`, and whatever followed the caret moves to `after`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InlineBlock {
    /// Start of the UTF-16 replace range; it swallows the blanks around the
    /// caret so no trailing whitespace is left behind.
    pub start: usize,
    /// End of the UTF-16 replace range.
    pub end: usize,
    /// Indentation of the entry's own line.
    pub indent: String,
    /// Line break plus closing indent before a closer, separator plus line
    /// break before a following entry, or empty.
    pub after: String,
}

/// How many `{` / `[` opened on `line_prefix` are still open, ignoring
/// brackets inside double-quoted strings.
pub fn unclosed_brackets(line_prefix: &str) -> usize {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for character in line_prefix.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        match character {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// Enter in bracketed syntax, for the `pairs` the language allows to span
/// lines:
/// - between a pair (`[|]`) the caret lands on an indented middle line and
///   the closer moves below it;
/// - after an opener the new line is indented one level;
/// - before a closer, the closer moves to a new line aligned with the line
///   that opened it;
/// - otherwise the line's indentation is kept.
///
/// `code` strips a trailing comment from the text before the caret on its
/// line, so an opener inside a comment does not count.
pub fn bracket_newline(
    source: &str,
    cursor: usize,
    pairs: &[(char, char)],
    code: impl Fn(&str) -> &str,
) -> Newline {
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    // Only indentation before the caret counts: Enter at column 0 of an
    // indented line leaves an empty line, the line keeping its indent.
    let indent = line_indent(&source[line_start..byte]);
    let before = code(&source[line_start..byte]).trim_end_matches([' ', '\t']);
    let after = source[byte..].trim_start_matches([' ', '\t']);
    // Brackets inside a string literal are text.
    let in_string = inside_string(before);
    let opens = before
        .chars()
        .last()
        .filter(|last| !in_string && pairs.iter().any(|(open, _)| open == last));
    let closes = after
        .chars()
        .next()
        .filter(|next| !in_string && pairs.iter().any(|(_, close)| close == next));
    let unit = super::commands::indent_unit(source);
    match (opens, closes) {
        (Some(_), Some(_)) => Newline::with_caret(
            format!("\n{indent}{unit}\n{indent}"),
            1 + indent.len() + unit.len(),
        ),
        (Some(_), None) => Newline::new(format!("\n{indent}{unit}")),
        (None, Some(closer)) => {
            let opener = pairs
                .iter()
                .find(|(_, close)| *close == closer)
                .map(|(open, _)| *open);
            let aligned = opener
                .and_then(|opener| opener_line(source, byte, opener, closer))
                .map_or(indent, |start| line_indent(&source[start..]));
            Newline::new(format!("\n{aligned}"))
        }
        // Inside a bracket opened earlier on this line (`[1,| 2]`): one
        // level deeper, as after the opener itself.
        (None, None) if !in_string && opened_on_line(before, pairs) => {
            Newline::new(format!("\n{indent}{unit}"))
        }
        (None, None) => Newline::new(format!("\n{indent}")),
    }
}

/// Whether a bracket of `pairs` opened in `line` is still open at its end,
/// skipping quoted text.
fn opened_on_line(line: &str, pairs: &[(char, char)]) -> bool {
    let mut depth = 0isize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for character in line.chars() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if character == '\\' => escaped = true,
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '"' => quote = Some('"'),
            None if pairs.iter().any(|(open, _)| *open == character) => depth += 1,
            None if pairs.iter().any(|(_, close)| *close == character) => depth -= 1,
            None => {}
        }
    }
    depth > 0
}

/// Whether the end of `line` (text before the caret) is inside a string: a
/// `"` string with `\\` escapes, or a `'` string opened where a value starts.
fn inside_string(line: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut previous = ' ';
    for character in line.chars() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if character == '\\' => escaped = true,
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '"' => quote = Some('"'),
            None if character == '\''
                && (previous.is_whitespace()
                    || matches!(previous, ':' | '=' | '[' | '{' | ',' | '(')) =>
            {
                quote = Some('\'');
            }
            None => {}
        }
        previous = character;
    }
    quote.is_some()
}

fn line_indent(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// The start of the line holding the innermost `opener` still open at
/// `byte`, skipping double-quoted strings.
fn opener_line(source: &str, byte: usize, opener: char, closer: char) -> Option<usize> {
    let mut open = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    for (index, character) in source[..byte].char_indices() {
        if in_string {
            match character {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
        } else if character == '"' {
            in_string = true;
        } else if character == opener {
            open.push(index);
        } else if character == closer {
            open.pop();
        }
    }
    let index = *open.last()?;
    Some(source[..index].rfind('\n').map_or(0, |newline| newline + 1))
}

/// Ctrl+/ on lines that are each wrapped in a block comment (`/* a = 1 */`):
/// the markers removed. `None` when any selected line is not wrapped, so
/// line comments take over. Offsets are UTF-16; returns the text and the
/// selection over the unwrapped lines.
pub fn unwrap_block_comments(
    source: &str,
    selection_start: usize,
    selection_end: usize,
    (open, close): (&str, &str),
) -> Option<(String, usize, usize)> {
    let start = utf16_to_byte_index(source, selection_start);
    let end = utf16_to_byte_index(source, selection_end);
    // Inside one block comment over several lines, its markers on lines of
    // their own: they go. (One sharing its lines with code is commented
    // line by line instead: its text would become code.)
    let own_lines = |at: usize, close_end: usize| {
        let line_start = source[..at].rfind('\n').map_or(0, |newline| newline + 1);
        let line_end = source[close_end..].find('\n').map_or(source.len(), |offset| close_end + offset);
        source[line_start..at].trim().is_empty() && source[close_end..line_end].trim().is_empty()
    };
    if let Some(at) = source[..start].rfind(open)
        && !source[at..start].contains(close)
        && let Some(close_at) = source[end..].find(close).map(|offset| end + offset)
        && source[at..close_at].contains('\n')
        && !source[at + open.len()..close_at].contains(open)
        && own_lines(at, close_at + close.len())
    {
        let open_end = at + open.len() + usize::from(source[at + open.len()..].starts_with(' '));
        let close_start =
            close_at - usize::from(source[..close_at].ends_with(' ') && close_at > open_end);
        let updated = format!(
            "{}{}{}",
            &source[..at],
            &source[open_end..close_start],
            &source[close_at + close.len()..]
        );
        let removed = open_end - at;
        let map = |byte: usize| {
            let byte = byte.clamp(open_end, close_start) - removed;
            updated[..byte].encode_utf16().count()
        };
        return Some((updated.clone(), map(start), map(end)));
    }
    let first_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let last_end = source[end..].find('\n').map_or(source.len(), |at| end + at);
    let block = &source[first_start..last_end];
    let wrapped = |line: &str| {
        let trimmed = line.trim();
        trimmed.len() >= open.len() + close.len()
            && trimmed.starts_with(open)
            && trimmed.ends_with(close)
    };
    let mut content = block
        .lines()
        .filter(|line| !line.trim().is_empty())
        .peekable();
    if content.peek().is_none() || !content.all(wrapped) {
        return None;
    }
    // Each line unwrapped, with what it takes to map a position on it: its
    // start before and after, its indent, and the bytes cut before the text.
    let mut replaced = String::new();
    let mut lines: Vec<(usize, usize, usize, usize, usize)> = Vec::new();
    let mut offset = first_start;
    for (index, line) in block.split('\n').enumerate() {
        if index > 0 {
            replaced.push('\n');
        }
        let new_start = first_start + replaced.len();
        if line.trim().is_empty() {
            replaced.push_str(line);
            lines.push((offset, new_start, line.len(), 0, line.len()));
        } else {
            let indent = line.len() - line.trim_start().len();
            let body = line.trim();
            let inner = &body[open.len()..body.len() - close.len()];
            let cut = open.len() + (inner.len() - inner.trim_start().len());
            let text = inner.trim();
            replaced.push_str(&line[..indent]);
            replaced.push_str(text);
            lines.push((offset, new_start, indent, cut, text.len()));
        }
        offset += line.len() + 1;
    }
    let updated = format!(
        "{}{replaced}{}",
        &source[..first_start],
        &source[last_end..]
    );
    // A position keeps its place in the text; on a marker it moves to the
    // text's nearest end.
    let map = |byte: usize| {
        let (start, new_start, indent, cut, text) = lines
            .iter()
            .rev()
            .find(|(start, ..)| *start <= byte)
            .copied()
            .unwrap_or((first_start, first_start, 0, 0, 0));
        let column = byte - start;
        let mapped = if column <= indent {
            column
        } else {
            (column.saturating_sub(cut)).clamp(indent, indent + text)
        };
        updated[..new_start + mapped].encode_utf16().count()
    };
    Some((updated.clone(), map(start), map(end)))
}

/// Ctrl+/ for line-comment languages: the selected lines (UTF-16 offsets)
/// commented at the block's smallest indent with `markers[0]`, or, when every
/// non-blank line already starts with one of `markers`, uncommented. Returns
/// the text and the selection mapped onto it.
///
/// ```
/// use config_editor_leptos::language::toggle_line_comments;
/// let (text, ..) = toggle_line_comments("a:\n  b: 1\n", 0, 9, &["#"]).unwrap();
/// assert_eq!(text, "# a:\n#   b: 1\n");
/// ```
pub fn toggle_line_comments(
    source: &str,
    selection_start: usize,
    selection_end: usize,
    markers: &[&str],
) -> Option<(String, usize, usize)> {
    let insert = *markers.first()?;
    let start = utf16_to_byte_index(source, selection_start);
    let end = utf16_to_byte_index(source, selection_end);
    let first_line = source[..start].matches('\n').count();
    let last_line = source[..end]
        .matches('\n')
        .count()
        .saturating_sub(usize::from(start < end && source[..end].ends_with('\n')));
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    if first_line >= lines.len() {
        // The caret is on the empty last line: start a comment there.
        if start != end || start != source.len() {
            return None;
        }
        let updated = format!("{source}{insert} ");
        let caret = updated.encode_utf16().count();
        return Some((updated, caret, caret));
    }
    let last_line = last_line.min(lines.len() - 1);
    let body = |line: &str| {
        line.trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_string()
    };
    let indent_of = |line: &str| line.len() - line.trim_start_matches([' ', '\t']).len();
    let selected: Vec<String> = lines[first_line..=last_line]
        .iter()
        .map(|line| body(line))
        .collect();
    let content_lines = selected.iter().filter(|line| !line.trim().is_empty());
    let marker_of = |content: &str| {
        markers
            .iter()
            .copied()
            .find(|marker| content.starts_with(marker))
    };
    let uncomment = selected
        .iter()
        .filter(|line| !line.trim().is_empty())
        .all(|line| marker_of(&line[indent_of(line)..]).is_some());
    let column = content_lines.map(|line| indent_of(line)).min().unwrap_or(0);
    // On a blank line, start a comment there, after its indentation.
    if first_line == last_line && selected[0].trim().is_empty() {
        let line_start: usize = lines[..first_line].iter().map(|line| line.len()).sum();
        let at = line_start + selected[0].len();
        let mut updated = source.to_string();
        updated.insert_str(at, &format!("{insert} "));
        let caret = updated[..at + insert.len() + 1].encode_utf16().count();
        return Some((updated, caret, caret));
    }

    let mut edits: Vec<(usize, usize, usize)> = Vec::new();
    let mut updated = String::with_capacity(source.len() + lines.len() * (insert.len() + 1));
    let mut offset = 0;
    for (index, line) in lines.iter().enumerate() {
        let text = body(line);
        if (first_line..=last_line).contains(&index) && !text.trim().is_empty() {
            if uncomment {
                let indent = indent_of(&text);
                let marker = marker_of(&text[indent..]).unwrap_or_default();
                let removed =
                    marker.len() + usize::from(text[indent + marker.len()..].starts_with(' '));
                edits.push((offset + indent, removed, 0));
                updated.push_str(&line[..indent]);
                updated.push_str(&line[indent + removed..]);
            } else {
                let added = format!("{insert} ");
                edits.push((offset + column, 0, added.len()));
                updated.push_str(&line[..column]);
                updated.push_str(&added);
                updated.push_str(&line[column..]);
            }
        } else {
            updated.push_str(line);
        }
        offset += line.len();
    }
    let map = |position: usize, right_biased: bool| {
        let mut delta = 0isize;
        for (at, removed, inserted) in &edits {
            if position < *at {
                break;
            }
            if position == *at && *removed == 0 && !right_biased {
                return (*at as isize + delta) as usize;
            }
            if position <= *at + *removed {
                return (*at as isize + delta + *inserted as isize) as usize;
            }
            delta += *inserted as isize - *removed as isize;
        }
        (position as isize + delta) as usize
    };
    let new_start = map(start, start == end).min(updated.len());
    let new_end = map(end, true).min(updated.len());
    let utf16 = |byte: usize| updated[..byte].encode_utf16().count();
    Some((updated.clone(), utf16(new_start), utf16(new_end)))
}

/// The [`InlineBlock`] for the byte range `start..end` on a line where `depth`
/// containers are still open; `None` when `depth` is zero. `closes` tells
/// whether text starts with the language's closer (`}`, `]`, `</`);
/// `separator` goes between the insert and a following entry (`,` in JSON).
pub fn inline_block(
    source: &str,
    start: usize,
    end: usize,
    depth: usize,
    unit: &str,
    closes: impl Fn(&str) -> bool,
    separator: &str,
) -> Option<InlineBlock> {
    if depth == 0 {
        return None;
    }
    let start = start.min(source.len());
    let end = end.clamp(start, source.len());
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |index| end + index);
    let line_indent: String = source[line_start..]
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let indent = format!("{line_indent}{}", unit.repeat(depth));
    let before = source[line_start..start].trim_end_matches([' ', '\t']);
    let block_start = line_start + before.len();
    let rest = &source[end..line_end];
    let following = rest.trim_start_matches([' ', '\t']);
    let (after, block_end) = if following.is_empty() {
        (String::new(), end)
    } else if closes(following) {
        (
            format!("\n{line_indent}{}", unit.repeat(depth - 1)),
            end + (rest.len() - following.len()),
        )
    } else {
        // Swallow the separator already there so exactly one is written.
        let following = match following.strip_prefix(separator) {
            Some(after) if !separator.is_empty() => after.trim_start_matches([' ', '\t']),
            _ => following,
        };
        (
            format!("{separator}\n{indent}"),
            end + (rest.len() - following.len()),
        )
    };
    Some(InlineBlock {
        start: source[..block_start].encode_utf16().count(),
        end: source[..block_end].encode_utf16().count(),
        indent,
        after,
    })
}

/// Where the caret sits in the document's structure, as a language reports
/// it. Build one with struct update syntax:
/// `CompletionContext { path, typed, ..Default::default() }`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompletionContext {
    /// Schema path of the mapping holding the caret.
    pub path: Vec<SchemaPathSegment>,
    /// The query typed so far.
    pub typed: String,
    /// Start of the UTF-16 range an accepted suggestion replaces.
    pub replace_start: usize,
    /// End of that range.
    pub replace_end: usize,
    /// Set when the caret is in the value of this key (under `path`).
    pub value_key: Option<String>,
    /// A value insert needs a leading space (`key:|`).
    pub value_needs_space: bool,
    /// The caret is in an array item (`path` ends at the item).
    pub is_array_item: bool,
    /// Indentation of the caret's line.
    pub base_indent: String,
    /// Prepended verbatim to an inserted entry (for example a separator after
    /// a complete value).
    pub entry_prefix: String,
    /// An empty query still opens the suggestion list (for example after a
    /// typed opening quote in JSON).
    pub explicit_entry_start: bool,
    /// Appended verbatim to an inserted entry, after the caret (for example a
    /// separator before a following JSON entry).
    pub entry_suffix: String,
    /// How a property inserted here is written.
    pub position: EntryPosition,
    /// Containers opened earlier on the line; see [`inline_block`].
    pub block: Option<InlineBlock>,
    /// No property may be inserted here (for example the XML config root's
    /// start tag, whose attributes are editor metadata).
    pub no_properties: bool,
    /// The value position can also hold child entries (an empty XML element
    /// between its tags): an object value offers its properties as children.
    pub value_or_children: bool,
    /// Delimiters a new property entry must be written between: a key typed
    /// directly in an array starts an object item (`("{ ", " }")` in TOML),
    /// and the first key of an empty JSON document opens its root object.
    /// `None` where properties can follow directly (YAML `- `).
    pub entry_wrap: Option<(&'static str, &'static str)>,
    /// An existing entry starts right after the caret (`lim|listen = 1`),
    /// and this is the word its key begins with. A suggestion ending in that
    /// word completes it (`inse|cure_plaintext`) and renames the key; any
    /// other is inserted before the entry, `entry_suffix` moving the entry to
    /// its own line.
    pub entry_after: Option<String>,
    /// The other places the key being renamed is written (UTF-16 ranges,
    /// after the caret: an XML end tag's name, a TOML table's sub-tables and
    /// dotted keys): a suggestion that renames the key writes it there too.
    pub paired_names: Vec<(usize, usize)>,
    /// The caret ends a name in a complete tag (`<hea|>`, `<hea|/>`): a
    /// suggestion writes just the name (and its `paired_names`).
    pub names_only: bool,
    /// The caret is in a closing tag being typed (`</we|`): the only
    /// suggestion is this name, the element it closes, written with what
    /// ends the tag (`websocket>`), renaming nothing.
    pub closing_name: Option<(String, String)>,
    /// Bytes of `entry_suffix` the caret goes past: a new TOML table named
    /// in front of another (`[hea|websocket]`) leaves the caret in its own
    /// body, after `]` and the line break.
    pub suffix_caret: usize,
    /// When an entry is inserted (not a rename), the UTF-16 range it
    /// replaces instead of `replace_start..replace_end`, with
    /// `entry_prefix` and `entry_suffix` rewriting what was there: an HCL
    /// block on one line opens over lines to take a second attribute.
    pub insert_range: Option<(usize, usize)>,
}

/// Splits a JSON pointer into unescaped segments.
pub fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// How a language writes arrays besides literal lists; see
/// [`ConfigLanguage::repetition`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Repetition {
    /// Arrays are always written as lists.
    #[default]
    None,
    /// Any value can be repeated (XML sibling elements).
    Elements,
    /// Objects can be repeated as blocks (HCL `certificates { … }`); other
    /// values are always lists.
    Blocks,
}

/// What Enter writes: `text` replaces the caret's selection plus the next
/// `replaces` bytes, and the caret lands `caret` bytes into `text`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Newline {
    /// The text written.
    pub text: String,
    /// The caret's byte offset within `text`.
    pub caret: usize,
    /// Bytes after the caret that `text` replaces, so Enter can step over
    /// closers (`{ a = 1| }` in TOML moves past ` }`).
    pub replaces: usize,
    /// Bytes before the caret that `text` replaces, so Enter can clear what
    /// it would otherwise continue (an empty YAML `- ` item).
    pub replaces_before: usize,
}

impl Newline {
    /// `text` written at the caret, with the caret after it.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Newline {
            caret: text.len(),
            text,
            replaces: 0,
            replaces_before: 0,
        }
    }

    /// `text` written at the caret, with the caret `caret` bytes into it.
    pub fn with_caret(text: impl Into<String>, caret: usize) -> Self {
        Newline {
            text: text.into(),
            caret,
            replaces: 0,
            replaces_before: 0,
        }
    }
}

/// What the editor shows for a language: its label, toolbar icon, and file
/// extensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LanguageInfo {
    /// Short UI label, such as `"YAML"`; also the language's identity.
    pub label: &'static str,
    /// Single-character toolbar icon.
    pub icon: &'static str,
    /// File extensions, lowercase and without the dot; the first is preferred.
    pub extensions: &'static [&'static str],
}

/// A one-line source range: one-based `line`, and one-based character
/// columns `start_column..end_column`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// One-based line.
    pub line: usize,
    /// One-based character column where the range starts.
    pub start_column: usize,
    /// One-based character column just past the range.
    pub end_column: usize,
}

impl From<(usize, usize, usize)> for Span {
    fn from((line, start_column, end_column): (usize, usize, usize)) -> Self {
        Span {
            line,
            start_column,
            end_column,
        }
    }
}

/// The lexical state at the caret: inside a string (and its quote) or a
/// comment. Brackets and quotes do not auto-close there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lexical {
    /// The opening quote of the string the caret is in.
    pub quote: Option<char>,
    /// The caret is in a comment.
    pub in_comment: bool,
}

/// Syntax a language can describe as data instead of implementing editing
/// hooks, like VS Code's `language-configuration.json`. The defaults of
/// [`newline`](ConfigLanguage::newline),
/// [`toggle_comments`](ConfigLanguage::toggle_comments),
/// [`comment_start`](ConfigLanguage::comment_start) and the key/value hooks
/// come from it, so a simple language gets them without code; a method a
/// language implements itself takes precedence.
///
/// ```
/// use config_editor_leptos::language::LanguageConfig;
///
/// const INI: LanguageConfig = LanguageConfig {
///     line_comments: &[";", "#"],
///     block_comment: None,
///     brackets: &[],
///     separators: &["="],
/// };
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LanguageConfig {
    /// Markers that start a comment running to the end of the line. `Ctrl+/`
    /// writes the first; all of them are recognized.
    pub line_comments: &'static [&'static str],
    /// The markers around a block comment (`("/*", "*/")`), if the language
    /// has them: `Ctrl+/` on lines each wrapped in one unwraps them.
    pub block_comment: Option<(&'static str, &'static str)>,
    /// Bracket pairs: Enter between them opens an indented line, and a closer
    /// typed on its own line aligns with its opener.
    pub brackets: &'static [(char, char)],
    /// What separates a key from its value (`":"`, `"="`): completing a key
    /// that already has a value renames only the key, a space after a complete
    /// value ends the entry, and `Tab` walks values left empty after one.
    pub separators: &'static [&'static str],
}

/// A document language for the editor.
///
/// Only [`info`](Self::info) and [`parse`](Self::parse) are required; each
/// other method adds one capability (diagnostic positions, completion, hover,
/// highlighting, conversion, editing) and defaults to doing without it.
/// Offsets named `cursor` and returned carets are UTF-16 indices into the
/// document, as the browser reports them.
///
/// Simple syntax (comments, brackets, key/value separators) can be described
/// as data with [`config`](Self::config) instead of code. The last five
/// methods are optional refinements for some kinds of languages, with
/// defaults from that config: key/value languages ([`value_follows_key`](Self::value_follows_key),
/// [`line_has_value`](Self::line_has_value),
/// [`empty_value_slots`](Self::empty_value_slots)), markup
/// ([`closing_markup_len`](Self::closing_markup_len)), and languages with
/// trailing comments ([`comment_start`](Self::comment_start)). Skip the ones
/// that don't fit.
///
/// Built-in languages implement this trait the same way an external one does;
/// wrap an implementation with [`Language::new`](crate::Language::new).
pub trait ConfigLanguage: Send + Sync + 'static {
    /// The language's label, icon, and extensions.
    fn info(&self) -> LanguageInfo;

    /// Syntax described as data; see [`LanguageConfig`]. Default: none.
    fn config(&self) -> LanguageConfig {
        LanguageConfig::default()
    }

    /// The document as the shared data model, or its first syntax error.
    fn parse(&self, source: &str) -> Result<Value, ParseError>;

    /// The document for `value`, keeping the framing of `previous` (this
    /// language's last text for the document) where the language has any.
    ///
    /// # Errors
    ///
    /// A message when the language cannot express `value`. The default
    /// supports no conversion.
    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        let _ = (value, previous);
        Err(format!("{} documents cannot be written", self.info().label))
    }

    /// The span of the key at the JSON pointer `pointer`, for diagnostics.
    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        let _ = (source, pointer);
        None
    }

    /// The span of the value at the JSON pointer `pointer`, for diagnostics.
    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        let _ = (source, pointer);
        None
    }

    /// [`value_span`](Self::value_span) for each of `pointers`. A language
    /// whose lookup reads the whole document overrides it to read it once.
    fn value_spans(&self, source: &str, pointers: &[String]) -> Vec<Option<Span>> {
        pointers
            .iter()
            .map(|pointer| self.value_span(source, pointer))
            .collect()
    }

    /// Where whole-document problems are marked: the first non-blank
    /// character.
    fn first_span(&self, source: &str) -> Option<Span> {
        source.lines().enumerate().find_map(|(index, line)| {
            let start = line
                .chars()
                .position(|character| !character.is_whitespace())?;
            Some(Span {
                line: index + 1,
                start_column: start + 1,
                end_column: start + 2,
            })
        })
    }

    /// The caret's place in the document's structure, for completion and
    /// hover. The default is the top level with nothing typed.
    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        let _ = (source, cursor);
        CompletionContext::default()
    }

    /// Keys already present in the mapping at `path`, which completion skips.
    /// `cursor_byte` is a byte index; the caret's own line is ignored so a
    /// half-typed key hides nothing.
    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        let _ = (source, cursor_byte, path);
        None
    }

    /// The text inserted for property `key` and the caret offset (UTF-16)
    /// within it; `None` skips the property.
    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        let _ = (key, property, root, context);
        None
    }

    /// A schema value (default, const, example) as document text.
    fn format_value(&self, value: &Value) -> String {
        value.to_string()
    }

    /// `value` written in place of `written` (a one-line container being
    /// edited), in its style: compact JSON stays compact.
    fn format_value_like(&self, value: &Value, written: &str) -> String {
        let _ = written;
        self.format_value(value)
    }

    /// The key under character `column` of `line`, and the column it starts at.
    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        let _ = (line, column);
        None
    }

    /// The schema path of the key under character `column` of line
    /// `line_index`, with the key and the column it starts at, for keys the
    /// completion context cannot place (a segment of a dotted key or table
    /// header, a block type). `None` uses [`hover_key`](Self::hover_key).
    fn hover_path(
        &self,
        source: &str,
        line_index: usize,
        column: usize,
    ) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        let _ = (source, line_index, column);
        None
    }

    /// The byte offset in `line` where the hover context for the key at
    /// `column` is read: past the key's separator when one line holds
    /// several keys.
    fn hover_cursor(&self, line: &str, column: usize) -> usize {
        let _ = column;
        line.len()
    }

    /// The highlighted runs of one line; uncovered text renders plain.
    fn highlight(&self, line: &str) -> Vec<Token> {
        let _ = line;
        Vec::new()
    }

    /// What Enter writes at `cursor` (UTF-16). The default keeps the line's
    /// indentation and opens the [`LanguageConfig::brackets`] it is between.
    fn newline(&self, source: &str, cursor: usize) -> Newline {
        bracket_newline(source, cursor, self.config().brackets, |line| {
            &line[..self.comment_start(line).unwrap_or(line.len())]
        })
    }

    /// The document with the selected lines commented or uncommented, and the
    /// remapped selection; `None` when the language has no comments. The
    /// default uses [`LanguageConfig::line_comments`].
    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        let config = self.config();
        // Whole strings and constructs over lines, until both hold.
        let (mut selection_start, mut selection_end) = (selection_start, selection_end);
        loop {
            let widened = over_whole_strings(self, source, selection_start, selection_end);
            let widened = super::comment_scope::over_balanced_lines(self, source, widened.0, widened.1);
            if widened == (selection_start, selection_end) {
                break;
            }
            (selection_start, selection_end) = widened;
        }
        config
            .block_comment
            .and_then(|block| unwrap_block_comments(source, selection_start, selection_end, block))
            .or_else(|| {
                toggle_line_comments(source, selection_start, selection_end, config.line_comments)
            })
    }

    /// Text to insert after the caret when `typed` is typed at `cursor`
    /// (UTF-16): the end tag XML writes when `>` finishes `<bob>`, or the
    /// closing `"""` of a TOML multi-line string. Checked before the shared
    /// bracket and quote pairing.
    fn auto_close(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        let _ = (source, cursor, typed);
        None
    }

    /// The indentation the caret's line should take when `typed` is typed at
    /// `cursor` (UTF-16), for a line that only now shows it closes a block:
    /// an XML end tag typed on its own line moves out one level.
    fn reindent_on_type(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        let _ = (source, cursor, typed);
        None
    }

    /// Whether arrays can be written by repeating their key. A single entry
    /// then reads back as a bare value, so where the schema expects an array
    /// it is read as a one-item array before validating or converting, and an
    /// array key already present is offered again.
    fn repetition(&self) -> Repetition {
        Repetition::None
    }

    /// Whether the caret has just begun an entry (a JSON key's opening
    /// quote), so suggestions open before anything is typed.
    fn entry_started(&self, source: &str, cursor: usize) -> bool {
        let _ = (source, cursor);
        false
    }

    /// Whether the caret is in a string or a comment.
    fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        let _ = (source, cursor);
        Lexical::default()
    }

    /// Lines (0-based, end exclusive) that lie wholly inside the body of a
    /// multi-line string (TOML `"""`, an HCL heredoc, a YAML block scalar).
    /// They are text: Enter keeps their indentation, nothing is paired,
    /// aligned or suggested there, and they are colored as a string.
    fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        let _ = source;
        Vec::new()
    }

    /// The byte offset where the trailing comment of `line` starts, outside
    /// strings. Edit-as uses it to match lines that differ only in comments.
    /// See [`comment_start_outside_quotes`].
    fn comment_start(&self, line: &str) -> Option<usize> {
        comment_start_outside_quotes(line, self.config().line_comments)
    }

    /// Whether `rest`, the text after a key being completed, shows the key
    /// already has its value (`: 99`, ` = "x"`, an HCL block's `{`), so
    /// accepting a suggestion renames only the key.
    fn value_follows_key(&self, rest: &str) -> bool {
        let rest = rest.trim_start();
        // `==` is a comparison, not a separator.
        self.config().separators.iter().any(|separator| {
            rest.strip_prefix(separator)
                .is_some_and(|after| !after.starts_with(separator))
        })
    }

    /// Whether `line_before` (the caret's line up to the caret, without
    /// trailing whitespace) already holds a complete value (`key = false`),
    /// so a space typed after it ends the entry instead of opening values.
    /// See [`value_after_separator`].
    fn line_has_value(&self, line_before: &str) -> bool {
        let separators: Vec<char> = self
            .config()
            .separators
            .iter()
            .filter_map(|separator| separator.chars().next())
            .collect();
        !separators.is_empty() && value_after_separator(line_before, &separators)
    }

    /// Byte offsets in `text`, a block a suggestion just wrote, where a value
    /// is still empty besides empty quotes (`key: ` at a line end, `<k></k>`):
    /// `Tab` walks them like snippet placeholders. See [`empty_slots_at_line_ends`].
    fn empty_value_slots(&self, text: &str) -> Vec<usize> {
        let endings: Vec<String> = self
            .config()
            .separators
            .iter()
            .flat_map(|separator| [format!("{separator} "), (*separator).to_string()])
            .collect();
        let endings: Vec<&str> = endings.iter().map(String::as_str).collect();
        empty_slots_at_line_ends(text, &endings)
    }

    /// The length in bytes of markup at the start of `text` that closes an
    /// element, such as an XML end tag. The editor remembers the ones it
    /// writes, and Enter after a value typed before one steps past it.
    fn closing_markup_len(&self, text: &str) -> Option<usize> {
        let _ = text;
        None
    }

    /// The container at `pointer` when the document writes it on one line in
    /// a form other than a `{…}` or `[…]` value: an XML element with its
    /// children on one line, an HCL one-line block. A structural Edit-as
    /// save keeps such a container as written.
    fn one_line_container(&self, source: &str, pointer: &str) -> Option<OneLine> {
        let _ = (source, pointer);
        None
    }

    /// `source` with the key at `pointer` renamed `new_key` everywhere the
    /// language writes it (an XML element's start and end tags), or `None`
    /// to rename just the text at [`key_span`](Self::key_span).
    fn rename_key(&self, source: &str, pointer: &str, new_key: &str) -> Option<String> {
        let _ = (source, pointer, new_key);
        None
    }

    /// The text for the `value` range of a [`OneLine`] now holding `value`
    /// (`{ nodelay = true }`), so a save edits the container where it is.
    /// When one line cannot hold it, the text may span lines, indented by
    /// `unit` from the container's line indent `indent`.
    fn one_line_body(&self, value: &Value, indent: &str, unit: &str) -> Option<String> {
        let _ = (value, indent, unit);
        None
    }
}

/// A container written on one line (see
/// [`ConfigLanguage::one_line_container`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OneLine {
    /// The bytes kept as written.
    pub range: std::ops::Range<usize>,
    /// What the language writes in their place before a scalar value: `""`
    /// when the range is the value itself, `tcp = ` for `tcp { … }`.
    pub written: String,
    /// The bytes that also read as the container's value (`{ … }` of
    /// `tcp { … }`), for where the serializer writes it as a value.
    pub value: std::ops::Range<usize>,
}

/// The byte offset of the first of `markers` on `line` that starts a comment:
/// outside `"…"` and `'…'` strings, at the line start or after whitespace.
/// A building block for [`ConfigLanguage::comment_start`].
pub fn comment_start_outside_quotes(line: &str, markers: &[&str]) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut previous = ' ';
    for (index, character) in line.char_indices() {
        match quote {
            Some(open) if character == open && previous != '\\' => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if previous.is_whitespace()
                && markers
                    .iter()
                    .any(|marker| line[index..].starts_with(marker)) =>
            {
                return Some(index);
            }
            None => {}
        }
        previous = character;
    }
    None
}

/// Whether `line_before` ends with a value after its last `separators`
/// character (`key = false`, not `key = ` or `key = [`). A building block for
/// [`ConfigLanguage::line_has_value`].
pub fn value_after_separator(line_before: &str, separators: &[char]) -> bool {
    let value = line_before
        .rsplit_once(separators)
        .map_or("", |(_, value)| value)
        .trim();
    !value.is_empty() && !value.ends_with(['[', '{', ',']) && !value.ends_with(separators)
}

/// Byte offsets at the ends of lines of `text` that end with one of
/// `endings` (`"key: "`, `" = "`). A building block for
/// [`ConfigLanguage::empty_value_slots`].
pub fn empty_slots_at_line_ends(text: &str, endings: &[&str]) -> Vec<usize> {
    let mut slots = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        if endings.iter().any(|ending| body.ends_with(ending)) {
            slots.push(offset + body.len());
        }
        offset += line.len();
    }
    slots
}

/// The selection `start..end` (UTF-16) widened so it never ends inside a
/// multi-line string: a string it touches is taken whole, from the line
/// that opens it to the one that closes it. Comment markers then go on the
/// entry's lines, never into the string's text.
pub fn over_whole_strings(
    behavior: &(impl ConfigLanguage + ?Sized),
    source: &str,
    start: usize,
    end: usize,
) -> (usize, usize) {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut line_starts = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in &lines {
        line_starts.push(offset);
        offset += line.len() + 1;
    }
    // Lines inside a string: the language's string bodies, and lines that
    // start inside an open quote (a quoted string over several lines).
    let mut inside = vec![false; lines.len()];
    for range in behavior.string_body_lines(source) {
        let end = range.end.min(lines.len());
        if range.start < end {
            inside[range.start..end].fill(true);
        }
    }
    for (line, &at) in line_starts.iter().enumerate().skip(1) {
        let utf16 = source[..at].encode_utf16().count();
        if behavior.lexical_state(source, utf16).quote.is_some() {
            inside[line] = true;
        }
    }
    // A closing delimiter alone after a body (`"""`, `'''`, `EOT`).
    let closes = |line: usize| {
        lines.get(line).is_some_and(|text| {
            let trimmed = text.trim();
            trimmed.contains("\"\"\"")
                || trimmed.contains("'''")
                || (!trimmed.is_empty()
                    && trimmed
                        .chars()
                        .all(|character| character.is_alphanumeric() || character == '_'))
        })
    };
    let line_of = |utf16: usize| {
        let byte = utf16_to_byte_index(source, utf16);
        source[..byte].matches('\n').count()
    };
    let (first_line, last_line) = (line_of(start), line_of(end));
    let (mut first, mut last) = (first_line, last_line);
    while first > 0 && inside[first] {
        first -= 1;
    }
    while last + 1 < lines.len() && inside[last + 1] {
        last += 1;
    }
    if inside[last] && last + 1 < lines.len() && closes(last + 1) {
        last += 1;
    }
    if (first, last) == (first_line, last_line) {
        return (start, end);
    }
    let to_utf16 = |byte: usize| source[..byte].encode_utf16().count();
    (
        to_utf16(line_starts[first]),
        to_utf16(line_starts[last] + lines[last].len()),
    )
}

/// Whether UTF-16 `cursor` is on a line inside a multi-line string's body
/// (see [`ConfigLanguage::string_body_lines`]).
pub(crate) fn in_string_body(behavior: &dyn ConfigLanguage, source: &str, cursor: usize) -> bool {
    let byte = utf16_to_byte_index(source, cursor);
    let line = source[..byte].bytes().filter(|byte| *byte == b'\n').count();
    behavior
        .string_body_lines(source)
        .iter()
        .any(|lines| lines.contains(&line))
}

#[cfg(test)]
mod tests {
    #[test]
    fn pointer_segments_unescape_in_order() {
        use super::pointer_segments;
        assert_eq!(
            pointer_segments("/a~1b/~0c"),
            vec!["a/b".to_string(), "~c".to_string()]
        );
        assert_eq!(pointer_segments(""), Vec::<String>::new());
    }

    #[test]
    fn comments_start_outside_strings_after_whitespace() {
        use super::comment_start_outside_quotes as start;
        assert_eq!(start("listen = 7835 # port", &["#"]), Some(14));
        assert_eq!(start("# whole line", &["#"]), Some(0));
        assert_eq!(start("color = \"#fff\" # hex", &["#"]), Some(15));
        assert_eq!(start("url = http://x", &["#", "//"]), None);
        assert_eq!(start("a = 1 // note", &["#", "//"]), Some(6));
    }

    #[test]
    fn a_value_after_a_separator_is_complete_unless_it_opens_one() {
        use super::value_after_separator as has_value;
        assert!(has_value("insecure = false", &['=']));
        assert!(!has_value("insecure =", &['=']));
        assert!(!has_value("hosts = [", &['=']));
        assert!(!has_value("pool: {a: 1,", &[':']));
        assert!(!has_value("listen", &[':']));
    }

    #[test]
    fn empty_slots_are_the_ends_of_lines_with_a_separator() {
        use super::empty_slots_at_line_ends as slots;
        let text = "a: \nb: 1\nc:\n";
        assert_eq!(slots(text, &[": ", ":"]), vec![3, 11]);
        assert_eq!(slots("x = \ny = 2", &[" = "]), vec![4]);
    }
}
