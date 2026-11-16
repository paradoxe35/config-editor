//! Where a whole value is written: from its first character over every line
//! it takes to its last closer, read from the parse (`Text::value_ranges`).
//! Locks cover it, so no edit can change a part of a locked value.

use std::ops::Range;
use std::rc::Rc;

use serde_json::Value;

use super::super::language::{ConfigLanguage, ParseError, Span};
use super::super::parse_cache;
use super::span_memo::batch_cached;
use super::{key_span, line_start, spanning, split_line, value_span};

/// Fields inside one value read at most (a lock on a large container
/// covers what they span).
const MAX_INSIDE: usize = 2000;

/// The byte ranges the value at `pointer` is written in (see
/// [`ConfigLanguage::value_ranges`]), in any spelling of the pointer, and
/// whether they are written elsewhere than at the field, read through a
/// shared value (a YAML alias: the anchor's value). Empty when `source`
/// does not parse or does not write it.
#[cfg(feature = "protect")]
pub(crate) fn whole_value(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
) -> (Vec<Range<usize>>, bool) {
    if let Some(ranges) = super::spellings(behavior, source, pointer)
        .iter()
        .map(|spelling| behavior.value_ranges(source, spelling))
        .find(|ranges| !ranges.is_empty())
    {
        return (ranges, false);
    }
    match behavior.written_at(source, pointer) {
        Some(written) if written != pointer => (whole_value(behavior, source, &written).0, true),
        _ => (Vec::new(), false),
    }
}

/// [`Text::value_ranges`](super::super::language::Text::value_ranges)
/// from the language's one-line spans: the value's own span and the key
/// and value spans of everything inside it (each over the lines its end
/// column counts past, and through a construct over lines starting in
/// it), joined where no other field is written between them, the last one
/// carried over the closers of the containers they leave open.
pub(crate) fn joined_value_ranges(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
) -> Vec<Range<usize>> {
    let parsed = parsed(behavior, source);
    let Ok(document) = parsed.as_ref() else {
        return Vec::new();
    };
    let Some(value) = document.pointer(pointer) else {
        return Vec::new();
    };
    let constructs = batch_cached(source, || Constructs(constructs(behavior, source)));
    let through = |range: Range<usize>| {
        let end = constructs
            .0
            .iter()
            .filter(|construct| range.start <= construct.start && construct.start < range.end)
            .map(|construct| construct.end)
            .fold(range.end, usize::max);
        range.start..end
    };
    let mut stretches: Vec<Range<usize>> = value_span(behavior, source, pointer)
        .map(|span| through(bytes(source, span)))
        .into_iter()
        .collect();
    for child in inside(value, pointer) {
        stretches.extend(key_span(behavior, source, &child).map(|span| bytes(source, span)));
        stretches
            .extend(value_span(behavior, source, &child).map(|span| through(bytes(source, span))));
    }
    let mut ranges = join(behavior, source, document, pointer, stretches);
    // The containers are opened from the field's start: its key, from the
    // markup opening it (`meta {`, `<meta>`).
    let key = key_span(behavior, source, pointer).map(|span| {
        let start = bytes(source, span).start;
        super::super::flow_syntax::markup_before(behavior, source, start).unwrap_or(start)
    });
    let start = ranges.first().map_or(0, |range| range.start);
    let start = key.filter(|key| *key < start).unwrap_or(start);
    if let Some(last) = ranges.last_mut() {
        last.end = closed(behavior, source, start..last.end);
    }
    ranges
}

/// `source` parsed in `behavior`, once for all the values of a batch (the
/// parse cache keeps parses only while an editor is mounted).
pub(crate) fn parsed(behavior: &dyn ConfigLanguage, source: &str) -> Rc<Result<Value, ParseError>> {
    Rc::clone(&batch_cached(source, || Parsed(parse_cache::parsed(behavior, source))).0)
}

struct Parsed(Rc<Result<Value, ParseError>>);

