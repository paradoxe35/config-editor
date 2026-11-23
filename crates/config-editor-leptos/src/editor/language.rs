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
//!             let (key, value) = line.split_once('=').ok_or_else(|| {
//!                 // The message quotes nothing of the text.
//!                 ParseError::new("expected KEY=value", Some(index + 1), Some(1))
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
pub use super::comment_toggle::{Toggled, toggle_line_comments, unwrap_block_comments};
pub use super::completion::{
    Fill, InsertPlan, Planned, RequiredChild, array_branch, fill_value, kept_const, kept_default,
    language_keeps, object_branch, placeholder, required_children, schema_const, schema_default,
    schema_example, writes_key,
};
pub use super::flat_keys::{flat_value_text, flat_write_insert};
pub use super::fold_ranges::{
    FoldRange, IndentLine, bracket_fold_ranges, comment_fold_ranges, default_fold_ranges,
    indent_fold_ranges, merge_fold_ranges, string_body_fold_ranges,
};
pub use super::highlight::{ConstructKind, MultilineConstruct, Token, TokenKind, Tokens};
pub use super::item_sources::SaveTold;
pub use super::keyboard::utf16_to_byte_index;
pub use super::languages::inline::{
    InlineCaret, InlineEntry, InlineKey, caret_value_end, entry_follows, in_front_of_inline_entry,
    inline_caret, inline_key_at, inline_key_paths, inline_keys, inline_value_len, skip_quoted,
    statement_start, statements, string_end_at_caret, unclosed_brackets,
};
pub use item_start::bracket_item_start;
pub(crate) use item_start::cursor_at;

mod item_start;
mod said;
mod writer_door;
use super::comment_toggle::comment_target;
use super::parse_cache::string_bodies;
use super::paths::FieldPath;
pub(crate) use said::Said;
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
    /// Each part of `message` that quotes the text, with the text it
    /// quotes: where a secret may be written, the editor shows those parts
    /// as `…` and every other word of the message as it is. `Some(vec![])`
    /// for a message quoting nothing. `None` when the language does not
    /// say: the editor then reads the text again with its letters and
    /// digits changed and hides what changes with them in the message (a
    /// parse per try, and it may hide a word too many). Every built-in
    /// language gives its quotes.
    pub quotes: Option<Vec<Quote>>,
}

/// A part of a syntax message that quotes the text parsed (a key, a
/// name, a number as written), so the editor can hide exactly that part
/// where it may show a secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quote {
    /// Where the message says it: a byte range of [`ParseError::message`].
    pub said: std::ops::Range<usize>,
    /// What it quotes: a byte range of the text given to
    /// [`ConfigLanguage::parse`]. A range the language cannot narrow (a
    /// parser's own sentence naming text where it stopped) may be wider
    /// than the text quoted, never narrower.
    pub of: std::ops::Range<usize>,
}

impl ParseError {
    /// The error `message` at `line` and `column`, quoting nothing of the
    /// text.
    pub fn new(message: impl Into<String>, line: Option<usize>, column: Option<usize>) -> Self {
        ParseError {
            message: message.into(),
            line,
            column,
            quotes: Some(Vec::new()),
        }
    }
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

/// What [`ConfigLanguage::write_insert`] needs to lay out an insert.
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
    /// The key is written already as items the language repeats
    /// ([`Typing::rewritable_keys`]): the entry is one more item,
    /// written the same way (another HCL block).
    pub another_item: bool,
    /// The schema says of some property of the container whether it is
    /// an XML attribute (`"xml": {"attribute": true}`, or `false` for an
    /// element): a start tag offers the attributes alone, and the content
    /// the others.
    pub attributes_marked: bool,
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
    /// byte offset); returns the quote of the one `text` ends inside.
    fn walk(&self, text: &str, mut on: impl FnMut(usize, char)) -> Option<char> {
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
        quote.map(|(open, _)| open)
    }

    /// Whether the end of `text` (before the caret) is inside a string.
    fn ends_inside(&self, text: &str) -> bool {
        self.walk(text, |_, _| {}).is_some()
    }
}

