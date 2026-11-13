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
    array_branch, fill_value, object_branch, placeholder, schema_const, schema_default,
    schema_example,
};
pub use super::fold_ranges::{
    FoldRange, IndentLine, bracket_fold_ranges, comment_fold_ranges, default_fold_ranges,
    indent_fold_ranges, merge_fold_ranges, string_body_fold_ranges,
};
pub use super::highlight::{ConstructKind, MultilineConstruct, Token, TokenKind, Tokens};
pub use super::keyboard::utf16_to_byte_index;
pub use super::languages::inline::{
    InlineCaret, InlineEntry, InlineKey, caret_value_end, entry_follows, in_front_of_inline_entry,
    inline_caret, inline_key_at, inline_key_paths, inline_keys, inline_sibling_keys,
    inline_value_len, skip_quoted, statement_start, statements, string_end_at_caret,
    unclosed_brackets,
};
pub use item_start::bracket_item_start;
pub(crate) use item_start::cursor_at;

mod item_start;
use super::parse_cache::string_bodies;
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
/// line, so an opener inside a comment does not count. Brackets in the
/// strings of `config`'s quotes are text.
pub fn bracket_newline(
    source: &str,
    cursor: usize,
    pairs: &[(char, char)],
    config: &LanguageConfig,
    code: impl Fn(&str) -> &str,
) -> Newline {
    let strings = Strings::of(config);
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    // Only indentation before the caret counts: Enter at column 0 of an
    // indented line leaves an empty line, the line keeping its indent.
    let indent = line_indent(&source[line_start..byte]);
    let before = code(&source[line_start..byte]).trim_end_matches([' ', '\t']);
    let after = source[byte..].trim_start_matches([' ', '\t']);
    // Brackets inside a string literal are text.
    let in_string = strings.ends_inside(before);
    let opens = before
        .chars()
        .last()
        .filter(|last| !in_string && pairs.iter().any(|(open, _)| open == last));
    let closes = after
        .chars()
        .next()
        .filter(|next| !in_string && pairs.iter().any(|(_, close)| close == next));
    let unit = super::commands::indent_unit(source);
    // Spacing the caret ends a bracket's entry with (`[1, |]`) is not left
    // at the end of the line.
    let raw = &source[line_start..byte];
    let trailing = raw.len() - raw.trim_end_matches([' ', '\t']).len();
    let trimmed = |mut newline: Newline| {
        if !in_string && !raw.trim().is_empty() {
            newline.replaces_before = trailing;
        }
        newline
    };
    let open_here = !in_string && opened_on_line(&strings, before, pairs);
    match (opens, closes) {
        (Some(_), Some(_)) => trimmed(Newline::with_caret(
            format!("\n{indent}{unit}\n{indent}"),
            1 + indent.len() + unit.len(),
        )),
        (Some(_), None) => trimmed(Newline::new(format!("\n{indent}{unit}"))),
        // Before the closer of a bracket opened on this line (`[1, 2, |]`):
        // the entries go on one level deeper, the closer on its own line,
        // as between the brackets.
        (None, Some(_))
            if open_here
                && config
                    .item_separator
                    .is_some_and(|item| before.ends_with(item)) =>
        {
            trimmed(Newline::with_caret(
                format!("\n{indent}{unit}\n{indent}"),
                1 + indent.len() + unit.len(),
            ))
        }
        (None, Some(closer)) => {
            let opener = pairs
                .iter()
                .find(|(_, close)| *close == closer)
                .map(|(open, _)| *open);
            let aligned = opener
                .and_then(|opener| opener_line(&strings, source, byte, opener, closer))
                .map_or(indent, |start| line_indent(&source[start..]));
            trimmed(Newline::new(format!("\n{aligned}")))
        }
        // Inside a bracket opened earlier on this line (`[1,| 2]`): one
        // level deeper, as after the opener itself.
        (None, None) if open_here => trimmed(Newline::new(format!("\n{indent}{unit}"))),
        (None, None) => Newline::new(format!("\n{indent}")),
    }
}

/// The strings of a language's quotes, read character by character: a
/// quote in which a backslash escapes opens anywhere, a literal one (YAML's
/// and TOML's `'`) only where a value starts, so a word's `'` is text.
struct Strings {
    flow: super::flow_syntax::Flow,
    /// The characters a value starts after: the last of each key
    /// separator, the item separator and the brackets' openers.
    value_starts: Vec<char>,
}

impl Strings {
    fn of(config: &LanguageConfig) -> Self {
        let value_starts = config
            .separators
            .iter()
            .filter_map(|separator| separator.chars().next_back())
            .chain(config.item_separator)
            .chain(config.brackets.iter().map(|(open, _)| *open))
            .collect();
        Strings {
            flow: super::flow_syntax::Flow::from_config(*config),
            value_starts,
        }
    }

    /// Calls `on` with each character of `text` outside strings (and its
    /// byte offset); returns whether `text` ends inside one.
    fn walk(&self, text: &str, mut on: impl FnMut(usize, char)) -> bool {
        let mut quote: Option<(char, bool)> = None;
        let mut escaped = false;
        let mut previous = ' ';
        for (index, character) in text.char_indices() {
            match quote {
                Some(_) if escaped => escaped = false,
                Some((_, true)) if character == '\\' => escaped = true,
                Some((open, _)) if character == open => quote = None,
                Some(_) => {}
                None => match self.flow.quote(character) {
                    Some(true) => quote = Some((character, true)),
                    Some(false)
                        if previous.is_whitespace() || self.value_starts.contains(&previous) =>
                    {
                        quote = Some((character, false));
                    }
                    _ => on(index, character),
                },
            }
            previous = character;
        }
        quote.is_some()
    }

    /// Whether the end of `text` (before the caret) is inside a string.
    fn ends_inside(&self, text: &str) -> bool {
        self.walk(text, |_, _| {})
    }
}

/// Whether a bracket of `pairs` opened in `line` is still open at its end,
/// skipping quoted text.
fn opened_on_line(strings: &Strings, line: &str, pairs: &[(char, char)]) -> bool {
    let mut depth = 0isize;
    strings.walk(line, |_, character| {
        if pairs.iter().any(|(open, _)| *open == character) {
            depth += 1;
        } else if pairs.iter().any(|(_, close)| *close == character) {
            depth -= 1;
        }
    });
    depth > 0
}

