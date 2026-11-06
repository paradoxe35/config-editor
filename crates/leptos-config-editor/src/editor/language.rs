//! Core language contract. Language-agnostic: defines the
//! [`LanguageBehavior`] text-layer trait and the shared signature types.
//! Shared orchestration (`diagnostics.rs`, `completion.rs`, `keyboard.rs`,
//! `commands.rs`) only depends on this module. The [`Language`](super::languages::Language)
//! selector and its registry live in `languages/`; language implementations
//! (`languages/yaml.rs`, `languages/json.rs`, …) depend on this contract from
//! the other side. Production code in core never names a language file and
//! never matches on language variants — it programs against the trait.
//!
//! Test code is the deliberate exception: tests pin a concrete language via
//! `Language::Yaml.behavior()` to exercise the shared orchestration.

use leptos::prelude::*;
use serde_json::Value;
use std::collections::HashSet;

/// A syntax error in a document, with its one-based position when the
/// language reports one. The editor shows it as an inline diagnostic.
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

/// A path segment addressing a value inside the parsed document. Shared by
/// every language: schema navigation reads parsed `Value`s, never syntax.
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

/// Everything a language needs to render one property insert.
pub(super) struct InsertContext<'a> {
    /// Indentation of the line the entry starts on.
    pub(super) base_indent: &'a str,
    /// One indentation level, as the document writes it.
    pub(super) unit: &'a str,
    pub(super) position: EntryPosition,
    /// Schema path of the container receiving the entry.
    pub(super) path: &'a [SchemaPathSegment],
}

/// The caret sits after containers opened earlier on the same line
/// (`"web": {"le|}`). A multi-line insert there is moved onto its own line at
/// `indent`, and whatever followed the caret is pushed to `after`, so a nested
/// block comes out formatted instead of glued to the opening brace. Computed by
/// [`inline_block`]; only multi-line inserts use it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct InlineBlock {
    /// UTF-16 replace range used when expanding: it swallows the blanks around
    /// the caret so no trailing whitespace is left behind.
    pub(super) start: usize,
    pub(super) end: usize,
    /// Indentation of the entry's own line.
    pub(super) indent: String,
    /// Text placed after the insert: a line break plus the closing indent
    /// before a closer, or a separator plus line break before a following
    /// entry. Empty when nothing follows on the line.
    pub(super) after: String,
}

/// How many `{` / `[` opened on `line_prefix` (the line's text before the
/// caret) are still open, ignoring brackets inside double-quoted strings.
/// Shared by the brace languages (JSON, HCL, TOML inline tables).
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

/// Computes the [`InlineBlock`] for a replace range `start..end` (bytes) on a
/// line where `depth` containers opened before the caret are still open.
/// `closes` tells whether the text after the caret starts with the language's
/// closer (`}` / `]` / `</`); `separator` is written between the insert and an
/// entry that follows on the same line (`,` in JSON, nothing in XML).
/// Shared by every language whose containers can open mid-line.
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
        // An entry follows: write one separator (swallowing the one already
        // there) and move the entry onto its own line.
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

/// Where the caret is inside the document's structure: the schema path of the
/// enclosing mapping, the partially typed query, the replace range, and —
/// when editing a mapping value — which key's value is being edited.
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
    /// Text the language needs before any entry inserted at the caret (for
    /// example a separator after a complete value). Core prepends it verbatim.
    pub(super) entry_prefix: &'static str,
    /// Whether the caret has begun an entry whose empty query should still open
    /// the suggestion list (for example a typed opening quote in JSON).
    pub(super) explicit_entry_start: bool,
    /// Text the language needs after an entry inserted at the caret, such as a
    /// separator before an existing entry that follows (JSON). Core appends it
    /// verbatim; the caret stays before it.
    pub(super) entry_suffix: &'static str,
    /// How a property inserted here is written (child entry, inline entry, or
    /// XML attribute).
    pub(super) position: EntryPosition,
    /// Set when containers opened earlier on the caret's line are still open:
    /// multi-line property inserts then move onto their own line.
    pub(super) block: Option<InlineBlock>,
    /// The caret is somewhere no property may be inserted (for example the
    /// start tag of the XML config root, whose attributes are editor
    /// metadata): property completion stays empty.
    pub(super) no_properties: bool,
}