/// The lexical state at `cursor` (UTF-16) as `language`'s configuration
/// and comments tell it: the default of [`Text::lexical_state`], for a
/// language refining it to build on.
pub fn configured_lexical_state(
    language: &dyn ConfigLanguage,
    source: &str,
    cursor: usize,
) -> Lexical {
    let byte = utf16_to_byte_index(source, cursor);
    let config = language.config();
    // A line comment runs on at its end; one over lines, or ended by the
    // block comment's closer, is closed there, unless the text ends in it.
    let closed = |range: &std::ops::Range<usize>| {
        let text = &source[range.clone()];
        range.end < source.len()
            && (text.contains('\n')
                || config
                    .block_comment
                    .is_some_and(|(_, close)| text.ends_with(close)))
    };
    let in_comment = language.comments(source).iter().any(|range| {
        range.start < byte && (byte < range.end || (byte == range.end && !closed(range)))
    });
    if in_comment {
        return Lexical {
            quote: None,
            in_comment: true,
        };
    }
    if let Some(range) = language
        .multiline_strings(source)
        .into_iter()
        .find(|range| range.start < byte && byte < range.end)
    {
        return Lexical {
            quote: source[range.start..].chars().next(),
            in_comment: false,
        };
    }
    let line_start = source[..byte].rfind('\n').map_or(0, |at| at + 1);
    Lexical {
        quote: Strings::of(&config).walk(&source[line_start..byte], |_, _| {}),
        in_comment: false,
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
        start: super::paths::api_units(source, block_start),
        end: super::paths::api_units(source, block_end),
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
    /// In an empty document, how its first entry opens the first item of a
    /// root list, for a schema whose root is a list of objects: the
    /// delimiters the entry is written between (JSON's `[\n  {\n    ` and
    /// `\n  }\n]\n`, YAML's `- `) and the indentation the item's entries
    /// add to [`base_indent`](Self::base_indent). `None` (the default)
    /// offers nothing there.
    pub root_item: Option<(&'static str, &'static str, &'static str)>,
    /// As [`root_item`](Self::root_item), for a root list of values
    /// (strings, numbers, choices): what a value is written between as its
    /// first item (JSON's `[\n  ` and `\n]\n`, YAML's `- `). `None` offers
    /// nothing there.
    pub root_value: Option<(&'static str, &'static str)>,
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

/// `source` with `edits` (apart from each other) made, built in one pass
/// (not the text moved along once per edit).
pub(crate) fn with_edits(source: &str, mut edits: Vec<KeyEdit>) -> String {
    edits.sort_by_key(|(range, _)| range.start);
    let grown: usize = edits.iter().map(|(_, text)| text.len()).sum();
    let mut edited = String::with_capacity(source.len() + grown);
    let mut from = 0;
    for (range, text) in edits {
        edited.push_str(source.get(from..range.start).unwrap_or_default());
        edited.push_str(&text);
        from = from.max(range.end);
    }
    edited.push_str(&source[from..]);
    edited
}

/// Splits a JSON pointer into unescaped segments, each verbatim (RFC 6901:
/// `~1` is `/`, then `~0` is `~`).
pub fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/').skip(1).map(unescaped_key).collect()
}

/// `key` as one segment of a JSON pointer, escaped (RFC 6901: `~` is
/// written `~0`, then `/` is written `~1`).
///
/// ```
/// use leptos_config_editor::language::{pointer_key, unescaped_key};
/// assert_eq!(pointer_key("a/b~c"), "a~1b~0c");
/// assert_eq!(unescaped_key("a~1b~0c"), "a/b~c");
/// ```
pub fn pointer_key(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// One segment of a JSON pointer, unescaped: the key it names (`~1` is
/// `/`, then `~0` is `~`).
pub fn unescaped_key(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
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

/// How a language writes arrays besides literal lists, as the editor reads
/// it off the language's [`Repeats`] (`dyn ConfigLanguage::repetition`).
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

/// What a language with [`Repeats`] repeats to write a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Repeated {
    /// Any value (XML sibling elements).
    Elements,
    /// Objects, as blocks (HCL `certificates { … }`); other values are
    /// always lists.
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
/// hooks, like VS Code's `language-configuration.json`, and its
/// [`traits`](Self::traits). The defaults of [`Typing::newline`],
/// [`Typing::toggle_comments`], [`Text::comment_start`],
/// [`Text::lexical_state`] and the key/value hooks come from it, so a simple
/// language gets them without code; a group method a language implements
/// itself takes precedence.
///
/// A flat key/value language (one `KEY=value` entry per line, no nesting)
/// gets schema completion and hover from its separators too, by delegating
/// those hooks to the `flat_*` methods
/// ([`flat_completion_context`](Self::flat_completion_context),
/// [`flat_property_insert`](Self::flat_property_insert),
/// [`flat_hover_key`](Self::flat_hover_key),
/// [`flat_format_value`](Self::flat_format_value)).
///
/// ```
/// use leptos_config_editor::language::{LanguageConfig, LanguageTraits};
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
///     traits: LanguageTraits::NONE,
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
    /// How the language reads and writes values; see [`LanguageTraits`].
    pub traits: LanguageTraits,
}

/// What a language's values are like, beyond its syntax: each `false` by
/// default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LanguageTraits {
    /// The language has numbers for infinity and NaN. Its writers
    /// ([`serialize`](ConfigLanguage::serialize),
    /// [`format_value`](ConfigLanguage::format_value) and the
    /// [`SaveLayout`] ones that write values) are then given the data
    /// model's values, and read each scalar with
    /// [`float_of`](crate::float_of) and [`string_of`](crate::string_of).
    /// Without it they are given plain data: infinities and NaN as the
    /// text `inf`, `-inf` or `nan` (a conversion reports them as values the
    /// language cannot hold), and each string as the text it holds.
    pub writes_non_finite: bool,
    /// The language has one number type, so `2` and `2.0` are the same
    /// value (HCL), even when [`parse`](ConfigLanguage::parse) keeps how a
    /// number was written. An integer typed over a float the host holds
    /// then keeps the host's float type: the view cannot say otherwise.
    pub one_number_type: bool,
    /// Indentation is structure (YAML: a key's indent says which block it
    /// belongs to). A read-only field's lines then keep their indent;
    /// where it is not, re-indenting them stays free.
    pub indent_is_structure: bool,
    /// Values may be written unquoted to the end of their line (YAML plain
    /// scalars), so any character typed right next to one, punctuation
    /// included, joins it. Otherwise punctuation ends a value.
    pub plain_scalars: bool,
    /// Every scalar is written as text, its type only guessed on reading
    /// (XML: `<v>1</v>` reads as a number, `<v></v>` as an empty element).
    /// A value edited while editing as such a language keeps the host's
    /// type: a string stays a string, and an emptied value is the empty
    /// string, or waits where the host holds a number or a boolean. An
    /// integer past 64 bits typed there is saved as its digits, as the
    /// view's own language saves it (elsewhere the save waits).
    pub scalars_are_text: bool,
}

impl LanguageTraits {
    /// No trait: what [`Default`] gives, for `const` configurations.
    pub const NONE: Self = LanguageTraits {
        writes_non_finite: false,
        one_number_type: false,
        indent_is_structure: false,
        plain_scalars: false,
        scalars_are_text: false,
    };
}

/// A document language for the editor.
///
/// Only [`info`](Self::info) and [`parse`](Self::parse) are required; each
/// other method adds one capability (diagnostic positions, completion, hover,
/// highlighting, conversion) and defaults to doing without it, or to what
/// [`config`](Self::config) describes. Offsets named `cursor` and returned
/// carets are UTF-16 indices into the document, as the browser reports them.
///
/// Simple syntax (comments, quotes, brackets, key/value separators) and the
/// language's traits are data, in [`config`](Self::config). The rest comes in
/// groups, each a trait of its own the language implements only when it
/// needs more than the defaults:
/// - refinements, whose defaults come from the config: [`Text`] (strings,
///   comments and constructs over lines), [`Typing`] (Enter, Ctrl+/, typed
///   closers), [`Hover`] and [`SaveLayout`] (how an Edit-as save writes the
///   language);
/// - capabilities only some languages have: [`Markup`] (XML-like tags),
///   [`Sharing`] (values written once and read in several places, or written
///   with more than their data) and [`Repeats`] (lists written by repeating
///   a key).
///
/// A language implementing a group returns itself from the group's accessor
/// (`fn text(&self) -> &dyn Text { self }`). The editor reads every hook
/// through the language (`language.comment_start(line)` on a
/// `&dyn ConfigLanguage`): from the group the language implements, else the
/// group's default. Group methods whose default reads the language take it
/// as `language`, the language they belong to.
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
    ///
    /// The error's [`quotes`](ParseError::quotes) say which parts of its
    /// message quote the text: the editor hides each one that may show a
    /// secret (a placeholder, or text where the schema marks a secret).
    /// Without them (`None`) it finds them by reading the text again with
    /// its letters changed, which costs a parse per try and may hide a
    /// word of the message too.
    fn parse(&self, source: &str) -> Result<Value, ParseError>;

    /// The document for `value`, keeping the framing of `previous` (this
    /// language's last text for the document) where the language has any.
    /// The editor calls every writer through one door, which gives `value`
    /// as plain data: each string as the text to write, and each infinity
    /// or NaN as the text `inf`, `-inf` or `nan`. With
    /// [`LanguageTraits::writes_non_finite`] the language is given the
    /// data model's value instead and reads each scalar with
    /// [`float_of`](crate::float_of) and [`string_of`](crate::string_of).
    ///
    /// # Errors
    ///
    /// A message when the language cannot express `value`. The default
    /// supports no conversion.
    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        let _ = (value, previous);
        Err(format!("{} documents cannot be written", self.info().label))
    }

    /// The span of the key of the field at `path`, for diagnostics.
    fn key_span(&self, source: &str, path: &FieldPath) -> Option<Span> {
        let _ = (source, path);
        None
    }

    /// The span of the value of the field at `path`, for diagnostics.
    fn value_span(&self, source: &str, path: &FieldPath) -> Option<Span> {
        let _ = (source, path);
        None
    }

    /// [`key_span`](Self::key_span) for each of `paths`. A language whose
    /// lookup reads the whole document overrides it to read it once
    /// (problems by the thousand are placed with it).
    fn key_spans(&self, source: &str, paths: &[FieldPath]) -> Vec<Option<Span>> {
        paths
            .iter()
            .map(|path| self.key_span(source, path))
            .collect()
    }

    /// [`value_span`](Self::value_span) for each of `paths`. A language
    /// whose lookup reads the whole document overrides it to read it once.
    fn value_spans(&self, source: &str, paths: &[FieldPath]) -> Vec<Option<Span>> {
        paths
            .iter()
            .map(|path| self.value_span(source, path))
            .collect()
    }

    /// The caret's place in the document's structure, for completion and
    /// hover. The default is the top level with nothing typed.
    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        let _ = (source, cursor);
        CompletionContext::default()
    }

    /// The text a suggestion of `key` inserts, spelling `plan` (what the
    /// core decided the key's value is, from every schema at the key), and
    /// the caret's UTF-16 offset within it; `None` skips the key. The
    /// default writes the key and the language's
    /// [`flat_separator`](LanguageConfig::flat_separator), the caret after
    /// them (`None` without separators).
    fn write_insert(
        &self,
        key: &str,
        plan: &InsertPlan<'_>,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        let _ = (plan, context);
        self.config().flat_property_insert(key)
    }

    /// A value (a schema's default, const or example, or one of the
    /// document's) as document text, written as
    /// [`serialize`](Self::serialize) writes it, and given as it is given
    /// there. The default writes JSON.
    fn format_value(&self, value: &Value) -> String {
        value.to_string()
    }

    /// The key under character `column` of `line`, and the column it starts at.
    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        let _ = (line, column);
        None
    }

    /// The highlighted runs of one line; uncovered text renders plain.
    fn highlight(&self, line: &str) -> Vec<Token> {
        let _ = line;
        Vec::new()
    }

    /// How the language refines its [`Text`] defaults. Default: the
    /// defaults alone (from its [`config`](Self::config)).
    fn text(&self) -> &dyn Text {
        &Defaults
    }

    /// How the language refines its [`Typing`] defaults. Default: the
    /// defaults alone (from its [`config`](Self::config)).
    fn typing(&self) -> &dyn Typing {
        &Defaults
    }

    /// How the language refines its [`Hover`] defaults. Default: the
    /// defaults alone (from its [`config`](Self::config)).
    fn hover(&self) -> &dyn Hover {
        &Defaults
    }

    /// How the language refines its [`SaveLayout`] defaults. Default: the
    /// defaults alone (from its [`config`](Self::config)).
    fn layout(&self) -> &dyn SaveLayout {
        &Defaults
    }

    /// The language's [`Markup`] capability, if it has one. Default: none.
    fn markup(&self) -> Option<&dyn Markup> {
        None
    }

    /// The language's [`Sharing`] capability, if it has one. Default: none.
    fn sharing(&self) -> Option<&dyn Sharing> {
        None
    }

    /// The language's [`Repeats`] capability, if it has one. Default: none.
    fn repeats(&self) -> Option<&dyn Repeats> {
        None
    }
}