fn line_indent(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// The start of the line holding the innermost `opener` still open at
/// `byte`, skipping strings.
fn opener_line(
    strings: &Strings,
    source: &str,
    byte: usize,
    opener: char,
    closer: char,
) -> Option<usize> {
    let mut open = Vec::new();
    strings.walk(&source[..byte], |index, character| {
        if character == opener {
            open.push(index);
        } else if character == closer {
            open.pop();
        }
    });
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
        let line_end = source[close_end..]
            .find('\n')
            .map_or(source.len(), |offset| close_end + offset);
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
    // Off the lines while a batch runs for the text (many places asked at
    // once each counted the text up to theirs).
    Some(InlineBlock {
        start: super::paths::units_at(source, block_start),
        end: super::paths::units_at(source, block_end),
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
    /// A dotted head typed before the caret (`websocket.` of
    /// `websocket.ho|path`): the entry after the caret keeps it when its key
    /// is one under that head (it was `websocket.path`), else it was typed
    /// for the new entry only (`tls.|path` leaves `path` as it was).
    pub suffix_head: String,
    /// How many keys `suffix_head` names (1 for `websocket.`).
    pub suffix_head_depth: usize,
}

/// An edit of a text: the bytes it replaces, and their new text.
pub type KeyEdit = (std::ops::Range<usize>, String);

/// `source` with `edits` (apart from each other) made.
pub(crate) fn with_edits(source: &str, mut edits: Vec<KeyEdit>) -> String {
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut edited = source.to_string();
    for (range, text) in edits {
        edited.replace_range(range, &text);
    }
    edited
}

/// Splits a JSON pointer into unescaped segments, each verbatim (RFC 6901:
/// `~1` is `/`, then `~0` is `~`).
pub fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// The array index an unescaped pointer segment names, as RFC 6901 reads
/// it: `0`, or digits without a leading zero (`01`, `+1` and ` 1` are keys).
/// It indexes only where the value is an array; anywhere else it is a key.
pub fn pointer_index(segment: &str) -> Option<usize> {
    let canonical = !segment.is_empty()
        && segment.bytes().all(|byte| byte.is_ascii_digit())
        && (segment == "0" || !segment.starts_with('0'));
    canonical.then(|| segment.parse().ok()).flatten()
}

impl SchemaPathSegment {
    /// One unescaped JSON pointer segment: an index when it is written as
    /// one ([`pointer_index`]), else a key.
    pub fn from_pointer_segment(segment: String) -> Self {
        match pointer_index(&segment) {
            Some(index) => SchemaPathSegment::ArrayItem(index),
            None => SchemaPathSegment::Key(segment),
        }
    }

    /// The segments of a JSON pointer.
    pub fn from_pointer(pointer: &str) -> Vec<Self> {
        pointer_segments(pointer)
            .into_iter()
            .map(Self::from_pointer_segment)
            .collect()
    }

    /// The array index this step names in an array: an index, or a key
    /// written as one (a pointer cannot tell them apart).
    pub fn as_index(&self) -> Option<usize> {
        match self {
            SchemaPathSegment::ArrayItem(index) => Some(*index),
            SchemaPathSegment::Key(key) => pointer_index(key),
        }
    }

    /// The key this step names in an object: a key, or an index's digits.
    pub fn as_key(&self) -> std::borrow::Cow<'_, str> {
        match self {
            SchemaPathSegment::Key(key) => std::borrow::Cow::Borrowed(key),
            SchemaPathSegment::ArrayItem(index) => std::borrow::Cow::Owned(index.to_string()),
        }
    }
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
/// A flat key/value language (one `KEY=value` entry per line, no nesting)
/// gets schema completion and hover from its separators too, by delegating
/// those hooks to the `flat_*` methods
/// ([`flat_completion_context`](Self::flat_completion_context),
/// [`flat_existing_keys`](Self::flat_existing_keys),
/// [`flat_property_insert`](Self::flat_property_insert),
/// [`flat_hover_key`](Self::flat_hover_key),
/// [`flat_format_value`](Self::flat_format_value)).
///
/// ```
/// use config_editor_leptos::language::LanguageConfig;
///
/// const INI: LanguageConfig = LanguageConfig {
///     line_comments: &[";", "#"],
///     block_comment: None,
///     brackets: &[],
///     separators: &["="],
///     quotes: &["\""],
///     literal_quotes: &[],
///     map_brackets: None,
///     list_brackets: None,
///     item_separator: None,
///     item_marker: None,
///     written_separator: None,
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
    /// Bracket pairs: they auto-close as typed (only these do), Enter
    /// between them opens an indented line, and a closer typed on its own
    /// line aligns with its opener.
    pub brackets: &'static [(char, char)],
    /// What separates a key from its value (`":"`, `"="`): completing a key
    /// that already has a value renames only the key, a space after a complete
    /// value ends the entry, and `Tab` walks values left empty after one.
    pub separators: &'static [&'static str],
    /// String delimiters, longest first (`"\"\"\""` before `"\""`): `Ctrl+/`
    /// takes a string written over lines whole, never commenting part of it.
    /// Comment markers and brackets inside them are text. The ones of one
    /// character auto-close as typed (only these quotes do).
    pub quotes: &'static [&'static str],
    /// The quotes inside which a backslash is text, not an escape (TOML's
    /// and YAML's `'`); in the others it escapes the next character.
    pub literal_quotes: &'static [&'static str],
    /// The brackets around a map written as one value (`('{', '}')`), if
    /// the language writes maps so: an Edit-as save edits such a map where
    /// it is, its entries kept as written.
    pub map_brackets: Option<(char, char)>,
    /// The brackets around a list written as one value (`('[', ']')`), if
    /// the language writes lists so, edited in place as maps are.
    pub list_brackets: Option<(char, char)>,
    /// What separates the items and entries between those brackets
    /// (`','`).
    pub item_separator: Option<char>,
    /// How a new entry writes the separator after its key, with its
    /// spacing (`": "`, `" = "`). Default: the first of `separators` as it
    /// is (`KEY=`).
    pub written_separator: Option<&'static str>,
    /// What starts a list item written on lines of its own (YAML's `-`),
    /// followed by a space or the line's end: an Edit-as save moves and
    /// indents such items with their lines.
    pub item_marker: Option<&'static str>,
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
    /// Strings are built with [`string_value`](crate::string_value), and
    /// infinities and NaN with [`float_value`](crate::float_value), so the
    /// two never read as each other.
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

    /// Whether the language has numbers for infinity and NaN, which
    /// [`serialize`](Self::serialize) then writes from the values
    /// [`float_value`](crate::float_value) makes. Without them (the default)
    /// they reach `serialize` as the text `inf`, `-inf` or `nan`, and a
    /// conversion reports them as values the language cannot hold.
    fn writes_non_finite(&self) -> bool {
        false
    }

    /// Whether the language has one number type, so `2` and `2.0` are the
    /// same value (HCL), even when [`parse`](Self::parse) keeps how a number
    /// was written. An integer typed over a float the host holds then keeps
    /// the host's float type: the view cannot say otherwise.
    fn one_number_type(&self) -> bool {
        false
    }

    /// Whether indentation is structure (YAML: a key's indent says which
    /// block it belongs to). A read-only field's lines then keep their
    /// indent; where it is not (the default), re-indenting them stays free.
    fn indent_is_structure(&self) -> bool {
        false
    }

    /// Whether values may be written unquoted to the end of their line
    /// (YAML plain scalars), so any character typed right next to one,
    /// punctuation included, joins it. Default: no, punctuation ends a value.
    fn plain_scalars(&self) -> bool {
        false
    }

    /// Whether every scalar is written as text, its type only guessed on
    /// reading (XML: `<v>1</v>` reads as a number, `<v></v>` as an empty
    /// element). A value edited while editing as such a language keeps the
    /// host's type: a string stays a string, and an emptied value is the
    /// empty string, or waits where the host holds a number or a boolean.
    /// Default: no, the language writes each type its own way.
    fn scalars_are_text(&self) -> bool {
        false
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

    /// [`key_span`](Self::key_span) for each of `pointers`. A language
    /// whose lookup reads the whole document overrides it to read it once
    /// (problems by the thousand are placed with it).
    fn key_spans(&self, source: &str, pointers: &[String]) -> Vec<Option<Span>> {
        pointers
            .iter()
            .map(|pointer| self.key_span(source, pointer))
            .collect()
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

    /// `value` written over `source[range]`, a value the host wrote there,
    /// as [`format_value_like`](Self::format_value_like) does; the text before
    /// it on its line places it (a YAML block scalar's indentation indicator
    /// counts from its key or dash).
    fn format_value_at(
        &self,
        value: &Value,
        source: &str,
        range: std::ops::Range<usize>,
    ) -> String {
        self.format_value_like(value, &source[range])
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

    /// When character `column` of `line` (starting at byte `line_start` of
    /// `source`) opens a list item, the byte offset in `line` where the
    /// item's content starts; the hover card then shows the list's docs and
    /// the item's. The default: one of the language's opening brackets where
    /// an item goes (a JSON item's `{`). A language with its own item marker
    /// (a YAML `- `) reports that too.
    fn item_start(
        &self,
        source: &str,
        line_start: usize,
        line: &str,
        column: usize,
    ) -> Option<usize> {
        bracket_item_start(self, source, line_start, line, column)
    }

    /// The highlighted runs of one line; uncovered text renders plain.
    fn highlight(&self, line: &str) -> Vec<Token> {
        let _ = line;
        Vec::new()
    }

    /// What Enter writes at `cursor` (UTF-16). The default keeps the line's
    /// indentation and opens the [`LanguageConfig::brackets`] it is between.
    fn newline(&self, source: &str, cursor: usize) -> Newline {
        let config = self.config();
        bracket_newline(source, cursor, config.brackets, &config, |line| {
            &line[..self.comment_start(line).unwrap_or(line.len())]
        })
    }

    /// The UTF-16 selection `start..end` widened over whole lines and the
    /// constructs it would cut through (strings, brackets, elements over
    /// lines, and what [`comment_scope`](Self::comment_scope) adds): what
    /// Ctrl+/ comments, and what copy or cut on a folded line takes. The
    /// default widens over strings and the [`LanguageConfig::brackets`].
    fn whole_constructs(&self, source: &str, start: usize, end: usize) -> (usize, usize) {
        let (mut start, mut end) = (start, end);
        // Whole strings and constructs over lines, until both hold.
        loop {
            let widened = over_whole_strings(self, source, start, end);
            let widened =
                super::comment_scope::over_balanced_lines(self, source, widened.0, widened.1);
            let widened = self.comment_scope(source, widened.0, widened.1);
            if widened == (start, end) {
                return widened;
            }
            (start, end) = widened;
        }
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
        // Inside a block comment on lines of its own: its markers go.
        if let Some(block) = config.block_comment
            && let Some(unwrapped) =
                unwrap_block_comments(source, selection_start, selection_end, block)
        {
            return Some(unwrapped);
        }
        let (selection_start, selection_end) =
            self.whole_constructs(source, selection_start, selection_end);
        config
            .block_comment
            .and_then(|block| unwrap_block_comments(source, selection_start, selection_end, block))
            .or_else(|| {
                toggle_line_comments(source, selection_start, selection_end, config.line_comments)
            })
    }

    /// Why `source` does not convert to another language although it parses
    /// (XML elements of one parent with the same local name in different
    /// namespaces, which the document model would merge), ending with
    /// `(at /pointer)`; `None` when nothing stands in the way.
    fn unconvertible(&self, source: &str) -> Option<String> {
        let _ = source;
        None
    }

    /// Why the data model reads `source` with keys merged that it writes
    /// apart (XML elements of one parent with the same local name in
    /// different namespaces), ending with `(at /pointer)`: parsing refuses
    /// such a document, as converting it does. Default: `None`.
    fn merged_keys(&self, source: &str) -> Option<String> {
        let _ = source;
        None
    }

    /// The property a schema's `required` entry `name` names, as the
    /// language spells it (XML lists a required attribute `a` as `@a`).
    /// Default: `name` itself.
    fn required_property<'a>(&self, name: &'a str) -> &'a str {
        name
    }

    /// The key the language reads text written beside child entries under
    /// (XML's `#text`, for mixed content): a schema that does not allow it
    /// is told as text not allowed there. Default: none.
    fn text_key(&self) -> Option<&'static str> {
        None
    }

    /// The JSON pointer of the first value `source` writes as text beside
    /// child elements (XML mixed content): no other language can show it, so
    /// the document does not convert. `None` when there is none.
    fn mixed_content(&self, source: &str) -> Option<String> {
        let _ = source;
        None
    }

    /// The JSON pointer of each value `source` writes as an expression (an
    /// HCL reference `var.z`, or template `"${var.x}-y"`): the data holds
    /// its text, which another language reads as plain text (as the text a
    /// literal `"$${x}"` reads as), so a conversion lists it as a loss.
    /// Empty by default.
    fn expressions(&self, source: &str) -> Vec<String> {
        let _ = source;
        Vec::new()
    }

    /// The JSON pointer of each value `source` writes with a tag (YAML
    /// `!Sub x`, `!include`), with the tag: the data holds the value under
    /// it, and an Edit-as save keeps each one or waits. Empty by default.
    fn tags(&self, source: &str) -> Vec<(String, String)> {
        let _ = source;
        Vec::new()
    }

    /// `previous` with what it writes once and reads in several places (YAML
    /// aliases and merge keys) written out in place wherever `new` gives that
    /// place a value of its own, so a save can patch the rest; `None` when
    /// nothing needs it. The text must read as `old` still.
    fn detach_shared(&self, previous: &str, old: &Value, new: &Value) -> Option<String> {
        let _ = (previous, old, new);
        None
    }

    /// Where the field at `pointer` is written when `source` writes it once
    /// and reads it in several places: the pointer under a YAML anchor
    /// that an alias (`c: *x` reads `/c/b` from `/a/b` of `a: &x`) or a
    /// merge (`<<: *x`) reads it from. `None` (the default) where it reads
    /// through nothing so written. `rangeOf` finds such a field there.
    fn written_at(&self, source: &str, pointer: &str) -> Option<String> {
        let _ = (source, pointer);
        None
    }

    /// `code` (a line without its comment, trimmed) as an Edit-as save
    /// compares it with the serializer's line for the same entry: written
    /// forms that read the same are made equal (YAML's `-   a` and `- a`,
    /// `key: &anchor`, `key: ~`). The default keeps it.
    fn comparable_line<'a>(&self, code: &'a str) -> std::borrow::Cow<'a, str> {
        std::borrow::Cow::Borrowed(code)
    }

    /// The length of what may precede a value without being part of it
    /// (YAML's `&anchor `, `!!tag `), so a flow collection after it is found.
    fn value_properties_len(&self, text: &str) -> usize {
        let _ = text;
        0
    }

    /// The length of a value written as a reference to another (YAML's
    /// `*anchor`) at the start of `text`, which a save keeps as written
    /// while the value it stands for is unchanged. Default: none.
    fn reference_len(&self, text: &str) -> Option<usize> {
        let _ = text;
        None
    }

    /// Whether `code` (a line without its comment, trimmed) opens a map
    /// written inline over the following lines (YAML's `tcp: {`), which a
    /// save keeps as written rather than the serializer's form. Default: no.
    fn flow_map_opener(&self, code: &str) -> bool {
        let _ = code;
        false
    }

    /// Whether `written` (a scalar as the language writes one in a flow
    /// collection) reads as `value`, however it is spelled (`"caf\u00e9"`
    /// for `"café"`). Default: never, so the value is written again.
    fn scalar_reads_as(&self, written: &str, value: &Value) -> bool {
        let _ = (written, value);
        false
    }

    /// `value` written in the form of `written`, the number it replaces
    /// (its radix, digit separators, exponent and fraction digits:
    /// `0x1F90` edited to 8083 is `0x1F93`), when the language has that
    /// form, `value` fits it and the text reads back as `value`. A save
    /// from another view writes a changed number so. Default: `None`, the
    /// writer's own form.
    fn number_in_form(&self, written: &str, value: &serde_json::Number) -> Option<String> {
        let _ = (written, value);
        None
    }

    /// The bytes of `source` a scalar's written form occupies, from the
    /// byte `range` of its span: XML element text runs from its start tag to
    /// its end tag, since the whitespace around it is part of the value.
    /// Default: `range`.
    fn written_value_range(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
    ) -> std::ops::Range<usize> {
        let _ = source;
        range
    }

    /// Where a string value that ends at byte `end` of `source` really ends
    /// for keeping it as written: XML text runs through the line break
    /// before its end tag. Default: `end`.
    fn kept_value_end(&self, source: &str, end: usize) -> usize {
        let _ = source;
        end
    }

    /// The UTF-16 selection Ctrl+/ comments, widened over what the selected
    /// lines own that would be left broken (a YAML key's indented children).
    /// The default keeps it.
    fn comment_scope(&self, source: &str, start: usize, end: usize) -> (usize, usize) {
        let _ = source;
        (start, end)
    }

    /// Text to insert after the caret when `typed` is typed at `cursor`
    /// (UTF-16): the end tag XML writes when `>` finishes `<bob>`, or the
    /// closing `"""` of a TOML multi-line string. Checked before the shared
    /// bracket and quote pairing; `Some("")` writes `typed` alone, unpaired
    /// (a quote in XML element text is an apostrophe).
    fn auto_close(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        let _ = (source, cursor, typed);
        None
    }

    /// Text that finishes what `typed`, typed at `cursor` (UTF-16), begins,
    /// written after it with the caret moving past it: the end tag XML
    /// writes when `/` follows a `<` (`<a>x</` → `<a>x</a>`). The line then
    /// re-indents as [`reindent_on_type`](Self::reindent_on_type) says for
    /// its last character.
    fn complete_on_type(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
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

    /// Whether the field at `pointer` is written as one of what a list of
    /// it repeats, written once (an XML element, an HCL block): it is then
    /// also that list's only item, so `pointer/0` finds it. Never for an
    /// index, an attribute, an object of labelled blocks or the document.
    /// Only a language with [`Self::repetition`] has any.
    fn lone_item(&self, source: &str, pointer: &str) -> bool {
        let _ = (source, pointer);
        false
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

    /// The lines of strings written over lines that a save keeps as written
    /// (see [`string_body_lines`](Self::string_body_lines)): those, and in
    /// YAML a plain scalar's continuation lines, which the editor leaves to
    /// be typed as keys.
    fn saved_string_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        string_bodies(self, source).to_vec()
    }

    /// Lines (0-based, end exclusive) that lie wholly inside the body of a
    /// multi-line string (TOML `"""`, an HCL heredoc, a YAML block scalar).
    /// They are text: Enter keeps their indentation, nothing is paired,
    /// aligned or suggested there, and they are colored as a string.
    fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        let _ = source;
        Vec::new()
    }

    /// Byte ranges of the comments in `source`, in order; an unclosed one
    /// runs to the end. The default finds line comments
    /// ([`comment_start`](ConfigLanguage::comment_start)) on lines outside
    /// multi-line string bodies; a language with comments over several
    /// lines (XML `<!-- … -->`, HCL `/* … */`) reads them whole.
    ///
    /// [`ConfigLanguage::highlight`] reads one line at a time, so the lines
    /// after a comment's first are colored from these (a comment up to its
    /// end, the rest of its last line by `highlight`), and a secret
    /// placeholder written in a comment is told from the real one without
    /// looking up its path.
    fn comments(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        line_comments(self, source)
    }

    /// Byte ranges of the strings `source` writes over several lines, in
    /// order, for languages that do not describe them line by line with
    /// [`string_body_lines`](Self::string_body_lines). As with
    /// [`comments`](Self::comments), the lines after a string's first are
    /// colored as a string up to its end, and the rest of its last line by
    /// [`highlight`](Self::highlight). Default: none.
    fn multiline_strings(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        let _ = source;
        Vec::new()
    }

    /// The constructs `source` writes over several lines, other than
    /// [comments](Self::comments), in order: each line of one past its first
    /// is colored as what it is, up to the construct's end, the rest of its
    /// last line by [`highlight`](Self::highlight). A
    /// [`ConstructKind::Colored`] one is one kind of token (a TOML `"""`
    /// string, a YAML plain scalar's continuation lines); a
    /// [`ConstructKind::Structure`] one (a TOML array, a YAML flow
    /// collection, an XML start tag with attributes over lines) is colored
    /// by [`highlight_continued`](Self::highlight_continued), with the
    /// comments and strings inside it as those. Default: the
    /// [`multiline_strings`](Self::multiline_strings), colored as strings.
    fn multiline_constructs(&self, source: &str) -> Vec<MultilineConstruct> {
        self.multiline_strings(source)
            .into_iter()
            .map(|range| MultilineConstruct::colored(range, TokenKind::String))
            .collect()
    }

    /// The tokens of `text`, a part of a line inside a
    /// [structure construct](ConstructKind::Structure) opened on an earlier
    /// line (the inside of a TOML array, of an XML start tag), up to the
    /// construct's end. A string left open colors to the end of `text`.
    /// Default: [`highlight`](Self::highlight).
    fn highlight_continued(&self, text: &str) -> Vec<Token> {
        self.highlight(text)
    }

    /// The line ranges that fold (see [`FoldRange`]), sorted by start with
    /// one range per start line (see [`merge_fold_ranges`]): the gutter
    /// shows a chevron on each start line, and `Ctrl+Shift+[` folds the
    /// innermost one around the caret. A range that opens and closes on one
    /// line folds nothing and is left out.
    ///
    /// The default folds by indentation, by the brackets and block comments
    /// of [`config`](Self::config), runs of line comments, and the
    /// [multi-line strings](Self::string_body_lines) (see
    /// [`default_fold_ranges`]). Languages override it with their own
    /// structure, combining the same building blocks.
    ///
    /// Only the `folding` feature asks; without it the default finds
    /// nothing, so a build without folding carries none of its code.
    fn fold_ranges(&self, source: &str) -> Vec<FoldRange> {
        if !cfg!(feature = "folding") {
            return Vec::new();
        }
        default_fold_ranges(source, &self.config(), &string_bodies(self, source))
    }

    /// The byte offset where the trailing comment of `line` starts, outside
    /// strings. Edit-as uses it to match lines that differ only in comments.
    /// See [`comment_start_outside_quotes`].
    fn comment_start(&self, line: &str) -> Option<usize> {
        let config = self.config();
        comment_start_outside_quotes(line, config.line_comments, config.quotes)
    }

    /// Whether `source` holds a processing instruction other than an XML
    /// declaration (`<?xml-stylesheet …?>`), which [`has_comments`]
    /// counts and the Edit-as notice names. Default: none.
    ///
    /// [`has_comments`]: ConfigLanguage::has_comments
    fn has_processing_instructions(&self, source: &str) -> bool {
        let _ = source;
        false
    }

    /// Whether `source` holds a comment: a line comment
    /// ([`comment_start`](ConfigLanguage::comment_start)) or the start of a
    /// block comment, outside strings and multi-line string bodies. The
    /// Edit-as notice reads it: the data a conversion carries has none.
    fn has_comments(&self, source: &str) -> bool {
        let bodies = string_bodies(self, source);
        let quotes = self.config().quotes;
        let block = self.config().block_comment.map(|(open, _)| [open]);
        source.lines().enumerate().any(|(index, line)| {
            !bodies.iter().any(|lines| lines.contains(&index))
                && (self.comment_start(line).is_some()
                    || block.is_some_and(|open| {
                        comment_start_outside_quotes(line, &open, quotes).is_some()
                    }))
        })
    }

    /// Whether `rest`, the text after a key being completed, shows the key
    /// already has its value (`: 99`, ` = "x"`, an HCL block's `{`) or goes
    /// on (a dotted key's `.path`), so accepting a suggestion renames only
    /// the key.
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
        value_after_separator(line_before, &self.config())
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

    /// The characters the language's markup is written between (XML's `<`
    /// and `>`), if it has markup: a lock lets an edit that leaves the
    /// text unparsed add the one that ends markup a line left open.
    /// Default: no markup.
    fn markup_delimiters(&self) -> Option<(char, char)> {
        None
    }

    /// The length in bytes of markup at the start of `text` that opens an
    /// element whose name, a key, follows it directly (XML's `<` before a
    /// start tag's name). The markup belongs to the key's field: a lock
    /// covers it, and the caret on it is on the key. Default: none.
    fn key_markup_len(&self, text: &str) -> Option<usize> {
        let _ = text;
        None
    }

    /// The byte just past the element that opens at byte `open` of `text`
    /// (an XML element from its `<` through its end tag or `/>`), for
    /// moving a repeated element with all its lines; `None` when nothing
    /// opens there or it does not end. Default: none.
    fn element_end(&self, text: &str, open: usize) -> Option<usize> {
        let _ = (text, open);
        None
    }

    /// How many containers `text` opens and leaves open: by default its
    /// [`LanguageConfig::brackets`] outside its quotes; XML counts the
    /// elements it starts and does not end.
    fn unclosed_containers(&self, text: &str) -> usize {
        super::flow_syntax::Flow::of(self).unclosed(text, self.config().brackets)
    }

    /// The length of what closes a container at the start of `text` (a
    /// closing bracket; XML's element text then its end tag), or `None`.
    fn container_closer_len(&self, text: &str) -> Option<usize> {
        self.config()
            .brackets
            .iter()
            .find(|(_, close)| text.starts_with(*close))
            .map(|(_, close)| close.len_utf8())
    }

    /// Byte ranges of `text` written as a value's content outside strings
    /// (XML element text holding more than whitespace): its whitespace is
    /// the value's, never formatting, so a lock keeps it. Default: none.
    fn content_texts(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        let _ = text;
        Vec::new()
    }

    /// Whether the whitespace between `before` (the text up to it) and
    /// `after` (the text from its end) is part of the document's content
    /// rather than spacing between tokens: XML text's own space at a tag's
    /// edge, or space that would part a tag's name from its `<`. Default:
    /// no, whitespace between tokens is free.
    fn space_is_content(&self, before: &str, after: &str) -> bool {
        let _ = (before, after);
        false
    }

    /// Whether `line` may hold a whole container written on one line in a
    /// form other than map or list brackets (an XML element with its
    /// children, `<a><b>1</b></a>`), which [`one_line_container`] then
    /// locates. A cheap check: a save looks up only the containers on such
    /// lines. Default: no.
    ///
    /// [`one_line_container`]: ConfigLanguage::one_line_container
    fn writes_container_on_line(&self, line: &str) -> bool {
        let _ = line;
        false
    }

    /// Whether `line` starts a section that runs to the next one, whatever
    /// its indent (TOML's `[table]` header). Default: no sections.
    fn starts_section(&self, line: &str) -> bool {
        let _ = line;
        false
    }

    /// Whether `code` (a line without its comment, trimmed) marks the
    /// document's start or describes the document rather than holding a
    /// value (YAML's `---` and `%YAML` directives). Default: no such lines.
    fn document_marker(&self, code: &str) -> bool {
        let _ = code;
        false
    }

    /// Whether line `index` of `lines` starts an item of a list written as
    /// lines of its own: the comments right above it are the item's, never
    /// the file's header. Default: a line starting with the
    /// [`LanguageConfig::item_marker`]; TOML adds `[[array]]` tables, HCL
    /// labelled and repeated blocks, XML repeated elements.
    fn opens_list_item(&self, lines: &[&str], index: usize) -> bool {
        let line = lines[index];
        let code = &line[..self.comment_start(line).unwrap_or(line.len())];
        super::flow_syntax::Flow::of(self)
            .after_item_marker(code.trim())
            .is_some()
    }

    /// The bytes of the list item at `pointer` when the document writes it
    /// as lines of its own that are no `{…}` or `[…]` value: a TOML
    /// `[[array]]` table from its header, an HCL repeated block from its
    /// type to its `}`. An Edit-as save that moves the item moves these
    /// lines with the comment lines above them. `None` (the default) for
    /// any other item.
    fn item_lines(&self, source: &str, pointer: &str) -> Option<std::ops::Range<usize>> {
        let _ = (source, pointer);
        None
    }

    /// `read`, the value `source` reads as, with its keys spelled as
    /// `like` spells them, where the language reads one key two ways: XML
    /// reads an attribute `@a` beside a child element `a`, and `a` alone.
    /// A save then compares the two key by key (an attribute stays one
    /// when the child is removed, and a child added back beside it is a
    /// child). Default: `read`.
    fn spelled_like(&self, source: &str, read: Value, like: &Value) -> Value {
        let _ = (source, like);
        read
    }

    /// Whether the map at `pointer` is written in `source` as the labels
    /// of blocks (HCL's `svc "a" { … }` holds `/svc/a`, so `/svc` is such a
    /// map): emptied, it is written as no blocks, not as an empty block.
    /// Default: no.
    fn writes_as_labels(&self, source: &str, pointer: &str) -> bool {
        let _ = (source, pointer);
        false
    }

    /// The container at `pointer` when the document writes it on one line in
    /// a form other than a `{…}` or `[…]` value: an XML element with its
    /// children on one line, an HCL one-line block. A structural Edit-as
    /// save keeps such a container as written.
    fn one_line_container(&self, source: &str, pointer: &str) -> Option<OneLine> {
        let _ = (source, pointer);
        None
    }

    /// The edits renaming the key at `pointer` everywhere the language
    /// writes it (an XML element's start and end tags, a TOML key's headers
    /// and dotted keys): byte ranges of `source`, apart from each other,
    /// each with its new text; `None` to rename just the text at
    /// [`key_span`](Self::key_span). `new_key` is the new key as
    /// [`written_key`](Self::written_key) writes it; a key the host wrote
    /// in quotes keeps them. A save renaming many keys asks for each in one
    /// batch over `source` (see `paths::with_spans`): what the language
    /// reads of all of it to find them is read once
    /// (`paths::batch_cached`).
    fn key_renames(&self, source: &str, pointer: &str, new_key: &str) -> Option<Vec<KeyEdit>> {
        let _ = (source, pointer, new_key);
        None
    }

    /// `key` as the language writes it in place of the key at `pointer` of
    /// `source` (a rename): bare, or quoted and escaped where it must be
    /// (YAML quotes `on` and `1_000`, which YAML 1.1 reads as other types).
    /// `None` when the language cannot write it there (an HCL block type is
    /// an identifier): a rename then takes the other save paths, which
    /// refuse what the language cannot hold. By default the key as the
    /// language's own [`serialize`](Self::serialize) writes it, found by
    /// [`key_span`](Self::key_span) in a document holding only that key
    /// and read back by [`parse`](Self::parse); `None` without them.
    fn written_key(&self, source: &str, pointer: &str, key: &str) -> Option<String> {
        let _ = (source, pointer);
        key_as_written(self, key)
    }

    /// The value a save writes at `pointer` of `previous` for `value` (a
    /// changed scalar): `Ok(None)` keeps `value`; `Ok(Some)` is the
    /// language's own form of it where its reader normalizes what it reads
    /// (TOML reads the datetime `…00.50Z` as `…00.5Z`); `Err` says why the
    /// save waits (a TOML datetime made text that is no datetime, which
    /// would make it a string).
    fn scalar_edit(
        &self,
        previous: &str,
        pointer: &str,
        value: &Value,
    ) -> Result<Option<Value>, String> {
        let _ = (previous, pointer, value);
        Ok(None)
    }

    /// The pointer of the first value `patched` (a save's text for
    /// `previous`) writes in another form than `previous` does, where the
    /// two read the same but are different things to the language's
    /// readers: an HCL block and an object attribute (`name = { … }`), an
    /// expression (`var.x`, read as `${var.x}`) and a literal string. A
    /// patch that switches one is not taken, and a save that would write
    /// one waits.
    fn changed_form(&self, previous: &str, patched: &str) -> Option<String> {
        let _ = (previous, patched);
        None
    }

    /// Drops what the language keeps of the last texts it read (scans,
    /// spans), once no editor is mounted: nothing of a closed editor's text
    /// stays. Nothing by default.
    fn forget_kept(&self) {}

    /// The text for the `value` range of a [`OneLine`] now holding `value`
    /// (`{ nodelay = true }`), so a save edits the container where it is.
    /// When one line cannot hold it, the text may span lines, indented by
    /// `unit` from the container's line indent `indent`.
    fn one_line_body(&self, value: &Value, indent: &str, unit: &str) -> Option<String> {
        let _ = (value, indent, unit);
        None
    }

    /// Whether replacing the bytes `range` of `source` (a text that parses)
    /// with `inserted` changes one scalar value written on that line and
    /// nothing else: the text parses afterwards as it did, every other value
    /// reads as before, and nothing reads that one elsewhere (a YAML alias of
    /// an anchored node). A lock off that line then holds without reading
    /// the document again, so a lock costs a keystroke away from it nothing.
    /// `false` when unsure (the default: the document is read).
    fn edits_one_scalar(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
        inserted: &str,
    ) -> bool {
        let _ = (source, range, inserted);
        false
    }
}

