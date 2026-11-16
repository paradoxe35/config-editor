//! Where a protected field is written, from the parse: its key and its
//! whole value (`Text::value_ranges`), and the text's lines and offsets.

use std::ops::Range;

use serde_json::Value;

use super::super::language::{ConfigLanguage, Repetition};
use super::super::paths::{TextRange, field_key, key_range, whole_value};
use super::super::work_count::{Work, note_many};

/// Line starts of a text, to turn one-based positions into UTF-16 offsets
/// without rescanning it for each.
pub(crate) struct LineIndex<'a> {
    text: &'a str,
    /// Byte and UTF-16 offset of each line's start.
    starts: Vec<(usize, usize)>,
}

impl<'a> LineIndex<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let mut starts = vec![(0, 0)];
        let mut units = 0;
        for (byte, character) in text.char_indices() {
            units += character.len_utf16();
            if character == '\n' {
                starts.push((byte + 1, units));
            }
        }
        LineIndex { text, starts }
    }

    /// The UTF-16 offset of one-based `line`:`column`, clamped to the line.
    pub(crate) fn offset(&self, line: usize, column: usize) -> usize {
        let index = line.saturating_sub(1).min(self.starts.len() - 1);
        let (byte, units) = self.starts[index];
        let rest = self.text[byte..].split('\n').next().unwrap_or("");
        units
            + rest
                .chars()
                .take(column.saturating_sub(1))
                .map(char::len_utf16)
                .sum::<usize>()
    }

    pub(crate) fn range(&self, range: TextRange) -> Range<usize> {
        let start = self.offset(range.start.line, range.start.column);
        start..self.offset(range.end.line, range.end.column).max(start)
    }

    /// Zero-based line of UTF-16 `offset`.
    pub(crate) fn line_of(&self, offset: usize) -> usize {
        self.starts
            .partition_point(|(_, units)| *units <= offset)
            .saturating_sub(1)
    }

    /// UTF-16 offsets of zero-based `line`'s start and end (before its
    /// newline).
    pub(crate) fn line_bounds(&self, line: usize) -> Range<usize> {
        let line = line.min(self.starts.len() - 1);
        let (byte, units) = self.starts[line];
        let length: usize = self.text[byte..]
            .split('\n')
            .next()
            .unwrap_or("")
            .encode_utf16()
            .count();
        units..units + length
    }

    /// Zero-based `line`'s text, without its newline.
    pub(crate) fn line_text(&self, line: usize) -> &'a str {
        let line = line.min(self.starts.len() - 1);
        let start = self.starts[line].0;
        let end = self
            .starts
            .get(line + 1)
            .map_or(self.text.len(), |(next, _)| next - 1);
        &self.text[start..end]
    }

    pub(crate) fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// The byte index of UTF-16 `offset` (clamped to the text), read from
    /// its line's start, not from the text's.
    pub(crate) fn byte_of(&self, offset: usize) -> usize {
        let (byte, units) = self.starts[self.line_of(offset)];
        let mut counted = units;
        for (at, character) in self.text[byte..].char_indices() {
            if counted >= offset {
                return byte + at;
            }
            counted += character.len_utf16();
        }
        self.text.len()
    }

    /// The UTF-16 offset of byte `byte` (a character boundary), counted
    /// from its line's start.
    pub(crate) fn units_of(&self, byte: usize) -> usize {
        let line = self
            .starts
            .partition_point(|(start, _)| *start <= byte)
            .saturating_sub(1);
        let (start, units) = self.starts[line];
        note_many(Work::OffsetBytes, byte - start);
        units + self.text[start..byte].encode_utf16().count()
    }
}

/// Where a locked field is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Extent {
    /// The field's own key (not muted: only its value is).
    pub(crate) key: Option<Range<usize>>,
    /// Everything the field covers, in order, without overlaps.
    pub(crate) ranges: Vec<Range<usize>>,
}

/// Where the field at `pointer` is written in `text`, which parses: its key
/// and its whole value as the language's parse writes it
/// (`Text::value_ranges`: over lines, with its closers), from the markup
/// opening the key (`<` of `<web>`). `None` when the text writes neither.
pub(crate) fn extent(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    pointer: &str,
) -> Option<Extent> {
    let key = key_range(behavior, text, pointer).map(|range| lines.range(range));
    let (value, shared) = whole_value(behavior, text, pointer);
    let mut ranges: Vec<Range<usize>> = value
        .into_iter()
        .map(|range| lines.units_of(range.start)..lines.units_of(range.end))
        .collect();
    match (&key, ranges.first_mut()) {
        // The key and what stands between it and its value (a separator,
        // a line break, a tag) are the field's own.
        (Some(key), Some(first)) if !shared && key.start < first.start => first.start = key.start,
        (Some(key), Some(_)) if shared => ranges.insert(0, key.clone()),
        (Some(key), None) => ranges.push(key.clone()),
        (None, None) => return None,
        _ => {}
    }
    if let Some(first) = ranges.first_mut() {
        let name = lines.byte_of(first.start);
        if let Some(open) = super::super::flow_syntax::markup_before(behavior, text, name) {
            first.start -= text[open..name].encode_utf16().count();
        }
    }
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    Some(Extent {
        key,
        ranges: merged,
    })
}

/// `pointer` in `document`.
pub(crate) fn lookup<'a>(document: &'a Value, pointer: &str) -> Option<&'a Value> {
    document.pointer(pointer)
}

/// Whether `path` is `pointer` or inside it, ignoring the `/0` steps of
/// lone repeated elements.
pub(crate) fn within(behavior: &dyn ConfigLanguage, path: &str, pointer: &str) -> bool {
    let (path, pointer) = (plain(behavior, path), plain(behavior, pointer));
    pointer.is_empty() || path == pointer || path.starts_with(&format!("{pointer}/"))
}

/// `pointer` without the `/0` steps of lone repeated elements (as is, in a
/// language without them): the form [`within`] compares.
pub(crate) fn plain(behavior: &dyn ConfigLanguage, pointer: &str) -> String {
    if behavior.repetition() == Repetition::None {
        pointer.to_string()
    } else {
        field_key(pointer)
    }
}

/// The values of `value` (a document) under `keys`, any list index between
/// them, each item of a list there one value.
pub(super) fn values_under<'v>(value: &'v Value, keys: &[&str], found: &mut Vec<&'v Value>) {
    match value {
        Value::Array(items) => items
            .iter()
            .for_each(|item| values_under(item, keys, found)),
        _ if keys.is_empty() => found.push(value),
        Value::Object(map) => {
            if let Some(child) = map.get(&crate::editor::language::unescaped_key(keys[0])) {
                values_under(child, &keys[1..], found);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_converts_positions() {
        let text = "a: 1\nbé: 2\n";
        let lines = LineIndex::new(text);
        assert_eq!(lines.offset(2, 1), 5);
        assert_eq!(lines.offset(2, 3), 7);
        assert_eq!(lines.offset(9, 1), 11);
        assert_eq!(lines.line_of(6), 1);
        assert_eq!(lines.line_bounds(1), 5..10);
        assert_eq!(lines.line_count(), 3);
    }
}
