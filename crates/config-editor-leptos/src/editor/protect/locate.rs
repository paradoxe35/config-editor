//! Where a protected field is written: the stretches of text a lock covers,
//! found by path in any language (`field_range` for each field, the
//! fields inside a container, the bodies of multi-line strings, and the
//! closers ending a container).

use std::ops::Range;

use serde_json::Value;

use super::super::language::{ConfigLanguage, Repetition};
use super::super::paths::{TextRange, field_range, key_range, path_at};
use super::marks::escape;

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
}

/// Where a locked field is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Extent {
    /// The field's own key (not muted: only its value is).
    pub(crate) key: Option<Range<usize>>,
    /// Everything the field covers, in order, without overlaps.
    pub(crate) ranges: Vec<Range<usize>>,
}

/// Fields inside one locked container located at most.
const MAX_DESCENDANTS: usize = 2000;

/// Where the field at `pointer` is written in `text`, with everything
/// inside it when `document` (the parsed text) has it as a container.
pub(crate) fn extent(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    document: Option<&Value>,
    pointer: &str,
) -> Option<Extent> {
    let mut ranges = vec![lines.range(field_range(behavior, text, pointer)?)];
    let key = key_range(behavior, text, pointer).map(|range| lines.range(range));
    if let Some(value) = document.and_then(|document| lookup(document, pointer)) {
        let mut inside = Vec::new();
        descendants(value, pointer.to_string(), &mut inside);
        for child in inside.iter().take(MAX_DESCENDANTS) {
            if let Some(range) = field_range(behavior, text, child) {
                ranges.push(lines.range(range));
            }
        }
        if value.as_str().is_some_and(|text| text.contains('\n')) || value.is_object() {
            add_string_bodies(behavior, text, lines, &mut ranges);
        }
    }
    let markup = behavior.closing_markup_len("</x>").is_some();
    // Markup opening the field (`<web>`) starts at its `<`.
    if markup && let Some(first) = ranges.first_mut() {
        let before = &text[..byte_of(text, first.start)];
        if before.ends_with('<') {
            first.start -= 1;
        }
    }
    let mut ranges = merge(behavior, text, lines, ranges, pointer);
    close(behavior, text, &mut ranges, markup);
    Some(Extent { key, ranges })
}

/// `pointer` in `document`, also spelled without the `/0` of a lone
/// repeated element.
pub(crate) fn lookup<'a>(document: &'a Value, pointer: &str) -> Option<&'a Value> {
    document.pointer(pointer)
}

fn descendants(value: &Value, pointer: String, out: &mut Vec<String>) {
    if out.len() > MAX_DESCENDANTS {
        return;
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_pointer = format!("{pointer}/{}", escape(key));
                out.push(child_pointer.clone());
                descendants(child, child_pointer, out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let child_pointer = format!("{pointer}/{index}");
                out.push(child_pointer.clone());
                descendants(child, child_pointer, out);
            }
        }
        _ => {}
    }
}

/// A multi-line string's body lines below a covered line are covered too.
fn add_string_bodies(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    ranges: &mut Vec<Range<usize>>,
) {
    let bodies = behavior.string_body_lines(text);
    if bodies.is_empty() {
        return;
    }
    let covered: Vec<usize> = ranges
        .iter()
        .map(|range| lines.line_of(range.end))
        .collect();
    for body in bodies {
        if body.start > 0 && covered.contains(&(body.start - 1)) && !body.is_empty() {
            let start = lines.line_bounds(body.start).start;
            let end = lines.line_bounds(body.end - 1).end;
            ranges.push(start..end);
        }
    }
}

fn byte_of(text: &str, offset: usize) -> usize {
    super::super::keyboard::utf16_to_byte_index(text, offset)
}