/// `key` as `language` writes it (see [`ConfigLanguage::written_key`]): the
/// text its serializer writes for it in a document holding only that key,
/// when that document reads back with the key.
fn key_as_written(language: &(impl ConfigLanguage + ?Sized), key: &str) -> Option<String> {
    let pointer = format!("/{}", key.replace('~', "~0").replace('/', "~1"));
    [Value::from(0), Value::from("")]
        .into_iter()
        .find_map(|value| {
            let document = Value::Object([(key.to_string(), value)].into_iter().collect());
            let text = language.serialize(&document, None).ok()?;
            let read = language.parse(&text).ok()?;
            if read.as_object()?.keys().ne([key]) {
                return None;
            }
            let span = language.key_span(&text, &pointer)?;
            let line = text.split('\n').nth(span.line.checked_sub(1)?)?;
            let written: String = line
                .chars()
                .skip(span.start_column.checked_sub(1)?)
                .take(span.end_column.checked_sub(span.start_column)?)
                .collect();
            (!written.is_empty()).then_some(written)
        })
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

/// The line comments of `source` (see [`ConfigLanguage::comments`]).
pub(crate) fn line_comments(
    behavior: &(impl ConfigLanguage + ?Sized),
    source: &str,
) -> Vec<std::ops::Range<usize>> {
    let lines = source.split('\n').count();
    let mut body = vec![false; lines];
    for range in string_bodies(behavior, source).iter().cloned() {
        let end = range.end.min(lines);
        if range.start < end {
            body[range.start..end].fill(true);
        }
    }
    let mut comments = Vec::new();
    let mut line_start = 0;
    for (index, line) in source.split('\n').enumerate() {
        if !body[index]
            && let Some(start) = behavior.comment_start(line)
        {
            comments.push(line_start + start..line_start + line.len());
        }
        line_start += line.len() + 1;
    }
    comments
}

/// The byte offset of the first of `markers` on `line` that starts a comment:
/// outside strings in `quotes` (a backslash before a closing quote escapes
/// it), at the line start or after whitespace. A building block for
/// [`ConfigLanguage::comment_start`].
pub fn comment_start_outside_quotes(
    line: &str,
    markers: &[&str],
    quotes: &[&str],
) -> Option<usize> {
    let opens = |character: char| quotes.iter().any(|quote| quote.starts_with(character));
    let mut quote: Option<char> = None;
    let mut previous = ' ';
    for (index, character) in line.char_indices() {
        match quote {
            Some(open) if character == open && previous != '\\' => quote = None,
            Some(_) => {}
            None if opens(character) => quote = Some(character),
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

/// Whether `line_before` ends with a value after its last key separator
/// (`key = false`, not `key = `, `key = [` or `key = 1,`), as `config`
/// describes separators, brackets and the item separator. A building block
/// for [`ConfigLanguage::line_has_value`].
pub fn value_after_separator(line_before: &str, config: &LanguageConfig) -> bool {
    let separators: Vec<char> = config
        .separators
        .iter()
        .filter_map(|separator| separator.chars().next())
        .collect();
    if separators.is_empty() {
        return false;
    }
    let value = line_before
        .rsplit_once(separators.as_slice())
        .map_or("", |(_, value)| value)
        .trim();
    let opens = |character: char| {
        config.item_separator == Some(character)
            || config.brackets.iter().any(|(open, _)| *open == character)
            || separators.contains(&character)
    };
    !value.is_empty() && !value.chars().next_back().is_some_and(opens)
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
    let label = behavior.info().label;
    let kept = super::paths::batch_kept(source, || StringLines::new(behavior, source));
    let fresh;
    let strings = if kept.label == label {
        &*kept
    } else {
        fresh = StringLines::new(behavior, source);
        &fresh
    };
    let inside = &strings.inside;
    let line_count = inside.len();
    // A closing delimiter alone after a body: one of the language's
    // multi-character quotes, or a word (a heredoc's `EOT`).
    let closes = |line: usize| {
        super::paths::split_line(source, line).is_some_and(|text| {
            let trimmed = text.trim();
            behavior
                .config()
                .quotes
                .iter()
                .any(|quote| quote.len() > 1 && trimmed.contains(quote))
                || (!trimmed.is_empty()
                    && trimmed
                        .chars()
                        .all(|character| character.is_alphanumeric() || character == '_'))
        })
    };
    let line_of =
        |utf16: usize| super::paths::line_of_byte(source, utf16_to_byte_index(source, utf16));
    let (first_line, last_line) = (line_of(start), line_of(end));
    let (mut first, mut last) = (first_line, last_line);
    // On a closing delimiter (`EOF`): its body and opening line too.
    if first > 0 && inside[first - 1] && closes(first) {
        first -= 1;
    }
    while first > 0 && inside[first] {
        first -= 1;
    }
    while last + 1 < line_count && inside[last + 1] {
        last += 1;
    }
    if inside[last] && last + 1 < line_count && closes(last + 1) {
        last += 1;
    }
    // Strings the language gives whole (`multiline_strings`): every line
    // of one the lines touch, its opening and closing lines included.
    for (opens, closes) in &strings.whole {
        if *opens <= last && first <= *closes {
            first = first.min(*opens);
            last = last.max(*closes);
        }
    }
    if (first, last) == (first_line, last_line) {
        return (start, end);
    }
    let line_start = |line: usize| super::paths::line_start(source, line);
    let last_end = line_start(last) + super::paths::split_line(source, last).map_or(0, str::len);
    (
        super::paths::units_at(source, line_start(first)),
        super::paths::units_at(source, last_end),
    )
}

/// What [`over_whole_strings`] reads of a whole text, once while a batch
/// runs for it (each closed fold of a line command asks).
struct StringLines {
    label: &'static str,
    /// Whether each line is inside a string: the language's string bodies.
    /// (Quoted strings over lines are taken whole by
    /// `comment_scope::over_balanced_lines`, which reads them in one pass.)
    inside: Vec<bool>,
    /// The lines each of the language's `multiline_strings` opens and
    /// closes on.
    whole: Vec<(usize, usize)>,
}

impl StringLines {
    fn new(behavior: &(impl ConfigLanguage + ?Sized), source: &str) -> Self {
        super::work_count::note(super::work_count::Work::ConstructScan);
        let line_count = source.split('\n').count();
        let mut inside = vec![false; line_count];
        for range in string_bodies(behavior, source).iter().cloned() {
            let end = range.end.min(line_count);
            if range.start < end {
                inside[range.start..end].fill(true);
            }
        }
        // Each line's first byte, to place the strings' ends.
        let starts: Vec<usize> = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        let line_of = |byte: usize| starts.partition_point(|start| *start <= byte) - 1;
        let whole = behavior
            .multiline_strings(source)
            .into_iter()
            .map(|range| {
                (
                    line_of(range.start.min(source.len())),
                    line_of(
                        range
                            .end
                            .saturating_sub(1)
                            .max(range.start)
                            .min(source.len()),
                    ),
                )
            })
            .collect();
        Self {
            label: behavior.info().label,
            inside,
            whole,
        }
    }
}

#[cfg(test)]
mod tests {
    /// `key = value` lines, `#` comments, `'''` strings over lines, given
    /// only as `multiline_strings` (as a registered language's
    /// `multilineRanges` gives them).
    struct Tiny;

    impl super::ConfigLanguage for Tiny {
        fn info(&self) -> super::LanguageInfo {
            super::LanguageInfo {
                label: "Tiny",
                icon: "T",
                extensions: &[],
            }
        }

        fn config(&self) -> super::LanguageConfig {
            super::LanguageConfig {
                line_comments: &["#"],
                separators: &["="],
                ..super::LanguageConfig::default()
            }
        }

        fn parse(&self, _source: &str) -> Result<serde_json::Value, super::ParseError> {
            Ok(serde_json::json!({}))
        }

        fn multiline_strings(&self, source: &str) -> Vec<std::ops::Range<usize>> {
            let mut ranges = Vec::new();
            let mut from = 0;
            while let Some(open) = source[from..].find("'''") {
                let start = from + open;
                let end = source[start + 3..]
                    .find("'''")
                    .map_or(source.len(), |close| start + 3 + close + 3);
                ranges.push(start..end);
                from = end;
            }
            ranges
        }
    }

    #[test]
    fn comment_toggling_takes_a_given_multiline_string_whole() {
        use super::ConfigLanguage;
        let source = "a = 1\nname = '''multi\n# not a comment\nhost = x\nend'''\nb = 2\n";
        let commented =
            "a = 1\n# name = '''multi\n# # not a comment\n# host = x\n# end'''\nb = 2\n";
        let mut failures = Vec::new();
        for marker in ["name = '''", "# not", "host = x", "end'''"] {
            let at = source.find(marker).unwrap() + 2;
            let toggled = Tiny
                .toggle_comments(source, at, at)
                .map(|(text, _, _)| text);
            if toggled.as_deref() != Some(commented) {
                failures.push(format!("caret in {marker:?}: {toggled:?}"));
            }
        }
        let outside = source.find("b = 2").unwrap();
        let toggled = Tiny
            .toggle_comments(source, outside, outside)
            .map(|(text, _, _)| text);
        if toggled.as_deref()
            != Some("a = 1\nname = '''multi\n# not a comment\nhost = x\nend'''\n# b = 2\n")
        {
            failures.push(format!("caret outside: {toggled:?}"));
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

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
        use super::comment_start_outside_quotes;
        let start = |line, markers| comment_start_outside_quotes(line, markers, &["\"", "'"]);
        assert_eq!(start("listen = 7835 # port", &["#"]), Some(14));
        assert_eq!(start("# whole line", &["#"]), Some(0));
        assert_eq!(start("color = \"#fff\" # hex", &["#"]), Some(15));
        assert_eq!(start("url = http://x", &["#", "//"]), None);
        assert_eq!(start("a = 1 // note", &["#", "//"]), Some(6));
        // Only the language's own quotes open a string.
        assert_eq!(
            comment_start_outside_quotes("a = don't ; b", &[";"], &["\""]),
            Some(10)
        );
    }

    #[test]
    fn a_value_after_a_separator_is_complete_unless_it_opens_one() {
        use super::{LanguageConfig, value_after_separator};
        let config = |separators| LanguageConfig {
            separators,
            brackets: &[('{', '}'), ('[', ']')],
            item_separator: Some(','),
            ..LanguageConfig::default()
        };
        let has_value = |line, separators| value_after_separator(line, &config(separators));
        assert!(has_value("insecure = false", &["="]));
        assert!(!has_value("insecure =", &["="]));
        assert!(!has_value("hosts = [", &["="]));
        assert!(!has_value("pool: {a: 1,", &[":"]));
        assert!(!has_value("listen", &[":"]));
        // Only the language's own brackets open a value.
        let odd = LanguageConfig {
            separators: &[":="],
            brackets: &[('(', ')')],
            item_separator: Some('|'),
            ..LanguageConfig::default()
        };
        assert!(!value_after_separator("ports := (", &odd));
        assert!(value_after_separator("tags := [a,", &odd));
    }

    #[test]
    fn empty_slots_are_the_ends_of_lines_with_a_separator() {
        use super::empty_slots_at_line_ends as slots;
        let text = "a: \nb: 1\nc:\n";
        assert_eq!(slots(text, &[": ", ":"]), vec![3, 11]);
        assert_eq!(slots("x = \ny = 2", &[" = "]), vec![4]);
    }
}