/// How a language writes text that is not structure: strings, comments and
/// constructs over several lines, read from its [`LanguageConfig`] by default.
pub trait Text {
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

    /// Whether the caret is in a string or a comment.
    /// The default: in one of the language's [comments](Text::comments)
    /// (past its start, and at its end unless a block comment closes
    /// there), else in a string of its [`LanguageConfig::quotes`] the
    /// caret's line opens before it and leaves open, or in a
    /// [string over lines](Text::multiline_strings).
    fn lexical_state(&self, language: &dyn ConfigLanguage, source: &str, cursor: usize) -> Lexical {
        configured_lexical_state(language, source, cursor)
    }

    /// The byte offset where the trailing comment of `line` starts, outside
    /// strings. Edit-as uses it to match lines that differ only in comments.
    /// See [`comment_start_outside_quotes`].
    fn comment_start(&self, language: &dyn ConfigLanguage, line: &str) -> Option<usize> {
        let config = language.config();
        comment_start_outside_quotes(line, config.line_comments, config.quotes)
    }

    /// Byte ranges of the comments in `source`, in order; an unclosed one
    /// runs to the end. The default finds line comments
    /// ([`comment_start`](Text::comment_start)) on lines outside
    /// multi-line string bodies; a language with comments over several
    /// lines (XML `<!-- … -->`, HCL `/* … */`) reads them whole.
    ///
    /// [`ConfigLanguage::highlight`] reads one line at a time, so the lines
    /// after a comment's first are colored from these (a comment up to its
    /// end, the rest of its last line by `highlight`), and a secret
    /// placeholder written in a comment is told from the real one without
    /// looking up its path.
    fn comments(&self, language: &dyn ConfigLanguage, source: &str) -> Vec<std::ops::Range<usize>> {
        line_comments(language, source)
    }

    /// Whether `source` holds a comment: a line comment
    /// ([`comment_start`](Text::comment_start)) or the start of a
    /// block comment, outside strings and multi-line string bodies. The
    /// Edit-as notice reads it: the data a conversion carries has none.
    fn has_comments(&self, language: &dyn ConfigLanguage, source: &str) -> bool {
        let bodies = string_bodies(language, source);
        let quotes = language.config().quotes;
        let block = language.config().block_comment.map(|(open, _)| [open]);
        source.lines().enumerate().any(|(index, line)| {
            !bodies.iter().any(|lines| lines.contains(&index))
                && (language.comment_start(line).is_some()
                    || block.is_some_and(|open| {
                        comment_start_outside_quotes(line, &open, quotes).is_some()
                    }))
        })
    }