/// The ranges sorted and joined where only the field's own text lies
/// between them: blank space, punctuation, or lines whose path is inside
/// the field. A gap holding another field (a TOML table between two of
/// the field's own) stays open.
fn merge(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    mut ranges: Vec<Range<usize>>,
    pointer: &str,
) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        let Some(last) = merged.last_mut() else {
            merged.push(range);
            continue;
        };
        if range.start <= last.end
            || gap_is_inside(behavior, text, lines, last.end..range.start, pointer)
        {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

fn gap_is_inside(
    behavior: &dyn ConfigLanguage,
    text: &str,
    lines: &LineIndex<'_>,
    gap: Range<usize>,
    pointer: &str,
) -> bool {
    let first = lines.line_of(gap.start);
    let last = lines.line_of(gap.end);
    // Lines wholly inside the gap must belong to the field.
    for line in first + 1..last {
        let bounds = lines.line_bounds(line);
        let content = &text[byte_of(text, bounds.start)..byte_of(text, bounds.end)];
        let blank = content.trim();
        if blank.is_empty()
            || behavior.comment_start(content) == Some(content.len() - content.trim_start().len())
        {
            continue;
        }
        let indent = content.len() - content.trim_start().len();
        let at = bounds.start + content[..indent].encode_utf16().count();
        let inside = path_at(behavior, text, at)
            .is_some_and(|path| within(behavior, &path.to_pointer(), pointer));
        if !inside {
            return false;
        }
    }
    true
}

/// Whether `path` is `pointer` or inside it, ignoring the `/0` steps of
/// lone repeated elements.
pub(crate) fn within(behavior: &dyn ConfigLanguage, path: &str, pointer: &str) -> bool {
    let plain = |pointer: &str| -> String {
        if behavior.repetition() == Repetition::None {
            pointer.to_string()
        } else {
            let lone = format!("{pointer}/").replace("/0/", "/");
            lone.trim_end_matches('/').to_string()
        }
    };
    let (path, pointer) = (plain(path), plain(pointer));
    pointer.is_empty() || path == pointer || path.starts_with(&format!("{pointer}/"))
}

/// The last range carried over the closers of containers it opened (`}`,
/// `]`, an XML end tag), so deleting one cannot unmake the field.
fn close(behavior: &dyn ConfigLanguage, text: &str, ranges: &mut [Range<usize>], markup: bool) {
    let (Some(first), Some(last)) = (ranges.first().cloned(), ranges.last_mut()) else {
        return;
    };
    let covered = &text[byte_of(text, first.start)..byte_of(text, last.end)];
    let pairs = behavior.config().brackets;
    let mut depth = if markup {
        markup_depth(covered)
    } else {
        bracket_depth(covered, pairs)
    };
    let mut end = byte_of(text, last.end);
    while depth > 0 {
        let rest = &text[end..];
        let skipped = rest.len() - rest.trim_start().len();
        let rest = &rest[skipped..];
        let closer = if markup {
            rest.starts_with("</")
                .then(|| behavior.closing_markup_len(rest))
                .flatten()
        } else {
            pairs
                .iter()
                .find(|(_, close)| rest.starts_with(*close))
                .map(|(_, close)| close.len_utf8())
        };
        let Some(length) = closer else {
            break;
        };
        end += skipped + length;
        depth -= 1;
    }
    last.end = text[..end].encode_utf16().count();
}

/// Brackets opened and not closed in `text`, outside double-quoted strings.
fn bracket_depth(text: &str, pairs: &[(char, char)]) -> usize {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for character in text.chars() {
        if quoted {
            match character {
                '\\' if !escaped => {
                    escaped = true;
                    continue;
                }
                '"' if !escaped => quoted = false,
                _ => {}
            }
            escaped = false;
            continue;
        }
        if character == '"' {
            quoted = true;
        } else if pairs.iter().any(|(open, _)| *open == character) {
            depth += 1;
        } else if pairs.iter().any(|(_, close)| *close == character) {
            depth = depth.saturating_sub(1);
        }
    }
    depth
}

/// Elements started and not ended in `text`.
fn markup_depth(text: &str) -> usize {
    let mut depth = 0usize;
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        let tag = &rest[at..];
        let end = tag.find('>').map_or(tag.len(), |end| end + 1);
        let body = &tag[..end];
        if body.starts_with("</") {
            depth = depth.saturating_sub(1);
        } else if body.starts_with("<!") || body.starts_with("<?") || body.ends_with("/>") {
        } else if body[1..]
            .starts_with(|character: char| character.is_alphabetic() || character == '_')
        {
            depth += 1;
        }
        rest = &tag[end.max(1)..];
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depths_count_what_a_range_left_open() {
        assert_eq!(
            bracket_depth("\"web\": {\n \"a\": \"{\"", &[('{', '}'), ('[', ']')]),
            1
        );
        assert_eq!(markup_depth("web>\n<a>1</a>\n<b/>"), 0);
        assert_eq!(markup_depth("<web>\n<a>1</a>\n<!-- c -->"), 1);
    }

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
