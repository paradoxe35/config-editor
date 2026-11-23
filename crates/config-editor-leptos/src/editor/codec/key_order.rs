//! Map keys put in another order in a view: each entry's lines move with
//! the comment lines right above it, so the host reads its keys in the
//! view's order.

use super::super::item_sources::SaveTold;
use super::super::language::{ConfigLanguage, pointer_key};
use super::super::paths;
use super::reorder::{comment_only, indent_of};
use super::{comment_start, key_markup_start, read, span_text_range};
use crate::editor::paths::FieldPath;
use crate::editor::text::{TextRange, TextSize, slice, text_size, utf16_of};
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Range;

/// Whether `left` and `right` hold their maps' keys in the same order
/// (`==` on values does not look at it).
pub(in super::super) fn same_order(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            left.keys().eq(right.keys())
                && left
                    .iter()
                    .all(|(key, value)| right.get(key).is_none_or(|other| same_order(value, other)))
        }
        (Value::Array(left), Value::Array(right)) => left
            .iter()
            .zip(right)
            .all(|(left, right)| same_order(left, right)),
        _ => true,
    }
}

/// `previous` (which reads as `old`) with the entries of each map `new`
/// orders otherwise moved into `new`'s order, when that is all that
/// differs and each entry is lines of its own; `None` otherwise, or when
/// the text would not read back as `new` in its order.
pub(in super::super) fn patch_key_order(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
    _told: SaveTold<'_>,
) -> Option<String> {
    if old != new || same_order(old, new) {
        return None;
    }
    let mut maps = Vec::new();
    reordered_maps(old, new, String::new(), &mut maps);
    // One map at a time, each read from the text the one before wrote. A
    // member the host writes elsewhere (an XML attribute, a YAML merged
    // key) stays where it is, among the map's entries; the map's own
    // entries take the order.
    let (mut text, mut current) = (previous.to_string(), old.clone());
    let mut own_orders = Vec::with_capacity(maps.len());
    for (pointer, order) in maps {
        let keys: Vec<String> = current
            .pointer(&pointer)?
            .as_object()?
            .keys()
            .cloned()
            .collect();
        // Members the host writes inside one construct (an XML element's
        // attributes, in its start tag) put in the order there.
        if let Some(inline) = language.members_in_order(&text, &FieldPath::from(&pointer), &order) {
            text = inline;
        }
        let (moved, own) = moved_entries(language, &text, &pointer, &keys, &order)?;
        text = moved;
        let map = current.pointer_mut(&pointer)?.as_object_mut()?;
        let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
        let places: HashMap<&String, usize> = order
            .iter()
            .enumerate()
            .map(|(at, key)| (key, at))
            .collect();
        entries.sort_by_key(|(key, _)| places.get(key).copied());
        map.extend(entries);
        own_orders.push((pointer, own));
    }
    let reread = read(language, &text, schema).ok()?;
    let reread = language.spelled_like(&text, reread, new);
    (reread == *new && in_own_order(&reread, new, &own_orders)).then_some(text)
}

/// Whether `reread` holds its maps' keys in `new`'s order, but for the maps
/// of `own_orders`, whose own entries alone must be in the order given.
fn in_own_order(reread: &Value, new: &Value, own_orders: &[(String, Vec<String>)]) -> bool {
    let mut differing = Vec::new();
    reordered_maps(new, reread, String::new(), &mut differing);
    differing.iter().all(|(pointer, keys)| {
        own_orders
            .iter()
            .find(|(other, _)| other == pointer)
            .is_some_and(|(_, own)| keys.iter().filter(|key| own.contains(key)).eq(own.iter()))
    })
}

/// The maps whose keys `new` orders otherwise than `old` (the two equal
/// as values), outermost first, with `new`'s order.
fn reordered_maps(old: &Value, new: &Value, pointer: String, out: &mut Vec<(String, Vec<String>)>) {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            if !left.keys().eq(right.keys()) {
                out.push((pointer.clone(), right.keys().cloned().collect()));
            }
            for (key, value) in left {
                if let Some(other) = right.get(key) {
                    reordered_maps(value, other, format!("{pointer}/{}", pointer_key(key)), out);
                }
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            for (index, (left, right)) in left.iter().zip(right).enumerate() {
                reordered_maps(left, right, format!("{pointer}/{index}"), out);
            }
        }
        _ => {}
    }
}