    /// Lines (0-based, end exclusive) that lie wholly inside the body of a
    /// multi-line string (TOML `"""`, an HCL heredoc, a YAML block scalar).
    /// They are text: Enter keeps their indentation, nothing is paired,
    /// aligned or suggested there, and they are colored as a string.
    fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        let _ = source;
        Vec::new()
    }

    /// The lines of strings written over lines that a save keeps as written
    /// (see [`string_body_lines`](Self::string_body_lines)): those, and in
    /// YAML a plain scalar's continuation lines, which the editor leaves to
    /// be typed as keys.
    fn saved_string_lines(
        &self,
        language: &dyn ConfigLanguage,
        source: &str,
    ) -> Vec<std::ops::Range<usize>> {
        string_bodies(language, source).to_vec()
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
    fn multiline_constructs(
        &self,
        language: &dyn ConfigLanguage,
        source: &str,
    ) -> Vec<MultilineConstruct> {
        language
            .multiline_strings(source)
            .into_iter()
            .map(|range| MultilineConstruct::colored(range, TokenKind::String))
            .collect()
    }

    /// The tokens of `text`, a part of a line inside a
    /// [structure construct](ConstructKind::Structure) opened on an earlier
    /// line (the inside of a TOML array, of an XML start tag), up to the
    /// construct's end. A string left open colors to the end of `text`.
    /// Default: [`highlight`](Self::highlight).
    fn highlight_continued(&self, language: &dyn ConfigLanguage, text: &str) -> Vec<Token> {
        language.highlight(text)
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
    fn fold_ranges(&self, language: &dyn ConfigLanguage, source: &str) -> Vec<FoldRange> {
        if !cfg!(feature = "folding") {
            return Vec::new();
        }
        default_fold_ranges(source, &language.config(), &string_bodies(language, source))
    }

    /// The byte ranges the value at `path` is written in, whole: from
    /// its first character over every line it takes to its last closer (a
    /// string's closing quote, a block's `}`, an element's end tag, a
    /// heredoc's terminator), read from the parse. One range, unless the
    /// language writes one value in several places with other fields
    /// between them (a TOML table and a sub-table written apart, repeated
    /// elements or blocks). Empty when `source` does not parse or does not
    /// write `path`.
    ///
    /// A lock covers these ranges with the field's key, so no edit changes
    /// a part of a locked value. The default joins the language's one-line
    /// [`value_span`](ConfigLanguage::value_span) of the value and the key
    /// and value spans of everything inside it where no other field's key
    /// is written between them, and carries the last over the closers of
    /// the containers they leave open ([`container_closer_len`](Self::container_closer_len)).
    /// A language without spans gets none: its fields are checked by value.
    fn value_ranges(
        &self,
        language: &dyn ConfigLanguage,
        source: &str,
        path: &FieldPath,
    ) -> Vec<std::ops::Range<usize>> {
        super::paths::joined_value_ranges(language, source, &path.to_pointer())
    }

    /// How many containers `text` opens and leaves open: by default its
    /// [`LanguageConfig::brackets`] outside its quotes; XML counts the
    /// elements it starts and does not end. A language counting them
    /// otherwise counts them so in [`closing_line`](Self::closing_line) too.
    fn unclosed_containers(&self, language: &dyn ConfigLanguage, text: &str) -> usize {
        super::flow_syntax::Flow::of(language).unclosed(text, language.config().brackets)
    }

    /// The first line from `last` (of `lines`, from 0) where the lines
    /// from `first` through it, joined by line breaks, leave no container
    /// open ([`unclosed_containers`](Self::unclosed_containers)); the last
    /// line when none does: where an entry opened on line `first` ends.
    /// Read once over the lines, so an entry of many lines costs its lines;
    /// a language counting containers otherwise than by its brackets (XML
    /// its elements) counts them here the same way.
    fn closing_line(
        &self,
        language: &dyn ConfigLanguage,
        lines: &[&str],
        (first, last): (usize, usize),
    ) -> usize {
        super::flow_syntax::Flow::of(language).closing_line(
            lines,
            language.config().brackets,
            (first, last),
        )
    }

    /// What closes each container `text` opens and leaves open, innermost
    /// first: by default the closing brackets of its
    /// [`LanguageConfig::brackets`] outside its quotes and comments; XML
    /// the end tags of the elements it starts and does not end. Closes the
    /// start of a document cut before the caret, to read what it holds
    /// while the rest does not parse.
    fn container_closers(&self, language: &dyn ConfigLanguage, text: &str) -> Vec<String> {
        super::flow_syntax::Flow::of(language)
            .closers(text, language.config().brackets)
            .into_iter()
            .map(String::from)
            .collect()
    }

    /// The length of what closes a container at the start of `text` (a
    /// closing bracket; XML's element text then its end tag), or `None`.
    fn container_closer_len(&self, language: &dyn ConfigLanguage, text: &str) -> Option<usize> {
        language
            .config()
            .brackets
            .iter()
            .find(|(_, close)| text.starts_with(*close))
            .map(|(_, close)| close.len_utf8())
    }
}

/// What typing does in a language: Enter, Ctrl+/, closers typed or written
/// for the user, and the values a suggestion leaves to type. The defaults
/// read the language's [`LanguageConfig`].
pub trait Typing {
    /// What Enter writes at `cursor` (UTF-16). The default keeps the line's
    /// indentation and opens the [`LanguageConfig::brackets`] it is between.
    fn newline(&self, language: &dyn ConfigLanguage, source: &str, cursor: usize) -> Newline {
        let config = language.config();
        bracket_newline(source, cursor, config.brackets, &config, |line| {
            &line[..language.comment_start(line).unwrap_or(line.len())]
        })
    }

    /// The UTF-16 selection `start..end` widened over whole lines and the
    /// constructs it would cut through (strings, brackets, elements over
    /// lines, and what [`comment_scope`](Self::comment_scope) adds): what
    /// Ctrl+/ comments, and what copy or cut on a folded line takes. The
    /// default widens over strings and the [`LanguageConfig::brackets`].
    fn whole_constructs(
        &self,
        language: &dyn ConfigLanguage,
        source: &str,
        start: usize,
        end: usize,
    ) -> (usize, usize) {
        let (mut start, mut end) = (start, end);
        // Whole strings and constructs over lines, until both hold.
        loop {
            let widened = over_whole_strings(language, source, start, end);
            let widened =
                super::comment_scope::over_balanced_lines(language, source, widened.0, widened.1);
            let widened = language.comment_scope(source, widened.0, widened.1);
            if widened == (start, end) {
                return widened;
            }
            (start, end) = widened;
        }
    }

    /// The edits that comment or uncomment the selected lines (UTF-16
    /// offsets), and the selection after (see [`Toggled`]); `None` when the
    /// language has no comments. A
    /// selection's lines are toggled exactly as selected (so a toggle and
    /// the next one on the lines it leaves selected give the text back); a
    /// caret's line is widened over the whole constructs it is in (see
    /// [`comment_target`]). The default uses [`LanguageConfig::line_comments`].
    fn toggle_comments(
        &self,
        language: &dyn ConfigLanguage,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<Toggled> {
        let config = language.config();
        // Inside a block comment on lines of its own: its markers go.
        if let Some(block) = config.block_comment
            && let Some(unwrapped) =
                unwrap_block_comments(source, selection_start, selection_end, block)
        {
            return Some(unwrapped);
        }
        let (selection_start, selection_end) =
            comment_target(language, source, selection_start, selection_end);
        config
            .block_comment
            .and_then(|block| unwrap_block_comments(source, selection_start, selection_end, block))
            .or_else(|| {
                toggle_line_comments(source, selection_start, selection_end, config.line_comments)
            })
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

    /// Whether the caret has just begun an entry (a JSON key's opening
    /// quote), so suggestions open before anything is typed.
    fn entry_started(&self, source: &str, cursor: usize) -> bool {
        let _ = (source, cursor);
        false
    }

    /// Whether `rest`, the text after a key being completed, shows the key
    /// already has its value (`: 99`, ` = "x"`, an HCL block's `{`) or goes
    /// on (a dotted key's `.path`), so accepting a suggestion renames only
    /// the key.
    fn value_follows_key(&self, language: &dyn ConfigLanguage, rest: &str) -> bool {
        let rest = rest.trim_start();
        // `==` is a comparison, not a separator.
        language.config().separators.iter().any(|separator| {
            rest.strip_prefix(separator)
                .is_some_and(|after| !after.starts_with(separator))
        })
    }

    /// Byte offsets in `text`, a block a suggestion just wrote, where a value
    /// is still empty besides empty quotes (`key: ` at a line end, `<k></k>`):
    /// `Tab` walks them like snippet placeholders. See [`empty_slots_at_line_ends`].
    fn empty_value_slots(&self, language: &dyn ConfigLanguage, text: &str) -> Vec<usize> {
        let endings: Vec<String> = language
            .config()
            .separators
            .iter()
            .flat_map(|separator| [format!("{separator} "), (*separator).to_string()])
            .collect();
        let endings: Vec<&str> = endings.iter().map(String::as_str).collect();
        empty_slots_at_line_ends(text, &endings)
    }

    /// The keys the object at `path` has already (read from the document
    /// around the caret at byte `cursor_byte`) that the entry typed there
    /// may still write, though the data cannot tell: an HCL block type
    /// written as repeated blocks (one more block follows, see
    /// [`Repeats`]), a TOML array of tables written as `[[…]]` headers
    /// (one more table follows), or a TOML table only its sub-tables'
    /// headers make (a header being named may name it). Default: none.
    fn rewritable_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> HashSet<String> {
        let _ = (source, cursor_byte, path);
        HashSet::new()
    }
}

/// Where the hover card finds a key, for keys the completion context cannot
/// place.
pub trait Hover {
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
        language: &dyn ConfigLanguage,
        source: &str,
        line_start: usize,
        line: &str,
        column: usize,
    ) -> Option<usize> {
        bracket_item_start(language, source, line_start, line, column)
    }
}

/// How an Edit-as save writes the language where its text is kept: which
/// lines read alike, constructs written on one line, renames, and the forms
/// a value is written in. The defaults keep nothing special.
pub trait SaveLayout {
    /// `code` (a line without its comment, trimmed) as an Edit-as save
    /// compares it with the serializer's line for the same entry: written
    /// forms that read the same are made equal (YAML's `-   a` and `- a`,
    /// `key: &anchor`, `key: ~`). The default keeps it.
    fn comparable_line<'a>(&self, code: &'a str) -> std::borrow::Cow<'a, str> {
        std::borrow::Cow::Borrowed(code)
    }

    /// `source` with the members of the map at `path` that it writes
    /// together inside one construct, not as entries of lines of their own
    /// (an XML element's attributes, in its start tag), in `order`'s order
    /// among themselves, each written as it was. A save moving keys
    /// through another view then keeps their order. Default: `None`, none
    /// such.
    fn members_in_order(&self, source: &str, path: &FieldPath, order: &[String]) -> Option<String> {
        let _ = (source, path, order);
        None
    }

    /// `source` with the entries of each container it writes on one line
    /// (an XML element's children, and the comments between them) on lines
    /// of their own, reading the same, for a structural Edit-as save to
    /// patch line by line: what it does not touch stays as written,
    /// comments included. [`entries_together`](Self::entries_together)
    /// writes them on one line again. Default: `None`, nothing spread.
    fn entries_apart(&self, source: &str) -> Option<String> {
        let _ = source;
        None
    }

    /// `patched` (a save made on [`entries_apart`](Self::entries_apart) of
    /// `previous`) with each container `previous` writes on one line written
    /// on one line again. Default: `patched`.
    fn entries_together(&self, patched: &str, previous: &str) -> String {
        let _ = previous;
        patched.to_string()
    }

    /// The lines (0-based) of `text`, written by
    /// [`serialize`](Self::serialize) over a host, that continue a
    /// construct begun on an earlier line and are indented as the host
    /// aligns them, not by depth (an XML start tag's attribute lines). A
    /// structural Edit-as save indents no line it adds from them, and
    /// writes such a line it adds as it is. Default: none.
    fn aligned_lines(&self, text: &str) -> Vec<usize> {
        let _ = text;
        Vec::new()
    }

    /// Where a string value that ends at byte `end` of `source` really ends
    /// for keeping it as written: XML text runs through the line break
    /// before its end tag. Default: `end`.
    fn kept_value_end(&self, source: &str, end: usize) -> usize {
        let _ = source;
        end
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

    /// Whether the map at `path` is written in `source` as the labels
    /// of blocks (HCL's `svc "a" { … }` holds `/svc/a`, so `/svc` is such a
    /// map): emptied, it is written as no blocks, not as an empty block.
    /// Default: no.
    fn writes_as_labels(&self, source: &str, path: &FieldPath) -> bool {
        let _ = (source, path);
        false
    }

    /// The container at `path` when the document writes it on one line in
    /// a form other than a `{…}` or `[…]` value: an XML element with its
    /// children on one line, an HCL one-line block. A structural Edit-as
    /// save keeps such a container as written.
    fn one_line_container(&self, source: &str, path: &FieldPath) -> Option<OneLine> {
        let _ = (source, path);
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

    /// The edits renaming the key at `path` everywhere the language
    /// writes it (an XML element's start and end tags, a TOML key's headers
    /// and dotted keys): byte ranges of `source`, apart from each other,
    /// each with its new text; `None` to rename just the text at
    /// [`key_span`](Self::key_span). `new_key` is the new key as
    /// [`written_key`](Self::written_key) writes it; a key the host wrote
    /// in quotes keeps them. A save renaming many keys asks for each in one
    /// batch over `source` (see `paths::with_spans`): what the language
    /// reads of all of it to find them is read once
    /// (`paths::batch_cached`).
    fn key_renames(&self, source: &str, path: &FieldPath, new_key: &str) -> Option<Vec<KeyEdit>> {
        let _ = (source, path, new_key);
        None
    }

    /// `key` as the language writes it in place of the key at `path` of
    /// `source` (a rename): bare, or quoted and escaped where it must be
    /// (YAML quotes `on` and `1_000`, which YAML 1.1 reads as other types).
    /// `None` when the language cannot write it there (an HCL block type is
    /// an identifier): a rename then takes the other save paths, which
    /// refuse what the language cannot hold. By default the key as the
    /// language's own [`serialize`](Self::serialize) writes it, found by
    /// [`key_span`](Self::key_span) in a document holding only that key
    /// and read back by [`parse`](Self::parse); `None` without them.
    fn written_key(
        &self,
        language: &dyn ConfigLanguage,
        source: &str,
        path: &FieldPath,
        key: &str,
    ) -> Option<String> {
        let _ = (source, path);
        key_as_written(language, key)
    }

    /// The value a save writes at `path` of `previous` for `value` (a
    /// changed scalar): `Ok(None)` keeps `value`; `Ok(Some)` is the
    /// language's own form of it where its reader normalizes what it reads
    /// (TOML reads the datetime `…00.50Z` as `…00.5Z`); `Err` says why the
    /// save waits (a TOML datetime made text that is no datetime, which
    /// would make it a string).
    fn scalar_edit(
        &self,
        previous: &str,
        path: &FieldPath,
        value: &Value,
    ) -> Result<Option<Value>, String> {
        let _ = (previous, path, value);
        Ok(None)
    }

    /// The pointer of the first value `patched` (a save's text for
    /// `previous`) writes in another form than `previous` does, where the
    /// two read the same but are different things to the language's
    /// readers: an HCL block and an object attribute (`name = { … }`), an
    /// expression (`var.x`, read as `${var.x}`) and a literal string. A
    /// patch that switches one is not taken, and a save that would write
    /// one waits. `told` says which host item each saved item is, when the
    /// edit's links tell it (a map moved keeps its form).
    fn changed_form(&self, previous: &str, patched: &str, told: SaveTold<'_>) -> Option<String> {
        let _ = (previous, patched, told);
        None
    }

    /// The document for `value` written by an Edit-as save over `previous`
    /// (the host's text), told which host item each item of `value` is
    /// (see [`SaveTold`]): a write that takes each item's markup or form
    /// from its host item (XML elements, HCL blocks) takes it from the item
    /// the edit moved. `None` (the default) writes it as
    /// [`ConfigLanguage::serialize`] does.
    fn serialize_over(
        &self,
        value: &Value,
        previous: Option<&str>,
        told: SaveTold<'_>,
    ) -> Option<Result<String, String>> {
        let _ = (value, previous, told);
        None
    }

    /// Why a save writing `value` (as the view holds it) waits on its form
    /// ([`Self::changed_form`] named it): what is wrong with it, said to
    /// the user (an HCL `${…}` text that is no valid expression any more,
    /// which HCL writes as plain text). `None` (the default) when the form
    /// alone tells it.
    fn changed_form_reason(&self, value: &Value) -> Option<String> {
        let _ = value;
        None
    }

    /// Whether `line` starts a section that runs to the next one, whatever
    /// its indent (TOML's `[table]` header). Default: no sections.
    fn starts_section(&self, line: &str) -> bool {
        let _ = line;
        false
    }

    /// Whether line `index` of `lines` starts an item of a list written as
    /// lines of its own: the comments right above it are the item's, never
    /// the file's header. Default: a line starting with the
    /// [`LanguageConfig::item_marker`]; TOML adds `[[array]]` tables, HCL
    /// labelled and repeated blocks, XML repeated elements.
    fn opens_list_item(&self, language: &dyn ConfigLanguage, lines: &[&str], index: usize) -> bool {
        let line = lines[index];
        let code = &line[..language.comment_start(line).unwrap_or(line.len())];
        super::flow_syntax::Flow::of(language)
            .after_item_marker(code.trim())
            .is_some()
    }

    /// The bytes of the list item or map entry at `path` when the
    /// document writes it as lines of its own that are no `{…}` or `[…]`
    /// value: a TOML `[[array]]` or `[table]` table from its header, an HCL
    /// repeated block from its type to its `}`. An Edit-as save that moves
    /// the item or entry moves these lines with the comment lines above
    /// them. `None` (the default) for any other.
    fn item_lines(&self, source: &str, path: &FieldPath) -> Option<std::ops::Range<usize>> {
        let _ = (source, path);
        None
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

    /// Whether `written` (a scalar as the language writes one in a flow
    /// collection) reads as `value`, however it is spelled (`"caf\u00e9"`
    /// for `"café"`). Default: never, so the value is written again.
    fn scalar_reads_as(&self, written: &str, value: &Value) -> bool {
        let _ = (written, value);
        false
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

    /// Where the number literals the editor cannot hold exactly (integers
    /// past 64 bits, floats past the largest) that the language allows are
    /// written in `source`, when `error` (what [`parse`](Self::parse)
    /// returned for it) is only about one of them: the text then saves in
    /// this language, the problem still shown. The places hold the one the
    /// error is about, and as many others as the language finds in one
    /// pass (a save reads the text again with all of them stood in for,
    /// not once per number). Default: none, every error holds the save.
    fn unheld_numbers(&self, source: &str, error: &ParseError) -> Vec<std::ops::Range<usize>> {
        let _ = (source, error);
        Vec::new()
    }

    /// `value` written in place of `written` (a one-line container being
    /// edited), in its style: compact JSON stays compact.
    fn format_value_like(
        &self,
        language: &dyn ConfigLanguage,
        value: &Value,
        written: &str,
    ) -> String {
        let _ = written;
        ConfigLanguage::format_value(language, value)
    }

    /// `value` written over `source[range]`, a value the host wrote there,
    /// as [`format_value_like`](Self::format_value_like) does; the text before
    /// it on its line places it (a YAML block scalar's indentation indicator
    /// counts from its key or dash).
    fn format_value_at(
        &self,
        language: &dyn ConfigLanguage,
        value: &Value,
        source: &str,
        range: std::ops::Range<usize>,
    ) -> String {
        SaveLayout::format_value_like(language.layout(), language, value, &source[range])
    }
}

/// A markup language (XML): elements between tags, attributes, text beside
/// child elements, and what its data model reads otherwise than it writes.
pub trait Markup {
    /// The characters the language's markup is written between (XML's `<`
    /// and `>`): a lock lets an edit that leaves the text unparsed add the
    /// one that ends markup a line left open.
    fn delimiters(&self) -> (char, char);

    /// The length in bytes of markup at the start of `text` that closes an
    /// element, such as an XML end tag. The editor remembers the ones it
    /// writes, and Enter after a value typed before one steps past it.
    fn closing_markup_len(&self, text: &str) -> Option<usize> {
        let _ = text;
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
    /// [`one_line_container`]: SaveLayout::one_line_container
    fn writes_container_on_line(&self, line: &str) -> bool {
        let _ = line;
        false
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

    /// Why the data model reads `source` with keys merged that it writes
    /// apart (XML elements of one parent with the same local name in
    /// different namespaces), ending with `(at /pointer)`: parsing refuses
    /// such a document, as converting it does. Default: `None`.
    fn merged_keys(&self, source: &str) -> Option<String> {
        let _ = source;
        None
    }

    /// Why `source` does not convert to another language although it parses
    /// (XML elements of one parent with the same local name in different
    /// namespaces, which the document model would merge; an entity
    /// reference only the document's own declarations resolve),
    /// ending with `(at /pointer)`; `None` when nothing stands in the way.
    /// An Edit-as save whose host text this refuses waits.
    fn unconvertible(&self, source: &str) -> Option<String> {
        let _ = source;
        None
    }

    /// The property a schema's key `name` names, as the language's text
    /// spells it (XML keys an attribute `a` beside a child element `a` as
    /// `@a`, and writes it `a`): what a fill writes for a `required`
    /// entry, and what a problem with the key calls it. Default: `name`
    /// itself.
    fn required_property<'a>(&self, name: &'a str) -> &'a str {
        name
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

    /// Whether the entries of `source` hold what no other language shows
    /// (an XML element's `xsi:type`, its namespace declaration, its
    /// name's prefix, which of its keys are attributes), so an Edit-as
    /// save over it reads which list item or map entry is which from the
    /// user's edits (`item_sources`), not only from values that read
    /// alike. Default: no, values tell.
    fn items_hold_unshown(&self, source: &str) -> bool {
        let _ = source;
        false
    }

    /// Whether `source` holds a processing instruction other than an XML
    /// declaration (`<?xml-stylesheet …?>`), which [`has_comments`]
    /// counts and the Edit-as notice names. Default: none.
    ///
    /// [`has_comments`]: Text::has_comments
    fn has_processing_instructions(&self, source: &str) -> bool {
        let _ = source;
        false
    }
}

/// Values a language writes once and reads in several places (YAML anchors,
/// aliases and merge keys), or writes with more than their data (YAML tags,
/// HCL expressions).
pub trait Sharing {
    /// `previous` with what it writes once and reads in several places (YAML
    /// aliases and merge keys) written out in place wherever `new` gives that
    /// place a value of its own, so a save can patch the rest; `None` when
    /// nothing needs it. The text must read as `old` still.
    fn detach_shared(&self, previous: &str, old: &Value, new: &Value) -> Option<String> {
        let _ = (previous, old, new);
        None
    }

    /// Where the field at `path` is written when `source` writes it once
    /// and reads it in several places: the path under a YAML anchor
    /// that an alias (`c: *x` reads `/c/b` from `/a/b` of `a: &x`) or a
    /// merge (`<<: *x`) reads it from. `None` (the default) where it reads
    /// through nothing so written. `rangeOf` finds such a field there.
    fn written_at(&self, source: &str, path: &FieldPath) -> Option<FieldPath> {
        let _ = (source, path);
        None
    }

    /// `saved` (an Edit-as save's text) with each value a host text of the
    /// session `origin` (its first, or the host's last) wrote as a
    /// reference to a value written once (a YAML alias `*p`), that `saved`
    /// writes out while it writes that value again before it (an anchor
    /// cut, saved, and pasted back; a reference moved with its anchor),
    /// written as that reference again; `None` (the default) when there
    /// is none. The text must read as `saved` does. `placed`: the items of
    /// `saved` the edit's links tell came from `origin`, each as its
    /// pointer in `saved` and its pointer in `origin` (a value under one
    /// of them comes from under its source); every other value stands for
    /// the one at its own pointer.
    fn references_back(
        &self,
        saved: &str,
        origin: &str,
        placed: &[(String, String)],
    ) -> Option<String> {
        let _ = (saved, origin, placed);
        None
    }

    /// The length of a value written as a reference to another (YAML's
    /// `*anchor`) at the start of `text`, which a save keeps as written
    /// while the value it stands for is unchanged. Default: none.
    fn reference_len(&self, text: &str) -> Option<usize> {
        let _ = text;
        None
    }

    /// The length of what may precede a value without being part of it
    /// (YAML's `&anchor `, `!!tag `), so a flow collection after it is found.
    fn value_properties_len(&self, text: &str) -> usize {
        let _ = text;
        0
    }

    /// The JSON pointer of each value `source` writes with a tag (YAML
    /// `!Sub x`, `!include`), with the tag: the data holds the value under
    /// it, and an Edit-as save keeps each one or waits. Empty by default.
    fn tags(&self, source: &str) -> Vec<(String, String)> {
        let _ = source;
        Vec::new()
    }

    /// Whether `code` (a line without its comment, trimmed) opens a map
    /// written inline over the following lines (YAML's `tcp: {`), which a
    /// save keeps as written rather than the serializer's form. Default: no.
    fn flow_map_opener(&self, code: &str) -> bool {
        let _ = code;
        false
    }

    /// Whether `code` (a line without its comment, trimmed) marks the
    /// document's start or describes the document rather than holding a
    /// value (YAML's `---` and `%YAML` directives). Default: no such lines.
    fn document_marker(&self, code: &str) -> bool {
        let _ = code;
        false
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
}

/// A language that writes lists by repeating their key (XML sibling
/// elements, HCL blocks). A single entry then reads back as a bare value, so
/// where the schema expects a list it is read as a one-item list before
/// validating or converting, and a list key already present is offered
/// again.
pub trait Repeats {
    /// What the language repeats.
    fn repeated(&self) -> Repeated;

    /// Whether the field at `path` is written as one of what a list of
    /// it repeats, written once (an XML element, an HCL block): it is then
    /// also that list's only item, so `path/0` finds it. Never for an
    /// index, an attribute, an object of labelled blocks or the document.
    fn lone_item(&self, source: &str, path: &FieldPath) -> bool;
}

/// The defaults of every refinement group, for a language that has none.
struct Defaults;

impl Text for Defaults {}
impl Typing for Defaults {}
impl Hover for Defaults {}
impl SaveLayout for Defaults {}

/// Whether a hook's length of what starts `text` ends inside it, on a
/// character boundary: a custom language's longer length, or one inside a
/// character, is no markup the editor can cut, so it counts as none.
fn is_prefix(text: &str, length: usize) -> bool {
    text.is_char_boundary(length)
}

/// The hooks of every group, read through the language: each from the
/// group the language refines, else its default.
impl dyn ConfigLanguage {
    /// See [`LanguageTraits::writes_non_finite`].
    pub fn writes_non_finite(&self) -> bool {
        self.config().traits.writes_non_finite
    }

    /// See [`LanguageTraits::one_number_type`].
    pub fn one_number_type(&self) -> bool {
        self.config().traits.one_number_type
    }

    /// See [`LanguageTraits::indent_is_structure`].
    pub fn indent_is_structure(&self) -> bool {
        self.config().traits.indent_is_structure
    }

    /// See [`LanguageTraits::plain_scalars`].
    pub fn plain_scalars(&self) -> bool {
        self.config().traits.plain_scalars
    }

    /// See [`LanguageTraits::scalars_are_text`].
    pub fn scalars_are_text(&self) -> bool {
        self.config().traits.scalars_are_text
    }

    /// See [`Text::first_span`].
    pub fn first_span(&self, source: &str) -> Option<Span> {
        self.text().first_span(source)
    }

    /// See [`Text::lexical_state`].
    pub fn lexical_state(&self, source: &str, cursor: usize) -> Lexical {
        self.text().lexical_state(self, source, cursor)
    }

    /// See [`Text::comment_start`].
    pub fn comment_start(&self, line: &str) -> Option<usize> {
        self.text().comment_start(self, line)
    }

    /// See [`Text::comments`].
    pub fn comments(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        self.text().comments(self, source)
    }

    /// See [`Text::has_comments`].
    pub fn has_comments(&self, source: &str) -> bool {
        self.text().has_comments(self, source)
    }

    /// See [`Text::string_body_lines`].
    pub fn string_body_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        self.text().string_body_lines(source)
    }

    /// See [`Text::saved_string_lines`].
    pub fn saved_string_lines(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        self.text().saved_string_lines(self, source)
    }

    /// See [`Text::multiline_strings`].
    pub fn multiline_strings(&self, source: &str) -> Vec<std::ops::Range<usize>> {
        self.text().multiline_strings(source)
    }

    /// See [`Text::multiline_constructs`].
    pub fn multiline_constructs(&self, source: &str) -> Vec<MultilineConstruct> {
        self.text().multiline_constructs(self, source)
    }

    /// See [`Text::highlight_continued`].
    pub fn highlight_continued(&self, text: &str) -> Vec<Token> {
        self.text().highlight_continued(self, text)
    }

    /// See [`Text::fold_ranges`].
    pub fn fold_ranges(&self, source: &str) -> Vec<FoldRange> {
        self.text().fold_ranges(self, source)
    }

    /// See [`Text::value_ranges`].
    pub fn value_ranges(&self, source: &str, path: &FieldPath) -> Vec<std::ops::Range<usize>> {
        self.text().value_ranges(self, source, path)
    }

    /// See [`Text::unclosed_containers`].
    pub fn unclosed_containers(&self, text: &str) -> usize {
        self.text().unclosed_containers(self, text)
    }

    /// See [`Text::closing_line`].
    pub fn closing_line(&self, lines: &[&str], (first, last): (usize, usize)) -> usize {
        self.text().closing_line(self, lines, (first, last))
    }

    /// See [`Text::container_closers`].
    pub fn container_closers(&self, text: &str) -> Vec<String> {
        self.text().container_closers(self, text)
    }

    /// See [`Text::container_closer_len`].
    pub fn container_closer_len(&self, text: &str) -> Option<usize> {
        self.text()
            .container_closer_len(self, text)
            .filter(|length| is_prefix(text, *length))
    }

    /// See [`Typing::newline`].
    pub fn newline(&self, source: &str, cursor: usize) -> Newline {
        self.typing().newline(self, source, cursor)
    }

    /// See [`Typing::whole_constructs`].
    pub fn whole_constructs(&self, source: &str, start: usize, end: usize) -> (usize, usize) {
        self.typing().whole_constructs(self, source, start, end)
    }

    /// See [`Typing::toggle_comments`].
    pub fn toggle_comments(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<Toggled> {
        self.typing()
            .toggle_comments(self, source, selection_start, selection_end)
    }

    /// [`Self::toggle_comments`] made: the text after and the selection
    /// (UTF-16 offsets of it). Panics when the edits do not fit `source`.
    #[cfg(test)]
    pub(crate) fn toggled_text(
        &self,
        source: &str,
        selection_start: usize,
        selection_end: usize,
    ) -> Option<(String, usize, usize)> {
        let toggled = self.toggle_comments(source, selection_start, selection_end)?;
        let text = toggled
            .apply(source)
            .expect("Ctrl+/ edits sorted, apart and inside the text");
        Some((text, toggled.selection.0, toggled.selection.1))
    }

    /// See [`Typing::comment_scope`].
    pub fn comment_scope(&self, source: &str, start: usize, end: usize) -> (usize, usize) {
        self.typing().comment_scope(source, start, end)
    }

    /// See [`Typing::auto_close`].
    pub fn auto_close(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        self.typing().auto_close(source, cursor, typed)
    }

    /// See [`Typing::complete_on_type`].
    pub fn complete_on_type(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        self.typing().complete_on_type(source, cursor, typed)
    }

    /// See [`Typing::reindent_on_type`].
    pub fn reindent_on_type(&self, source: &str, cursor: usize, typed: char) -> Option<String> {
        self.typing().reindent_on_type(source, cursor, typed)
    }

    /// See [`Typing::entry_started`].
    pub fn entry_started(&self, source: &str, cursor: usize) -> bool {
        self.typing().entry_started(source, cursor)
    }

    /// See [`Typing::value_follows_key`].
    pub fn value_follows_key(&self, rest: &str) -> bool {
        self.typing().value_follows_key(self, rest)
    }

    /// See [`Typing::empty_value_slots`].
    pub fn empty_value_slots(&self, text: &str) -> Vec<usize> {
        self.typing().empty_value_slots(self, text)
    }

    /// See [`Typing::rewritable_keys`].
    pub fn rewritable_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> HashSet<String> {
        self.typing().rewritable_keys(source, cursor_byte, path)
    }

    /// See [`Hover::hover_path`].
    pub fn hover_path(
        &self,
        source: &str,
        line_index: usize,
        column: usize,
    ) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
        self.hover().hover_path(source, line_index, column)
    }

    /// See [`Hover::hover_cursor`].
    pub fn hover_cursor(&self, line: &str, column: usize) -> usize {
        self.hover().hover_cursor(line, column)
    }

    /// See [`Hover::item_start`].
    pub fn item_start(
        &self,
        source: &str,
        line_start: usize,
        line: &str,
        column: usize,
    ) -> Option<usize> {
        self.hover()
            .item_start(self, source, line_start, line, column)
    }

    /// See [`SaveLayout::comparable_line`].
    pub fn comparable_line<'a>(&self, code: &'a str) -> std::borrow::Cow<'a, str> {
        self.layout().comparable_line(code)
    }

    /// See [`SaveLayout::members_in_order`].
    pub fn members_in_order(
        &self,
        source: &str,
        path: &FieldPath,
        order: &[String],
    ) -> Option<String> {
        self.layout().members_in_order(source, path, order)
    }

    /// See [`SaveLayout::entries_apart`].
    pub fn entries_apart(&self, source: &str) -> Option<String> {
        self.layout().entries_apart(source)
    }

    /// See [`SaveLayout::entries_together`].
    pub fn entries_together(&self, patched: &str, previous: &str) -> String {
        self.layout().entries_together(patched, previous)
    }

    /// See [`SaveLayout::aligned_lines`].
    pub fn aligned_lines(&self, text: &str) -> Vec<usize> {
        self.layout().aligned_lines(text)
    }

    /// See [`SaveLayout::kept_value_end`].
    pub fn kept_value_end(&self, source: &str, end: usize) -> usize {
        self.layout().kept_value_end(source, end)
    }

    /// See [`SaveLayout::written_value_range`].
    pub fn written_value_range(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
    ) -> std::ops::Range<usize> {
        self.layout().written_value_range(source, range)
    }

    /// See [`SaveLayout::writes_as_labels`].
    pub fn writes_as_labels(&self, source: &str, path: &FieldPath) -> bool {
        self.layout().writes_as_labels(source, path)
    }

    /// See [`SaveLayout::one_line_container`].
    pub fn one_line_container(&self, source: &str, path: &FieldPath) -> Option<OneLine> {
        self.layout().one_line_container(source, path)
    }

    /// See [`SaveLayout::key_renames`].
    pub fn key_renames(
        &self,
        source: &str,
        path: &FieldPath,
        new_key: &str,
    ) -> Option<Vec<KeyEdit>> {
        self.layout().key_renames(source, path, new_key)
    }

    /// See [`SaveLayout::written_key`].
    pub fn written_key(&self, source: &str, path: &FieldPath, key: &str) -> Option<String> {
        self.layout().written_key(self, source, path, key)
    }

    /// See [`SaveLayout::scalar_edit`].
    pub fn scalar_edit(
        &self,
        previous: &str,
        path: &FieldPath,
        value: &Value,
    ) -> Result<Option<Value>, String> {
        self.layout().scalar_edit(previous, path, value)
    }

    /// See [`SaveLayout::changed_form`].
    pub fn changed_form(
        &self,
        previous: &str,
        patched: &str,
        told: SaveTold<'_>,
    ) -> Option<String> {
        self.layout().changed_form(previous, patched, told)
    }

    /// See [`SaveLayout::changed_form_reason`].
    pub fn changed_form_reason(&self, value: &Value) -> Option<String> {
        self.layout().changed_form_reason(value)
    }

    /// See [`SaveLayout::starts_section`].
    pub fn starts_section(&self, line: &str) -> bool {
        self.layout().starts_section(line)
    }

    /// See [`SaveLayout::opens_list_item`].
    pub fn opens_list_item(&self, lines: &[&str], index: usize) -> bool {
        self.layout().opens_list_item(self, lines, index)
    }

    /// See [`SaveLayout::item_lines`].
    pub fn item_lines(&self, source: &str, path: &FieldPath) -> Option<std::ops::Range<usize>> {
        self.layout().item_lines(source, path)
    }

    /// See [`SaveLayout::number_in_form`].
    pub fn number_in_form(&self, written: &str, value: &serde_json::Number) -> Option<String> {
        self.layout().number_in_form(written, value)
    }

    /// See [`SaveLayout::scalar_reads_as`].
    pub fn scalar_reads_as(&self, written: &str, value: &Value) -> bool {
        self.layout().scalar_reads_as(written, value)
    }

    /// See [`SaveLayout::edits_one_scalar`].
    pub fn edits_one_scalar(
        &self,
        source: &str,
        range: std::ops::Range<usize>,
        inserted: &str,
    ) -> bool {
        self.layout().edits_one_scalar(source, range, inserted)
    }

    /// See [`SaveLayout::unheld_numbers`].
    pub fn unheld_numbers(&self, source: &str, error: &ParseError) -> Vec<std::ops::Range<usize>> {
        self.layout().unheld_numbers(source, error)
    }

    /// See [`Markup::delimiters`]; `None` without [`Markup`].
    pub fn markup_delimiters(&self) -> Option<(char, char)> {
        self.markup().map(Markup::delimiters)
    }

    /// See [`Markup::closing_markup_len`].
    pub fn closing_markup_len(&self, text: &str) -> Option<usize> {
        self.markup()
            .and_then(|group| group.closing_markup_len(text))
            .filter(|length| is_prefix(text, *length))
    }

    /// See [`Markup::key_markup_len`].
    pub fn key_markup_len(&self, text: &str) -> Option<usize> {
        self.markup()
            .and_then(|group| group.key_markup_len(text))
            .filter(|length| is_prefix(text, *length))
    }

    /// See [`Markup::element_end`].
    pub fn element_end(&self, text: &str, open: usize) -> Option<usize> {
        self.markup()
            .and_then(|group| group.element_end(text, open))
    }

    /// See [`Markup::content_texts`].
    pub fn content_texts(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        self.markup()
            .map(|group| group.content_texts(text))
            .unwrap_or_default()
    }

    /// See [`Markup::space_is_content`].
    pub fn space_is_content(&self, before: &str, after: &str) -> bool {
        self.markup()
            .is_some_and(|group| group.space_is_content(before, after))
    }

    /// See [`Markup::writes_container_on_line`].
    pub fn writes_container_on_line(&self, line: &str) -> bool {
        self.markup()
            .is_some_and(|group| group.writes_container_on_line(line))
    }

    /// See [`Markup::text_key`].
    pub fn text_key(&self) -> Option<&'static str> {
        self.markup().and_then(Markup::text_key)
    }

    /// See [`Markup::mixed_content`].
    pub fn mixed_content(&self, source: &str) -> Option<String> {
        self.markup().and_then(|group| group.mixed_content(source))
    }

    /// See [`Markup::merged_keys`].
    pub fn merged_keys(&self, source: &str) -> Option<String> {
        self.markup().and_then(|group| group.merged_keys(source))
    }

    /// See [`Markup::unconvertible`].
    pub fn unconvertible(&self, source: &str) -> Option<String> {
        self.markup().and_then(|group| group.unconvertible(source))
    }

    /// See [`Markup::required_property`].
    pub fn required_property<'a>(&self, name: &'a str) -> &'a str {
        self.markup()
            .map_or(name, |group| group.required_property(name))
    }

    /// See [`Markup::spelled_like`].
    pub fn spelled_like(&self, source: &str, read: Value, like: &Value) -> Value {
        match self.markup() {
            Some(group) => group.spelled_like(source, read, like),
            None => read,
        }
    }

    /// See [`Markup::items_hold_unshown`].
    pub fn items_hold_unshown(&self, source: &str) -> bool {
        self.markup()
            .is_some_and(|group| group.items_hold_unshown(source))
    }

    /// See [`Markup::has_processing_instructions`].
    pub fn has_processing_instructions(&self, source: &str) -> bool {
        self.markup()
            .is_some_and(|group| group.has_processing_instructions(source))
    }

    /// See [`Sharing::detach_shared`].
    pub fn detach_shared(&self, previous: &str, old: &Value, new: &Value) -> Option<String> {
        self.sharing()
            .and_then(|group| group.detach_shared(previous, old, new))
    }

    /// See [`Sharing::written_at`].
    pub fn written_at(&self, source: &str, path: &FieldPath) -> Option<FieldPath> {
        self.sharing()
            .and_then(|group| group.written_at(source, path))
    }

    /// See [`Sharing::references_back`].
    pub fn references_back(
        &self,
        saved: &str,
        origin: &str,
        placed: &[(String, String)],
    ) -> Option<String> {
        self.sharing()
            .and_then(|group| group.references_back(saved, origin, placed))
    }

    /// See [`Sharing::reference_len`].
    pub fn reference_len(&self, text: &str) -> Option<usize> {
        self.sharing().and_then(|group| group.reference_len(text))
    }

    /// See [`Sharing::value_properties_len`].
    pub fn value_properties_len(&self, text: &str) -> usize {
        self.sharing()
            .map_or_else(|| 0, |group| group.value_properties_len(text))
    }

    /// See [`Sharing::tags`].
    pub fn tags(&self, source: &str) -> Vec<(String, String)> {
        self.sharing()
            .map(|group| group.tags(source))
            .unwrap_or_default()
    }

    /// See [`Sharing::flow_map_opener`].
    pub fn flow_map_opener(&self, code: &str) -> bool {
        self.sharing()
            .is_some_and(|group| group.flow_map_opener(code))
    }

    /// See [`Sharing::document_marker`].
    pub fn document_marker(&self, code: &str) -> bool {
        self.sharing()
            .is_some_and(|group| group.document_marker(code))
    }

    /// See [`Sharing::expressions`].
    pub fn expressions(&self, source: &str) -> Vec<String> {
        self.sharing()
            .map(|group| group.expressions(source))
            .unwrap_or_default()
    }

    /// What the language repeats to write a list (see [`Repeats`]), or
    /// [`Repetition::None`].
    pub fn repetition(&self) -> Repetition {
        match self.repeats().map(Repeats::repeated) {
            Some(Repeated::Elements) => Repetition::Elements,
            Some(Repeated::Blocks) => Repetition::Blocks,
            None => Repetition::None,
        }
    }

    /// See [`Repeats::lone_item`]; never without [`Repeats`].
    pub fn lone_item(&self, source: &str, path: &FieldPath) -> bool {
        self.repeats()
            .is_some_and(|group| group.lone_item(source, path))
    }
}