/// `stretches` sorted and joined where the text between two holds no other
/// field's key (blank space, punctuation, comments: the value's own).
fn join(
    behavior: &dyn ConfigLanguage,
    source: &str,
    document: &Value,
    pointer: &str,
    mut stretches: Vec<Range<usize>>,
) -> Vec<Range<usize>> {
    stretches.retain(|range| range.start < range.end);
    stretches.sort_by_key(|range| (range.start, range.end));
    let mut joined: Vec<Range<usize>> = Vec::new();
    let mut keys: Option<Rc<KeyStarts>> = None;
    for range in stretches {
        let Some(last) = joined.last_mut() else {
            joined.push(range);
            continue;
        };
        let gap = last.end..range.start;
        let own = gap.start >= gap.end
            || source[gap.clone()].trim().is_empty()
            || !keys
                .get_or_insert_with(|| {
                    batch_cached(source, || KeyStarts::of(behavior, source, document))
                })
                .foreign_in(&gap, pointer);
        if own {
            last.end = last.end.max(range.end);
        } else {
            joined.push(range);
        }
    }
    joined
}

/// `range`'s end carried over the closers of the containers the text in it
/// opens and leaves open (`}`, `]`, an end tag).
fn closed(behavior: &dyn ConfigLanguage, source: &str, range: Range<usize>) -> usize {
    let mut depth = behavior.unclosed_containers(&source[range.clone()]);
    let mut end = range.end;
    while depth > 0 {
        let rest = &source[end..];
        let skipped = rest.len() - rest.trim_start().len();
        let Some(length) = behavior.container_closer_len(&rest[skipped..]) else {
            break;
        };
        end += skipped + length;
        depth -= 1;
    }
    end
}

/// The pointers of the fields inside `value` (at `pointer`), at most
/// [`MAX_INSIDE`].
pub(crate) fn inside(value: &Value, pointer: &str) -> Vec<String> {
    fn walk(value: &Value, pointer: &str, out: &mut Vec<String>) {
        let visit = |segment: String, child: &Value, out: &mut Vec<String>| {
            if out.len() >= MAX_INSIDE {
                return;
            }
            let child_pointer = format!("{pointer}/{segment}");
            out.push(child_pointer.clone());
            walk(child, &child_pointer, out);
        };
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    visit(crate::editor::language::pointer_key(key), child, out);
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    visit(index.to_string(), child, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, pointer, &mut out);
    out
}

/// `span` as a byte range of `source`, over the lines its end column
/// counts past (see `spanning`).
fn bytes(source: &str, span: Span) -> Range<usize> {
    let range = spanning(source, span);
    let at = |line: usize, column: usize| {
        let index = line.saturating_sub(1);
        let start = line_start(source, index);
        let text = split_line(source, index).unwrap_or("");
        start
            + text
                .char_indices()
                .nth(column.saturating_sub(1))
                .map_or(text.len(), |(at, _)| at)
    };
    let start = at(range.start.line, range.start.column);
    start..at(range.end.line, range.end.column).max(start)
}

/// The byte ranges of the constructs `source` writes over lines (see
/// `Text::multiline_constructs`), read once for all the values of a batch.
struct Constructs(Vec<Range<usize>>);

fn constructs(behavior: &dyn ConfigLanguage, source: &str) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = behavior
        .multiline_constructs(source)
        .into_iter()
        .map(|construct| construct.range)
        .filter(|range| range.start < range.end && range.end <= source.len())
        .collect();
    ranges.sort_by_key(|range| range.start);
    ranges
}

/// Where each field's key is written in a text, by start, read once for
/// all the values of a batch.
struct KeyStarts(Vec<(usize, String)>);

impl KeyStarts {
    fn of(behavior: &dyn ConfigLanguage, source: &str, document: &Value) -> Self {
        let pointers: Vec<String> = super::fields(document)
            .into_iter()
            .map(|(pointer, _)| pointer)
            .collect();
        let spans = behavior.key_spans(source, &pointers);
        let mut starts: Vec<(usize, String)> = pointers
            .into_iter()
            .zip(spans)
            .filter_map(|(pointer, span)| Some((bytes(source, span?).start, pointer)))
            .collect();
        starts.sort();
        KeyStarts(starts)
    }

    /// Whether a key in `gap` is another field's than `pointer`'s, one
    /// inside it or one holding it.
    fn foreign_in(&self, gap: &Range<usize>, pointer: &str) -> bool {
        let from = self.0.partition_point(|(start, _)| *start < gap.start);
        self.0[from..]
            .iter()
            .take_while(|(start, _)| *start < gap.end)
            .any(|(_, other)| !related(other, pointer))
    }
}

/// Whether `other` is `pointer`, inside it, or holds it.
fn related(other: &str, pointer: &str) -> bool {
    let within = |inner: &str, outer: &str| {
        inner == outer || (inner.starts_with(outer) && inner[outer.len()..].starts_with('/'))
    };
    within(other, pointer) || within(pointer, other)
}
