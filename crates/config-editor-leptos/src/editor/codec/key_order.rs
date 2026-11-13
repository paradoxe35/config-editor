//! Map keys put in another order in a view: each entry's lines move with
//! the comment lines right above it, so the host reads its keys in the
//! view's order (round 26: the order never reached the host).

use super::super::language::ConfigLanguage;
use super::super::paths;
use super::reorder::{comment_only, indent_of};
use super::{byte_range, comment_start, escape, key_markup_start, read};
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
) -> Option<String> {
    if old != new || same_order(old, new) {
        return None;
    }
    let mut maps = Vec::new();
    reordered_maps(old, new, String::new(), &mut maps);
    // One map at a time, each read from the text the one before wrote. A
    // member the host writes elsewhere (an XML attribute, a YAML merged
    // key) stays where it is; the map's own entries take the order.
    let (mut text, mut current) = (previous.to_string(), old.clone());
    let mut own_orders = Vec::with_capacity(maps.len());
    for (pointer, order) in maps {
        let keys: Vec<String> = current
            .pointer(&pointer)?
            .as_object()?
            .keys()
            .cloned()
            .collect();
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
                    reordered_maps(value, other, format!("{pointer}/{}", escape(key)), out);
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
        .map(|key| format!("{pointer}/{}", escape(key)))
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
    let mut patched: Vec<String> = Vec::with_capacity(lines.len());
    let mut at = 0;
    for ((_, slot), key) in own.iter().zip(&order) {
        patched.extend(lines[at..slot.start].iter().map(|line| line.to_string()));
        let from = block(key)?;
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
/// of a map): the comment lines right above its key, the key's line, and the
/// lines its value runs on (a block scalar, a container to where it
/// closes, or the lines indented under the key where indentation is
/// structure). `None` when the key shares its line with something before
/// it, or is written in another map (a key a YAML merge brings in).
fn entry_lines(
    language: &dyn ConfigLanguage,
    text: &str,
    lines: &[&str],
    pointer: &str,
) -> Option<Range<usize>> {
    let span = paths::memo_key_span(language, text, pointer)?;
    let line = span.line.checked_sub(1)?;
    let name = byte_range(text, span)?;
    let at = paths::units_at(text, name.start);
    if paths::path_at(language, text, at)?.to_pointer() != pointer {
        return None;
    }
    let line_start = text[..name.start].rfind('\n').map_or(0, |at| at + 1);
    let opens = key_markup_start(language, text, name.start).unwrap_or(name.start);
    if !text[line_start..opens.min(name.start)].trim().is_empty() {
        return None;
    }
    let mut last = line;
    // A value over lines (a block scalar, a heredoc) ends where it ends.
    if let Some(value) =
        paths::memo_value_span(language, text, pointer).and_then(|span| byte_range(text, span))
    {
        last = last.max(paths::line_column(text, value.end).0 - 1);
    }
    // An entry the language writes as lines of its own (a TOML table)
    // ends where it says.
    if let Some(own) = language.item_lines(text, pointer) {
        last = last.max(paths::line_column(text, own.end).0 - 1);
    }
    // A container opened on its lines runs to where it closes.
    while last + 1 < lines.len() && language.unclosed_containers(&lines[line..=last].join("\n")) > 0
    {
        last += 1;
    }
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