/// `key` as `language` writes it (see [`SaveLayout::written_key`]): the
/// text its serializer writes for it in a document holding only that key,
/// when that document reads back with the key.
fn key_as_written(language: &dyn ConfigLanguage, key: &str) -> Option<String> {
    let path = FieldPath::root().key(key);
    [Value::from(0), Value::from("")]
        .into_iter()
        .find_map(|value| {
            let document = Value::Object([(key.to_string(), value)].into_iter().collect());
            let text = language.write_document(&document, None).ok()?;
            let read = language.parse(&text).ok()?;
            if read.as_object()?.keys().ne([key]) {
                return None;
            }
            let span = language.key_span(&text, &path)?;
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
/// [`SaveLayout::one_line_container`]).
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

/// The line comments of `source` (see [`Text::comments`]).
pub(crate) fn line_comments(
    behavior: &dyn ConfigLanguage,
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
/// [`Text::comment_start`].
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
/// describes separators, brackets and the item separator: a space typed
/// after such a value ends the entry instead of opening values.
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
/// [`Typing::empty_value_slots`].
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
    behavior: &dyn ConfigLanguage,
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
    let line_of = |utf16: usize| {
        super::paths::line_of_byte(
            source,
            super::text::text_size(utf16_to_byte_index(source, utf16)),
        )
    };
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
    let line_start = |line: usize| usize::from(super::paths::line_start(source, line));
    let last_end = line_start(last) + super::paths::split_line(source, last).map_or(0, str::len);
    (
        super::paths::api_units(source, line_start(first)),
        super::paths::api_units(source, last_end),
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
    fn new(behavior: &dyn ConfigLanguage, source: &str) -> Self {
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

        fn text(&self) -> &dyn super::Text {
            self
        }
    }

    impl super::Text for Tiny {
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
        let source = "a = 1\nname = '''multi\n# not a comment\nhost = x\nend'''\nb = 2\n";
        let commented =
            "a = 1\n# name = '''multi\n# # not a comment\n# host = x\n# end'''\nb = 2\n";
        let mut failures = Vec::new();
        for marker in ["name = '''", "# not", "host = x", "end'''"] {
            let at = source.find(marker).unwrap() + 2;
            let toggled = (&Tiny as &dyn super::ConfigLanguage)
                .toggled_text(source, at, at)
                .map(|(text, _, _)| text);
            if toggled.as_deref() != Some(commented) {
                failures.push(format!("caret in {marker:?}: {toggled:?}"));
            }
        }
        let outside = source.find("b = 2").unwrap();
        let toggled = (&Tiny as &dyn super::ConfigLanguage)
            .toggled_text(source, outside, outside)
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
