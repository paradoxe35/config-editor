//! Paths and places in the text, both ways: where a [`FieldPath`] is written
//! (`field_range`, `problem_span`) and which path the caret is on
//! (`path_at`). Built on each language's span and context lookups
//! (`key_span`, `value_span`, `hover_path`, `completion_context`), so a
//! language gets them by implementing those.

mod fields;
mod lines;
mod span_memo;
mod types;
mod walk;

pub(crate) use fields::fields;
#[cfg(any(feature = "yaml", feature = "toml"))]
pub(crate) use lines::text_line;
pub(crate) use lines::{
    Lines, byte_at, line_column, line_of_byte, line_start, lines, offset_of, split_line, units_at,
};
#[cfg(any(feature = "toml", feature = "hcl", feature = "xml"))]
pub(crate) use span_memo::batch_cached_as;
pub(crate) use span_memo::in_batch;
pub(crate) use span_memo::{KeptSpans, with_spans};
pub(crate) use span_memo::{batch_cached, batch_kept};
pub(crate) use span_memo::{key_span as memo_key_span, value_span as memo_value_span};
pub use types::{FieldPath, TextPosition, TextRange};
#[cfg(any(feature = "yaml", feature = "json", feature = "toml", feature = "xml"))]
pub(crate) use walk::Checkpoints;

use super::codec::{inline_location, written_inline};
use super::completion::{hovered_key, path_with_key};
use super::keyboard::utf16_to_byte_index;
use super::language::{ConfigLanguage, Repetition, Span, pointer_index};
use super::work_count::{Work, note};

/// `pointer`, and for languages that repeat elements or blocks, the pointer
/// written: without the `/0` after each field the language writes as a lone
/// item (`ConfigLanguage::lone_item`: an XML element or HCL block written
/// once, read as a one-item list too). An index anywhere else (on a scalar,
/// an attribute, an object of labelled blocks, another index, the document)
/// is only an index. A list written as one (`ea = []`) has no lone item: its
/// `/0` is only its first item.
fn spellings(behavior: &dyn ConfigLanguage, source: &str, pointer: &str) -> Vec<String> {
    let mut spellings = vec![pointer.to_string()];
    if let Some(lone) = lone_spelling(behavior, source, pointer) {
        spellings.push(lone);
    }
    spellings
}

/// [`spellings`]' second one, when it differs from `pointer`.
pub(crate) fn lone_spelling(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
) -> Option<String> {
    if behavior.repetition() == Repetition::None || !pointer.contains("/0") {
        return None;
    }
    let mut lone = String::new();
    // The item just left out: an index on it names nothing (an element or
    // block is no list).
    let mut left_out = false;
    for segment in pointer.split('/').skip(1) {
        if left_out && pointer_index(segment).is_some() {
            return None;
        }
        if segment == "0" && !lone.is_empty() {
            if memo_value_span(behavior, source, &lone)
                .is_some_and(|span| written_inline(behavior, source, span))
            {
                return None;
            }
            if behavior.lone_item(source, &lone) {
                left_out = true;
                continue;
            }
        }
        left_out = false;
        lone.push('/');
        lone.push_str(segment);
    }
    (lone != pointer).then_some(lone)
}

/// The key written for `pointer`, inside an inline container or not.
fn key_span(behavior: &dyn ConfigLanguage, source: &str, pointer: &str) -> Option<Span> {
    inline_location(behavior, source, pointer, true)
        .or_else(|| memo_key_span(behavior, source, pointer))
}

/// The value written for `pointer`, inside an inline container or not.
fn value_span(behavior: &dyn ConfigLanguage, source: &str, pointer: &str) -> Option<Span> {
    inline_location(behavior, source, pointer, false)
        .or_else(|| memo_value_span(behavior, source, pointer))
}

/// Characters of line `line` (one-based) up to its last non-blank one.
fn trimmed_width(source: &str, line: usize) -> usize {
    split_line(source, line.saturating_sub(1)).map_or(0, |text| text.trim_end().chars().count())
}

fn range(span: Span) -> TextRange {
    TextRange::new(
        TextPosition::new(span.line, span.start_column),
        TextPosition::new(span.line, span.end_column),
    )
}

