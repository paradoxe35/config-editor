//! Map keys put in another order in a view: each entry's lines move with
//! the comment lines right above it, so the host reads its keys in the
//! view's order (round 26: the order never reached the host).

use super::super::language::ConfigLanguage;
use super::reorder::{comment_only, indent_of};
use super::{byte_range, comment_start, escape, key_markup_start, read};
use serde_json::Value;
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
    // One map at a time, each read from the text the one before wrote.
    let (mut text, mut current) = (previous.to_string(), old.clone());
    for (pointer, order) in maps {
        let keys: Vec<String> = current
            .pointer(&pointer)?
            .as_object()?
            .keys()
            .cloned()
            .collect();
        text = moved_entries(language, &text, &pointer, &keys, &order)?;
        let map = current.pointer_mut(&pointer)?.as_object_mut()?;
        let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
        entries.sort_by_key(|(key, _)| order.iter().position(|other| other == key));
        map.extend(entries);
    }
    let reread = read(language, &text, schema).ok()?;
    let reread = language.spelled_like(&text, reread, new);
    (reread == *new && same_order(&reread, new)).then_some(text)
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

/// `text` with the entries of the map at `pointer` (its `keys`, in the
/// order written) rewritten in `order`: each entry's lines (its own-line
/// comments above it, its key's line, and the lines its value runs on)
/// in the place of the entry `order` puts there.
fn moved_entries(
    language: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    keys: &[String],
    order: &[String],
) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut slots: Vec<Range<usize>> = Vec::with_capacity(keys.len());
    for key in keys {
        let slot = entry_lines(
            language,
            text,
            &lines,
            &format!("{pointer}/{}", escape(key)),
        )?;
        // In the order written, each apart from the one before.
        if slots.last().is_some_and(|last| last.end > slot.start) {
            return None;
        }
        slots.push(slot);
    }
    let block = |key: &String| -> Option<&Range<usize>> {
        keys.iter()
            .position(|other| other == key)
            .map(|at| &slots[at])
    };
    let mut patched: Vec<String> = Vec::with_capacity(lines.len());
    let mut at = 0;
    for (slot, key) in slots.iter().zip(order) {
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
    Some(patched.join("\n"))
}

/// The lines (0-based, end exclusive) of the entry at `pointer`: the
/// comment lines right above its key, the key's line, and the lines its
/// value runs on (a block scalar, a container to where it closes, or the
/// lines indented under the key where indentation is structure). `None`
/// when the key shares its line with something before it.
fn entry_lines(
    language: &dyn ConfigLanguage,
    text: &str,
    lines: &[&str],
    pointer: &str,
) -> Option<Range<usize>> {
    let key = language.key_span(text, pointer)?;
    let line = key.line.checked_sub(1)?;
    let name = byte_range(text, key)?;
    let line_start = text[..name.start].rfind('\n').map_or(0, |at| at + 1);
    let opens = key_markup_start(language, text, name.start).unwrap_or(name.start);
    if !text[line_start..opens.min(name.start)].trim().is_empty() {
        return None;
    }
    let mut last = line;
    // A value over lines (a block scalar, a heredoc) ends where it ends.
    if let Some(value) = language
        .value_span(text, pointer)
        .and_then(|span| byte_range(text, span))
    {
        last = last.max(text[..value.end].matches('\n').count());
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
