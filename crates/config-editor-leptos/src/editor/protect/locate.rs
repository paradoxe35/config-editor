//! Where a protected field is written, from the parse: its key and its
//! whole value (`Text::value_ranges`), and the text's lines and offsets.

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::super::paths::{self, field_key, key_range, whole_value};
use super::super::text::{TextRange, TextSize, byte_range, slice, text_size};
use super::super::work_count::{Work, note_many};

/// Line starts of a text, to turn one-based positions into byte offsets
/// without rescanning it for each.
pub(crate) struct LineIndex<'a> {
    text: &'a str,
    /// The byte each line starts at.
    starts: Vec<TextSize>,
}

impl<'a> LineIndex<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        note_many(Work::OffsetBytes, text.len());
        let starts = std::iter::once(TextSize::default())
            .chain(text.match_indices('\n').map(|(at, _)| text_size(at + 1)))
            .collect();
        LineIndex { text, starts }
    }

    /// The byte of one-based `line`:`column` (a character column),
    /// clamped to the line.
    pub(crate) fn offset(&self, line: usize, column: usize) -> TextSize {
        let index = line.saturating_sub(1).min(self.starts.len() - 1);
        self.starts[index]
            + self
                .line_text(index)
                .chars()
                .take(column.saturating_sub(1))
                .map(TextSize::of)
                .sum::<TextSize>()
    }

    /// `range` (lines and character columns) as bytes.
    pub(crate) fn range(&self, range: paths::TextRange) -> TextRange {
        let start = self.offset(range.start.line, range.start.column);
        TextRange::new(
            start,
            self.offset(range.end.line, range.end.column).max(start),
        )
    }

    /// Zero-based line of byte `offset`.
    pub(crate) fn line_of(&self, offset: TextSize) -> usize {
        self.starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
    }

    /// Zero-based `line`'s start and end (before its newline).
    pub(crate) fn line_bounds(&self, line: usize) -> TextRange {
        let line = line.min(self.starts.len() - 1);
        TextRange::at(self.starts[line], TextSize::of(self.line_text(line)))
    }

    /// Zero-based `line`'s text, without its newline.
    pub(crate) fn line_text(&self, line: usize) -> &'a str {
        let line = line.min(self.starts.len() - 1);
        let end = self
            .starts
            .get(line + 1)
            .map_or(TextSize::of(self.text), |next| *next - TextSize::from(1));
        slice::slice(self.text, TextRange::new(self.starts[line], end)).unwrap_or_default()
    }

    pub(crate) fn line_count(&self) -> usize {
        self.starts.len()
    }
}

/// Where a locked field is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Extent {
    /// The field's own key (not muted: only its value is).
    pub(crate) key: Option<TextRange>,
    /// Everything the field covers, in order, without overlaps.
    pub(crate) ranges: Vec<TextRange>,
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
    let mut ranges: Vec<TextRange> = value.into_iter().map(byte_range).collect();
    match (&key, ranges.first_mut()) {
        // The key and what stands between it and its value (a separator,
        // a line break, a tag) are the field's own.
        (Some(key), Some(first)) if !shared && key.start() < first.start() => {
            *first = TextRange::new(key.start(), first.end());
        }
        (Some(key), Some(_)) if shared => ranges.insert(0, *key),
        (Some(key), None) => ranges.push(*key),
        (None, None) => return None,
        _ => {}
    }
    if let Some(first) = ranges.first_mut() {
        // `markup_before` is the flow syntax's, in `str` bytes.
        let name = usize::from(first.start());
        if let Some(open) = super::super::flow_syntax::markup_before(behavior, text, name) {
            *first = TextRange::new(text_size(open), first.end());
        }
    }
    ranges.sort_by_key(|range| (range.start(), range.end()));
    let mut merged: Vec<TextRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start() <= last.end() => *last = last.cover(range),
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
    let (path, pointer) = (field_key(behavior, path), field_key(behavior, pointer));
    pointer.is_empty() || path == pointer || path.starts_with(&format!("{pointer}/"))
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
        let at = TextSize::from;
        assert_eq!(lines.offset(2, 1), at(5));
        assert_eq!(lines.offset(2, 3), at(8));
        assert_eq!(lines.offset(9, 1), at(12));
        assert_eq!(lines.line_of(at(6)), 1);
        assert_eq!(lines.line_bounds(1), TextRange::new(at(5), at(11)));
        assert_eq!(lines.line_count(), 3);
    }
}