/// Where the field at `pointer` is written: its key and value when they
/// share a line (`listen: 7835`), the key alone when the value is a block
/// below it, the value alone for an array item, the dash or header of an
/// item holding a block. The root is the whole text.
pub(crate) fn field_range(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
) -> Option<TextRange> {
    if pointer.is_empty() {
        let lines = source.split('\n').count();
        let last = source.split('\n').next_back().unwrap_or("");
        return Some(TextRange::new(
            TextPosition::new(1, 1),
            TextPosition::new(lines, last.chars().count() + 1),
        ));
    }
    spellings(behavior, source, pointer)
        .iter()
        .find_map(|pointer| field_span(behavior, source, pointer))
        .or_else(|| first_repeated(behavior, source, pointer).map(range))
        // Read through a shared value (a YAML alias): where that is written.
        .or_else(|| {
            let written = behavior.written_at(source, pointer)?;
            (written != pointer).then(|| field_range(behavior, source, &written))?
        })
}

/// A list written as repeated elements or blocks (`<hosts>` twice) has no
/// key of its own: its first item's stands for it.
fn first_repeated(behavior: &dyn ConfigLanguage, source: &str, pointer: &str) -> Option<Span> {
    (behavior.repetition() != Repetition::None)
        .then(|| key_span(behavior, source, &format!("{pointer}/0")))
        .flatten()
}

/// Where the key of the field at `pointer` is written, in any spelling of
/// the pointer; `None` for an array item or a field not written. (Locks
/// cover keys with it.)
#[cfg(feature = "protect")]
pub(crate) fn key_range(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
) -> Option<TextRange> {
    spellings(behavior, source, pointer)
        .iter()
        .find_map(|pointer| key_span(behavior, source, pointer))
        .map(range)
}

/// [`field_range`] of one spelling of a pointer.
fn field_span(behavior: &dyn ConfigLanguage, source: &str, pointer: &str) -> Option<TextRange> {
    let key = key_span(behavior, source, pointer);
    let value = value_span(behavior, source, pointer);
    let Some(key) = key else {
        return value.map(|value| spanning(source, value));
    };
    let width = trimmed_width(source, key.line);
    let Some(value) = value.filter(|value| {
        value.line == key.line
            && value.start_column >= key.start_column
            && value.start_column <= width
    }) else {
        // A key over lines (a YAML block scalar key, `? |`) ends where it
        // ends.
        return Some(spanning(source, key));
    };
    let line: Vec<char> = split_line(source, key.line - 1)
        .unwrap_or("")
        .chars()
        .collect();
    let text = |from: usize, to: usize| -> String {
        line[from.min(line.len())..to.min(line.len())]
            .iter()
            .collect()
    };
    // A container opened on the key's line (`"web": {`, `hosts = [`) is a
    // block below it.
    let written = text(value.start_column - 1, value.end_column - 1);
    let mut opener = written.trim().chars();
    if let (Some(open), None) = (opener.next(), opener.next())
        && behavior
            .config()
            .brackets
            .iter()
            .any(|(bracket, _)| *bracket == open)
    {
        return Some(range(key));
    }
    let start = key.start_column;
    // A value over lines (a block scalar, a heredoc, element text) ends on
    // the line it ends on.
    let end = if value.end_column > width + 1 {
        spanning(source, value).end
    } else {
        TextPosition::new(key.line, value.end_column.max(key.end_column))
    };
    // Markup closing the value (`</listen>`) is part of the field, and so
    // is the start tag's `<`.
    let end_line: Vec<char> = split_line(source, end.line - 1)
        .unwrap_or("")
        .chars()
        .collect();
    let rest: String = end_line[(end.column - 1).min(end_line.len())..]
        .iter()
        .collect();
    match behavior.closing_markup_len(&rest) {
        Some(closing) => {
            // The markup opening the key (`<` of `<listen>`) too.
            let line_text: String = line.iter().collect();
            let name = line_text
                .char_indices()
                .nth(start - 1)
                .map_or(line_text.len(), |(byte, _)| byte);
            let start = match super::flow_syntax::markup_before(behavior, &line_text, name) {
                Some(open) => start - line_text[open..name].chars().count(),
                None => start,
            };
            Some(TextRange::new(
                TextPosition::new(key.line, start),
                TextPosition::new(
                    end.line,
                    end.column + rest[..closing.min(rest.len())].chars().count(),
                ),
            ))
        }
        None => Some(TextRange::new(TextPosition::new(key.line, start), end)),
    }
}

/// `span` over the lines it covers: its end column counts on past its line
/// (each line break one column) to where it ends.
fn spanning(source: &str, span: Span) -> TextRange {
    let start = TextPosition::new(span.line, span.start_column);
    let (mut line, mut column) = (span.line, span.end_column);
    while let Some(text) = split_line(source, line - 1) {
        let width = text.chars().count();
        if column <= width + 1 || split_line(source, line).is_none() {
            break;
        }
        column -= width + 1;
        line += 1;
    }
    // Through a line break (a heredoc's terminator line): the end of that
    // line.
    if column == 1 && line > span.line {
        line -= 1;
        column = split_line(source, line - 1).map_or(0, |text| text.chars().count()) + 1;
    }
    TextRange::new(start, TextPosition::new(line, column))
}

