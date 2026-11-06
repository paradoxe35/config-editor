//! The language contract: the [`LanguageBehavior`] trait every language
//! implements, plus the types it exchanges with the shared orchestration
//! (`diagnostics.rs`, `completion.rs`, `keyboard.rs`, `commands.rs`).
//!
//! Core code programs against this trait and never names or matches on a
//! language; tests may pin one (`Language::Yaml.behavior()`). Implementations
//! live in `languages/<name>.rs` and are registered in `languages/mod.rs`.

use leptos::prelude::*;
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
#[derive(Clone, PartialEq, Eq)]
pub(super) enum SchemaPathSegment {
    Key(String),
    ArrayItem(usize),
}

/// How an entry inserted at the caret must be written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum EntryPosition {
    /// A child entry in a multi-line body (the normal case).
    #[default]
    Block,
    /// Inside a container that must stay on one line, such as a TOML inline
    /// table: nested objects are written inline too.
    Inline,
    /// An XML-style attribute on the current tag (`name="value"`).
    Attribute,
}

pub(super) struct InsertContext<'a> {
    /// Indentation of the line the entry starts on.
    pub(super) base_indent: &'a str,
    /// One indentation level, as the document writes it.
    pub(super) unit: &'a str,
    pub(super) position: EntryPosition,
    /// Schema path of the container receiving the entry.
    pub(super) path: &'a [SchemaPathSegment],
}

/// How a multi-line insert is laid out when the caret follows containers
/// opened on the same line (`"web": {"le|}`): the insert moves onto its own
/// line at `indent`, and whatever followed the caret moves to `after`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct InlineBlock {
    /// UTF-16 replace range; it swallows the blanks around the caret so no
    /// trailing whitespace is left behind.
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) indent: String,
    /// Line break plus closing indent before a closer, separator plus line
    /// break before a following entry, or empty.
    pub(super) after: String,
}

/// How many `{` / `[` opened on `line_prefix` are still open, ignoring
/// brackets inside double-quoted strings.
pub(super) fn unclosed_brackets(line_prefix: &str) -> usize {
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
pub(super) fn inline_block(
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

/// Where the caret sits in the document's structure, as a language reports it.
/// Offsets are UTF-16; `value_key` is set when the caret is in a value.
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct CompletionContext {
    pub(super) path: Vec<SchemaPathSegment>,
    pub(super) typed: String,
    pub(super) replace_start: usize,
    pub(super) replace_end: usize,
    pub(super) value_key: Option<String>,
    pub(super) value_needs_space: bool,
    pub(super) is_array_item: bool,
    pub(super) base_indent: String,
    /// Prepended verbatim to an inserted entry (for example a separator after
    /// a complete value).
    pub(super) entry_prefix: &'static str,
    /// An empty query still opens the suggestion list (for example after a
    /// typed opening quote in JSON).
    pub(super) explicit_entry_start: bool,
    /// Appended verbatim to an inserted entry, after the caret (for example a
    /// separator before a following JSON entry).
    pub(super) entry_suffix: &'static str,
    pub(super) position: EntryPosition,
    /// See [`inline_block`].
    pub(super) block: Option<InlineBlock>,
    /// No property may be inserted here (for example the XML config root's
    /// start tag, whose attributes are editor metadata).
    pub(super) no_properties: bool,
}

/// Splits a JSON pointer into unescaped segments.
#[cfg(feature = "schema-validation")]
pub(super) fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// The text layer a language implements; the shared orchestration calls it.
/// Unless noted, `cursor` and returned offsets are UTF-16 indices. Helpers
/// used by only one language stay free functions in its file.
pub(super) trait LanguageBehavior: Send + Sync {
    /// The document as the shared data model, or its first syntax error.
    fn parse(&self, source: &str) -> Result<Value, ParseError>;
    /// `(line, column, end_column)`, one-based, of the key at the JSON pointer
    /// `path`.
    #[cfg(feature = "schema-validation")]
    fn span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)>;
    /// Like `span_for_path`, but the span of the value.
    #[cfg(feature = "schema-validation")]
    fn value_span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)>;
    /// Span of the first non-blank content, for whole-document errors.
    #[cfg(feature = "schema-validation")]
    fn first_span(&self, source: &str) -> Option<(usize, usize, usize)>;
    /// The caret's structural context, for completion and hover.
    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext;
    /// Keys already present in the mapping at `path`, ignoring the caret's
    /// line (`cursor_byte` is a byte index) so a half-typed key hides nothing.
    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>>;
    /// Text and caret offset for inserting property `key`, written as
    /// `context.position` asks.
    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)>;
    /// A schema value (default, const, example) as document text.
    fn format_value(&self, value: &Value) -> String;
    /// The key under character `column` of `line`, with its starting column.
    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)>;
    /// Byte offset in `line` where the hover context for the key at `column`
    /// is read. Override when one line can hold several keys (JSON).
    fn hover_cursor(&self, line: &str, _column: usize) -> usize {
        line.len()
    }
    /// One document line, syntax-highlighted.
    fn highlight_line(&self, line: &str) -> AnyView;
    /// The text Enter inserts: a newline plus the indentation to continue with.
    fn newline_with_indent(&self, source: &str, cursor: usize) -> String;
    /// The text Enter inserts, plus the caret's byte offset within it.
    /// Override to put the caret on a middle line between delimiters.
    fn newline_insertion(&self, source: &str, cursor: usize) -> (String, usize) {
        let text = self.newline_with_indent(source, cursor);
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
    ) -> Option<(String, usize, usize)>;
    /// Whether the caret has just begun an entry, so suggestions open at once.
    fn entry_started(&self, _source: &str, _cursor: usize) -> bool {
        false
    }
    /// The quote of the string the caret is in, and whether it is in a
    /// comment; brackets and quotes do not auto-pair there.
    fn lexical_state(&self, source: &str, cursor: usize) -> (Option<char>, bool);
    /// How strongly `source` looks like this language (0 to 255), reserved for
    /// automatic language detection.
    #[allow(dead_code)]
    fn confidence(&self, _source: &str) -> u8 {
        0
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "schema-validation")]
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
