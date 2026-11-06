//! Adding a language: implement [`ConfigLanguage`] and wrap it with
//! [`Language::new`](crate::Language::new).
//!
//! The built-in languages implement the same trait, so everything they do is
//! available to an external language too: diagnostics positions, schema
//! completion, hover documentation, highlighting, editing, and conversion.
//! The helpers re-exported here are the building blocks they use.
//!
//! ```
//! use leptos_config_editor::Language;
//! use leptos_config_editor::language::{
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
    InlineCaret, InlineEntry, InlineKey, entry_follows, inline_caret, inline_key_at, inline_keys,
    inline_sibling_keys, inline_value_len, skip_quoted,
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
    /// The language writes an array by repeating its key (XML sibling
    /// elements), so an array property already present is offered again.
    pub repeats_arrays: bool,
    /// An existing entry starts right after the caret (`lim|listen = 1`),
    /// and this is the word its key begins with. A suggestion ending in that
    /// word completes it (`inse|cure_plaintext`) and renames the key; any
    /// other is inserted before the entry, `entry_suffix` moving the entry to
    /// its own line.
    pub entry_after: Option<String>,
}

/// Splits a JSON pointer into unescaped segments.
pub fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
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

/// A document language for the editor.
///
/// Only [`info`](Self::info) and [`parse`](Self::parse) are required; each
/// other method adds one capability (diagnostic positions, completion, hover,
/// highlighting, conversion, editing) and defaults to doing without it.
/// Offsets named `cursor` and returned carets are UTF-16 indices into the
/// document, as the browser reports them.
///
/// Built-in languages implement this trait the same way an external one does;
/// wrap an implementation with [`Language::new`](crate::Language::new).
pub trait ConfigLanguage: Send + Sync + 'static {
    /// The language's label, icon, and extensions.
    fn info(&self) -> LanguageInfo;

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

    /// The key under character `column` of `line`, and the column it starts at.
    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        let _ = (line, column);
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

    /// The text Enter inserts at `cursor` and the caret's byte offset within
    /// it. The default keeps the line's indentation.
    fn newline(&self, source: &str, cursor: usize) -> (String, usize) {
        let byte = super::keyboard::utf16_to_byte_index(source, cursor);
        let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
        let indent: String = source[line_start..byte]
            .chars()
            .take_while(|character| matches!(character, ' ' | '\t'))
            .collect();
        let text = format!("\n{indent}");
        let caret = text.len();
        (text, caret)
    }

    /// The document with the selected lines commented or uncommented, and the
    /// remapped selection; `None` when the language has no comments.
    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        let _ = (source, selection_start, selection_end);
        None
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
}