/// `text` with the own entries of the map at `pointer` (of its `keys`, in
/// the order written) rewritten in `order`, and those entries' keys in
/// `order`: each entry's lines (its own-line comments above it, its key's
/// line, and the lines its value runs on) in the place of the entry
/// `order` puts there. A member that is no entry of lines of its own in
/// the map (an XML attribute, a key a YAML merge brings in) keeps its
/// place.
fn moved_entries(
    language: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    keys: &[String],
    order: &[String],
) -> Option<(String, Vec<String>)> {
    let lines: Vec<&str> = text.split('\n').collect();
    // Every entry's key and value placed from one read of the text (alone,
    // each read it from the top).
    let pointers: Vec<String> = keys
        .iter()
        .map(|key| format!("{pointer}/{}", pointer_key(key)))
        .collect();
    let own = paths::with_spans(language, text, &pointers, || {
        let mut own: Vec<(&String, Range<usize>)> = Vec::with_capacity(keys.len());
        for (key, entry) in keys.iter().zip(&pointers) {
            let Some(slot) = entry_lines(language, text, &lines, entry) else {
                continue;
            };
            // In the order written, each apart from the one before.
            if own.last().is_some_and(|(_, last)| last.end > slot.start) {
                return None;
            }
            own.push((key, slot));
        }
        Some(own)
    })?;
    let blocks: HashMap<&String, &Range<usize>> =
        own.iter().map(|(key, slot)| (*key, slot)).collect();
    let order: Vec<String> = order
        .iter()
        .filter(|key| blocks.contains_key(key))
        .cloned()
        .collect();
    let block = |key: &String| -> Option<&Range<usize>> { blocks.get(key).copied() };
    // The blank lines right above each entry but the first, after the one
    // before: they set it apart from the entries above, and go with it (a
    // group's blank line stays before the group, not where it was). The
    // first entry has none of its own: moved down, it takes those of its
    // place.
    let blank_above: HashMap<&String, usize> = own
        .iter()
        .enumerate()
        .map(|(index, (key, slot))| {
            let floor = index
                .checked_sub(1)
                .map_or(slot.start, |before| own[before].1.end);
            let blank = lines[floor..slot.start]
                .iter()
                .rev()
                .take_while(|line| line.trim().is_empty())
                .count();
            (*key, blank)
        })
        .collect();
    let mut patched: Vec<String> = Vec::with_capacity(lines.len());
    let mut at = 0;
    for (index, ((place, slot), key)) in own.iter().zip(&order).enumerate() {
        // The place's own blank lines out, the moved entry's in (none
        // before the first entry).
        let start = slot.start - blank_above[place];
        patched.extend(lines[at..start].iter().map(|line| line.to_string()));
        let from = block(key)?;
        let blank = if index == 0 {
            0..0
        } else if own.first().is_some_and(|(first, _)| *first == key) {
            start..slot.start
        } else {
            from.start - blank_above[key]..from.start
        };
        patched.extend(lines[blank].iter().map(|line| line.to_string()));
        let mut moved: Vec<String> = lines[from.clone()]
            .iter()
            .map(|line| line.to_string())
            .collect();
        // The separator after an entry (JSON's `,`) is the place's.
        let last = moved.last_mut()?;
        *last = super::reorder::with_comma_of(language, last, lines[slot.end - 1]);
        patched.extend(moved);
        at = slot.end;
    }
    patched.extend(lines[at..].iter().map(|line| line.to_string()));
    Some((patched.join("\n"), order))
}

/// The lines (0-based, end exclusive) of the entry at `pointer` (a key
/// of a map, or an item of a list written without one): the comment lines
/// right above its key (or its value, after the language's item marker),
/// the key's line, and the lines its value runs on (a block scalar, a
/// container to where it closes, or the lines indented under the key
/// where indentation is structure). `None` when the key shares its line
/// with something before it, or is written in another map (a key a YAML
/// merge brings in).
fn entry_lines(
    language: &dyn ConfigLanguage,
    text: &str,
    lines: &[&str],
    pointer: &str,
) -> Option<Range<usize>> {
    if let Some(span) = paths::memo_key_span(language, text, pointer) {
        let name = span_text_range(text, span)?;
        if !written_at_its_place(language, text, pointer, name.start())? {
            return None;
        }
    }
    own_lines(language, text, lines, pointer)
}