/// Where a problem with the value at `pointer` is underlined, as schema
/// diagnostics are: the value, or its key when the value is `written_as_block`
/// below it (a block has no text on the key's line); a `required` problem
/// (a missing property) on the object's key.
pub(crate) fn problem_span(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
    required: bool,
    written_as_block: bool,
) -> Option<Span> {
    let locate = |path: &str| {
        if required {
            inline_location(behavior, source, path, true)
                .or_else(|| memo_key_span(behavior, source, path))
                .or_else(|| memo_value_span(behavior, source, path))
        } else {
            inline_location(behavior, source, path, false)
                .or_else(|| {
                    memo_value_span(behavior, source, path).filter(|span| {
                        !written_as_block || span.start_column <= trimmed_width(source, span.line)
                    })
                })
                .or_else(|| memo_key_span(behavior, source, path))
                .or_else(|| memo_value_span(behavior, source, path))
        }
    };
    spellings(behavior, source, pointer)
        .iter()
        .find_map(|pointer| locate(pointer))
        .or_else(|| first_repeated(behavior, source, pointer))
}

#[cfg(any(feature = "protect", feature = "schema-validation"))]
/// Whether two pointers name the same field, in languages that write a
/// lone list item without its `/0` (XML, HCL blocks) or with it.
pub(crate) fn same_field(one: &str, other: &str) -> bool {
    let plain = |pointer: &str| {
        let lone = format!("{pointer}/").replace("/0/", "/");
        lone.trim_end_matches('/').to_string()
    };
    one == other || plain(one) == plain(other)
}

/// Where the closing markup (an XML end tag) the caret at byte `at` of
/// `line` is inside starts, looking back a bounded distance.
fn closing_markup_around(behavior: &dyn ConfigLanguage, line: &str, at: usize) -> Option<usize> {
    line[..at]
        .char_indices()
        .rev()
        .take(256)
        .map(|(start, _)| start)
        .find(|start| {
            behavior
                .closing_markup_len(&line[*start..])
                .is_some_and(|length| *start + 1 < at && at <= *start + length)
        })
}

/// The path of what the caret at UTF-16 `cursor` is on: the key under or just
/// before it, else the value or container holding it (the root at the top
/// level). `None` past the end of the text.
pub(crate) fn path_at(
    behavior: &dyn ConfigLanguage,
    source: &str,
    cursor: usize,
) -> Option<FieldPath> {
    // A leading byte order mark is no part of the document.
    if let (text, true) = super::byte_order_mark::split(source) {
        return path_at(behavior, text, cursor.saturating_sub(1));
    }
    // Inside a batch the walks from the top are counted where they start,
    // each language's once for the text (see `walked_from_top`).
    if !in_batch(source) {
        note(Work::PathLookup);
    }
    if cursor > units_at(source, source.len()) {
        return None;
    }
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line_index = line_column(source, line_start).0 - 1;
    let column = source[line_start..byte].chars().count();
    let line = source[line_start..].split('\n').next().unwrap_or("");
    // In markup closing a value (`<a>1</a|>`): the value it closes.
    let in_line = byte - line_start;
    if let Some(open) = closing_markup_around(behavior, line, in_line) {
        let value_end = units_at(source, line_start + open);
        return path_at(behavior, source, value_end);
    }
    // On the key, just past it, past it and its separator (`listen =| 1`),
    // or before the markup that opens it (`|<listen>`).
    let separators: String = behavior.config().separators.concat();
    let before: Vec<char> = source[line_start..byte].chars().collect();
    let past_separator = before
        .iter()
        .rposition(|character| !character.is_whitespace() && !separators.contains(*character))
        .filter(|found| *found + 1 < column);
    let opening_markup = behavior
        .key_markup_len(&line[in_line..])
        .map(|length| column + line[in_line..in_line + length].chars().count());
    let columns = [
        Some(column),
        column.checked_sub(1),
        past_separator,
        opening_markup,
    ];
    if let Some((path, _, _)) = columns
        .into_iter()
        .flatten()
        .find_map(|column| hovered_key(source, line_index, column, behavior))
    {
        return Some(FieldPath::from(path));
    }
    if let Some(path) = in_string_over_lines(behavior, source, line_index, cursor) {
        return Some(path);
    }
    let context = behavior.completion_context(source, cursor);
    Some(FieldPath::from(match context.value_key {
        Some(key) => path_with_key(&context.path, &key),
        None => context.path,
    }))
}