/// Splits a JSON-pointer path into unescaped segments. Generic path utility
/// used by every language's span mapping; lives in the core contract so both
/// the shared orchestration and the implementations share one definition.
#[cfg(feature = "schema-validation")]
pub(super) fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// The text layer each document language implements. The shared orchestration
/// (validation flow, completion ranking, hover docs, pairing mechanics) calls
/// into this; per-language files implement it.
/// Every method is exercised by the editor or the shared orchestrators —
/// helpers used only inside one language stay free functions in its file.
pub(super) trait LanguageBehavior: Send + Sync {
    /// Parses the document into a schema-navigable `Value`.
    fn parse(&self, source: &str) -> Result<Value, ParseError>;
    /// Source span of the key at a JSON-pointer path.
    #[cfg(feature = "schema-validation")]
    fn span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)>;
    /// Source span of the value at a JSON-pointer path.
    #[cfg(feature = "schema-validation")]
    fn value_span_for_path(&self, source: &str, path: &str) -> Option<(usize, usize, usize)>;
    /// Source span of the first non-blank content, for whole-document errors.
    #[cfg(feature = "schema-validation")]
    fn first_span(&self, source: &str) -> Option<(usize, usize, usize)>;
    /// Structural context of the caret for completions and hover.
    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext;
    /// Keys already present in the mapping at `path`, to filter suggestions.
    /// `cursor_byte` is a byte index; the active line is ignored so a
    /// partially typed key does not hide the keys around it.
    fn existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>>;
    /// Insert text (plus caret offset, UTF-16) for a schema property
    /// completion, written as `context.position` asks.
    fn property_insert(
        &self,
        key: &str,
        property: &Value,
        root: &Value,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)>;
    /// Render a schema value (default, const, example) as document text.
    fn format_value(&self, value: &Value) -> String;
    /// The mapping key under `column` (a character index) on a single line,
    /// with its starting column. Comment-only lines yield `None`.
    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)>;
    /// Byte offset within `line` where the hover context is read for the key at
    /// `column`. YAML reads it at the line end; JSON reads it just after the key's
    /// colon, because one line can open a nested object or hold several keys.
    fn hover_cursor(&self, line: &str, _column: usize) -> usize {
        line.len()
    }
    /// Syntax-highlighted rendering of a single document line.
    fn highlight_line(&self, line: &str) -> AnyView;
    /// Indented newline insertion honoring the document's own style.
    fn newline_with_indent(&self, source: &str, cursor: usize) -> String;
    /// The insertion for Enter, plus the caret's byte offset within it. Defaults
    /// to the caret after the inserted text; a language that opens a block
    /// between its own delimiters places the caret on the new middle line.
    fn newline_insertion(&self, source: &str, cursor: usize) -> (String, usize) {
        let text = self.newline_with_indent(source, cursor);
        let caret = text.len();
        (text, caret)
    }
    /// Toggles line comments over the selection. Returns the updated
    /// document with remapped UTF-16 selection bounds.
    fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)>;
    /// Whether the caret has just begun an entry that should open the suggestion
    /// list immediately (see `CompletionContext::explicit_entry_start`).
    fn entry_started(&self, _source: &str, _cursor: usize) -> bool {
        false
    }
    /// Whether the caret sits inside a string or comment: the pairing guard.
    fn lexical_state(&self, source: &str, cursor: usize) -> (Option<char>, bool);
    /// How strongly `source` looks like this language (0–255). Reserved for a
    /// future `Language::Auto` selector that will detect the document type at
    /// runtime from the initial/baseline value and pick the highest-scoring
    /// behavior. Defaults to zero so a new language compiles before tuning it.
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
