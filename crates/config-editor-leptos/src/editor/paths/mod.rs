//! Paths and places in the text, both ways: where a [`FieldPath`] is written
//! (`field_range`, `problem_span`) and which path the caret is on
//! (`path_at`). Built on each language's span and context lookups
//! (`key_span`, `value_span`, `hover_path`, `completion_context`), so a
//! language gets them by implementing those.

mod types;

pub use types::{FieldPath, TextPosition, TextRange};

use super::codec::inline_location;
use super::completion::{hovered_key, path_with_key};
use super::keyboard::utf16_to_byte_index;
use super::language::{ConfigLanguage, Repetition, Span};

/// `pointer`, and for languages that repeat elements or blocks, the pointer
/// without its `/0` steps: a lone XML element or HCL block read as a one-item
/// list is addressed `…/0/…` but written without the index.
fn spellings(behavior: &dyn ConfigLanguage, pointer: &str) -> Vec<String> {
    let mut spellings = vec![pointer.to_string()];
    if behavior.repetition() != Repetition::None && pointer.contains("/0") {
        let lone = pointer.replace("/0/", "/");
        spellings.push(lone.strip_suffix("/0").unwrap_or(&lone).to_string());
    }
    spellings
}

/// The key written for `pointer`, inside an inline container or not.
fn key_span(behavior: &dyn ConfigLanguage, source: &str, pointer: &str) -> Option<Span> {
    inline_location(behavior, source, pointer, true).or_else(|| behavior.key_span(source, pointer))
}

/// The value written for `pointer`, inside an inline container or not.
fn value_span(behavior: &dyn ConfigLanguage, source: &str, pointer: &str) -> Option<Span> {
    inline_location(behavior, source, pointer, false)
        .or_else(|| behavior.value_span(source, pointer))
}

/// Characters of line `line` (one-based) up to its last non-blank one.
fn trimmed_width(source: &str, line: usize) -> usize {
    source
        .split('\n')
        .nth(line.saturating_sub(1))
        .map_or(0, |text| text.trim_end().chars().count())
}

fn range(span: Span) -> TextRange {
    TextRange::new(
        TextPosition::new(span.line, span.start_column),
        TextPosition::new(span.line, span.end_column),
    )
}

/// Where the field at `pointer` is written: its key and value when they
/// share a line (`listen: 7835`), the key alone when the value is a block
/// below it, the value alone for an array item. The root is the whole text.
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
    let found = spellings(behavior, pointer)
        .iter()
        .find_map(|pointer| field_span(behavior, source, pointer));
    // A list written as repeated elements or blocks (`<hosts>` twice) has no
    // key of its own: its first item's stands for it.
    let found = found.or_else(|| {
        (behavior.repetition() != Repetition::None)
            .then(|| key_span(behavior, source, &format!("{pointer}/0")))
            .flatten()
    });
    found.map(range)
}

/// [`field_range`] of one spelling of a pointer.
fn field_span(behavior: &dyn ConfigLanguage, source: &str, pointer: &str) -> Option<Span> {
    let key = key_span(behavior, source, pointer);
    let value = value_span(behavior, source, pointer);
    let Some(key) = key else {
        return value;
    };
    let Some(value) = value.filter(|value| {
        value.line == key.line
            && value.start_column >= key.start_column
            && value.start_column <= trimmed_width(source, key.line)
    }) else {
        return Some(key);
    };
    let line: Vec<char> = source
        .split('\n')
        .nth(key.line - 1)
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
    if matches!(written.trim(), "{" | "[") {
        return Some(key);
    }
    let mut start = key.start_column;
    let mut end = value
        .end_column
        .min(trimmed_width(source, key.line) + 1)
        .max(key.end_column);
    // Markup closing the value (`</listen>`) is part of the field, and so
    // is the start tag's `<`.
    let rest = text(end - 1, line.len());
    if let Some(closing) = behavior.closing_markup_len(&rest) {
        end += rest[..closing.min(rest.len())].chars().count();
        if start > 1 && line.get(start - 2) == Some(&'<') {
            start -= 1;
        }
    }
    Some(Span {
        line: key.line,
        start_column: start,
        end_column: end,
    })
}

/// Where a problem with the value at `pointer` is underlined, as schema
/// diagnostics are: the value, or its key when the value is `written_as_block`
/// below it (a block has no text on the key's line); a `required` problem
/// (a missing property) on the object's key.
#[cfg_attr(not(feature = "schema-validation"), allow(dead_code))]
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
                .or_else(|| behavior.key_span(source, path))
                .or_else(|| behavior.value_span(source, path))
        } else {
            inline_location(behavior, source, path, false)
                .or_else(|| {
                    behavior.value_span(source, path).filter(|span| {
                        !written_as_block || span.start_column <= trimmed_width(source, span.line)
                    })
                })
                .or_else(|| behavior.key_span(source, path))
                .or_else(|| behavior.value_span(source, path))
        }
    };
    spellings(behavior, pointer)
        .iter()
        .find_map(|pointer| locate(pointer))
}

/// The path of what the caret at UTF-16 `cursor` is on: the key under or just
/// before it, else the value or container holding it (the root at the top
/// level). `None` past the end of the text.
pub(crate) fn path_at(
    behavior: &dyn ConfigLanguage,
    source: &str,
    cursor: usize,
) -> Option<FieldPath> {
    if cursor > source.encode_utf16().count() {
        return None;
    }
    let byte = utf16_to_byte_index(source, cursor);
    let line_start = source[..byte].rfind('\n').map_or(0, |index| index + 1);
    let line_index = source[..line_start].matches('\n').count();
    let column = source[line_start..byte].chars().count();
    let line = source[line_start..].split('\n').next().unwrap_or("");
    // In markup closing a value (`<a>1</a|>`): the value it closes.
    let in_line = byte - line_start;
    if let Some(open) = line[..in_line].rfind("</")
        && behavior
            .closing_markup_len(&line[open..])
            .is_some_and(|len| in_line <= open + len)
    {
        let value_end = source[..line_start + open].encode_utf16().count();
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
    let opening_markup = (line[in_line..].starts_with('<')
        && behavior.closing_markup_len("</>").is_some())
    .then_some(column + 1);
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
    let context = behavior.completion_context(source, cursor);
    Some(FieldPath::from(match context.value_key {
        Some(key) => path_with_key(&context.path, &key),
        None => context.path,
    }))
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