/// The body lines of the strings a text writes over lines: read once for
/// all the places a batch asks about (each place alone reads them again;
/// a language without such strings reads nothing).
struct StringBodies(Vec<std::ops::Range<usize>>);

fn string_bodies(behavior: &dyn ConfigLanguage, source: &str) -> std::rc::Rc<StringBodies> {
    let read = || StringBodies(behavior.saved_string_lines(source));
    if in_batch(source) {
        batch_cached(source, read)
    } else {
        std::rc::Rc::new(read())
    }
}

/// On a line of a string written over lines (a YAML block scalar, a TOML
/// `"""` string, an HCL heredoc, its closing line too): the string's
/// field, the one its key's line names whose range runs over the caret.
fn in_string_over_lines(
    behavior: &dyn ConfigLanguage,
    source: &str,
    line_index: usize,
    cursor: usize,
) -> Option<FieldPath> {
    // The body's lines, and the line ending it (`EOT`, `two"""`).
    let key_line = string_bodies(behavior, source)
        .0
        .iter()
        .find(|body| body.start <= line_index && line_index <= body.end && body.start > 0)
        .map(|body| body.start - 1)?;
    // The key's line ends before the next line's start (the body follows).
    let key_line_end = units_at(
        source,
        lines::line_start(source, key_line + 1).checked_sub(1)?,
    );
    if key_line == line_index || key_line_end >= cursor {
        return None;
    }
    // Not through the key line's own fallback: a string's field is named
    // on its key's line.
    let path = path_on_key_line(behavior, source, key_line, key_line_end)?;
    let range = field_range(behavior, source, &path.to_pointer())?;
    let (start, end) = (range.start.to_offset(source), range.end.to_offset(source));
    (start <= cursor && cursor <= end).then_some(path)
}

/// The path of the field whose key the line `line_index` (ending at
/// UTF-16 offset `line_end`) writes: on its first character.
fn path_on_key_line(
    behavior: &dyn ConfigLanguage,
    source: &str,
    line_index: usize,
    line_end: usize,
) -> Option<FieldPath> {
    let line = source.split('\n').nth(line_index)?;
    let indent = line
        .chars()
        .take_while(|character| character.is_whitespace())
        .count();
    let found = [indent, indent + 1]
        .into_iter()
        .find_map(|column| hovered_key(source, line_index, column, behavior));
    match found {
        Some((path, _, _)) => Some(FieldPath::from(path)),
        // A list item's line (`- |`): what the caret there names.
        None => {
            let context = behavior.completion_context(source, line_end);
            Some(FieldPath::from(match context.value_key {
                Some(key) => path_with_key(&context.path, &key),
                None => context.path,
            }))
        }
    }
}

/// [`path_at`] at each of `cursors` (UTF-16) of `source`, reading the text
/// once for all of them: what each language reads of it from the top (its
/// lines, tokens, the state of its walk at places along it) is read once
/// and shared, where each `path_at` alone reads it again.
pub(crate) fn paths_at(
    behavior: &dyn ConfigLanguage,
    source: &str,
    cursors: &[usize],
) -> Vec<Option<FieldPath>> {
    // A leading byte order mark is no part of the document: the text after
    // it is the one read (and batched).
    if let (text, true) = super::byte_order_mark::split(source) {
        let after: Vec<usize> = cursors
            .iter()
            .map(|cursor| cursor.saturating_sub(1))
            .collect();
        return paths_at(behavior, text, &after);
    }
    let read = || {
        cursors
            .iter()
            .map(|cursor| path_at(behavior, source, *cursor))
            .collect()
    };
    if in_batch(source) {
        read()
    } else {
        with_spans(behavior, source, &[], read)
    }
}

/// A language's walk of `source` from the top for a place's path starts:
/// counted while a batch runs for the text, where each should be read once
/// (outside one, [`path_at`] counts its own read).
pub(crate) fn walked_from_top(source: &str) {
    if in_batch(source) {
        note(Work::PathLookup);
    }
}

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod path_matrix;

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod kept_tests;

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod ranges_matrix;

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod string_bodies_matrix;

#[cfg(all(
    test,
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod addressable_matrix;

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod pointer_matrix;

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod missing_matrix;

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "protect"
))]
mod index_matrix;