/// [`entry_lines`] where the key the text writes for `pointer` is taken
/// as its own (the check costs a walk to the key): a caller that reads
/// back what it writes there.
pub(in super::super) fn own_lines(
    language: &dyn ConfigLanguage,
    text: &str,
    lines: &[&str],
    pointer: &str,
) -> Option<Range<usize>> {
    // An entry the language writes as lines of its own (a TOML table, one
    // with no header of its own included) starts where it says.
    let item = language.item_lines(text, &FieldPath::from(pointer));
    let line = match (&item, paths::memo_key_span(language, text, pointer)) {
        (Some(own), _) => paths::line_column(text, text_size(own.start)).0 - 1,
        (None, Some(span)) => {
            let name = span_text_range(text, span)?;
            let line_start = slice::line_start_of(text, name.start());
            // The language's markup hook counts in `usize` bytes.
            let opens = key_markup_start(language, text, usize::from(name.start()))
                .map_or(name.start(), text_size);
            let before = TextRange::new(line_start, opens.clamp(line_start, name.start()));
            if !slice::slice(text, before)?.trim().is_empty() {
                return None;
            }
            span.line.checked_sub(1)?
        }
        (None, None) => {
            // A list item: its value first on its line, after the item
            // marker the language writes.
            let (_, index) = pointer.rsplit_once('/')?;
            index.parse::<usize>().ok()?;
            let span = paths::memo_value_span(language, text, pointer)?;
            let value = span_text_range(text, span)?;
            let line_start = slice::line_start_of(text, value.start());
            let before = slice::slice(text, TextRange::new(line_start, value.start()))?.trim();
            let marker_only = before.is_empty()
                || super::super::flow_syntax::Flow::of(language)
                    .after_item_marker(before)
                    .is_some_and(|rest| rest.trim().is_empty());
            if !marker_only {
                return None;
            }
            span.line.checked_sub(1)?
        }
    };
    let mut last = line;
    // A value over lines (a block scalar, a heredoc) ends where it ends,
    // the line break after it no line of its (a heredoc's span runs
    // through its terminator line).
    if let Some(value) =
        paths::memo_value_span(language, text, pointer).and_then(|span| span_text_range(text, span))
    {
        let end = value.start()
            + slice::slice(text, value).map_or(TextSize::default(), |value| {
                TextSize::of(value.trim_end_matches('\n'))
            });
        last = last.max(paths::line_column(text, end).0 - 1);
    }
    // And ends where it says.
    if let Some(own) = item {
        last = last.max(paths::line_column(text, text_size(own.end)).0 - 1);
    }
    // A container opened on its lines runs to where it closes.
    last = language.closing_line(lines, (line, last));
    if language.indent_is_structure() {
        let indent = indent_of(lines[line]);
        let mut next = last + 1;
        while next < lines.len() {
            let below = lines[next];
            if below.trim().is_empty() || comment_only(language, below) {
                next += 1;
                continue;
            }
            if indent_of(below) <= indent {
                break;
            }
            last = next;
            next += 1;
        }
    }
    let mut top = line;
    while top > 0
        && comment_only(language, lines[top - 1])
        && comment_start(language, lines[top - 1]) >= indent_of(lines[line])
    {
        top -= 1;
    }
    Some(top..last + 1)
}

/// Whether `text` writes the key at `pointer` in its own map, not in
/// another one it reads from (a key a YAML merge brings in is written in
/// the merged map); `true` when the key is not found.
pub(in super::super) fn written_in_its_map(
    language: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
) -> bool {
    paths::memo_key_span(language, text, pointer)
        .and_then(|span| span_text_range(text, span))
        .and_then(|name| written_at_its_place(language, text, pointer, name.start()))
        .unwrap_or(true)
}

/// Whether the key written at byte `start` of `text` is the key at
/// `pointer`; `None` when no path is there.
fn written_at_its_place(
    language: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    start: TextSize,
) -> Option<bool> {
    // A path lookup takes the language API's UTF-16 caret.
    let at = utf16_of(text, start);
    Some(paths::path_at(language, text, at)?.to_pointer() == pointer)
}
