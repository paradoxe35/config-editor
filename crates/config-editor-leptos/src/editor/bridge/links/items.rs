//! The list items and map entries of a view's text and the lines each is
//! written on: from the first line holding one of its keys or values to
//! the last. Of entries sharing a first line that are not each inside the
//! one before (the items of a flow list), only the outermost has lines:
//! lines do not tell the others apart.

use super::super::super::codec;
use super::super::super::language::{ConfigLanguage, pointer_key};
use super::super::super::paths;
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Range;

/// A list item or map entry of a view's text.
#[derive(Clone, Debug)]
pub(super) struct Item {
    pub(super) pointer: String,
    /// Its lines, from the start of the first.
    pub(super) lines: Range<usize>,
}

/// The list items and map entries of `text` in `language` with lines of
/// their own, in document order, from its value `value`.
pub(super) fn items(language: &dyn ConfigLanguage, text: &str, value: &Value) -> Vec<Item> {
    // Every value's pointer, each item's marked, in document order.
    let mut pointers = Vec::new();
    walk(value, &mut String::new(), false, &mut pointers);
    let (all, marked): (Vec<String>, Vec<bool>) = pointers.into_iter().unzip();
    // Every span from one read of the text.
    let (values, keys): (Vec<_>, Vec<_>) = paths::with_spans(language, text, &all, || {
        all.iter()
            .map(|pointer| {
                // A member of a container written with brackets over lines
                // (an item of an HCL or TOML list) is found in it.
                (
                    paths::memo_value_span(language, text, pointer)
                        .or_else(|| codec::inline_location(language, text, pointer, false)),
                    paths::memo_key_span(language, text, pointer)
                        .or_else(|| codec::inline_location(language, text, pointer, true)),
                )
            })
            .unzip()
    });
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    // The lines (from 1) each pointer's own key and value are on.
    let lines: Vec<Option<(usize, usize)>> = values
        .iter()
        .zip(&keys)
        .map(|(value, key)| {
            let lines = value.iter().chain(key.iter()).map(|span| span.line);
            Some((lines.clone().min()?, lines.max()?))
        })
        .collect();
    let mut found = Vec::new();
    for (index, pointer) in all.iter().enumerate() {
        if !marked[index] {
            continue;
        }
        let inside = format!("{pointer}/");
        let within = std::iter::once(&lines[index]).chain(
            all[index + 1..]
                .iter()
                .zip(&lines[index + 1..])
                .take_while(|(child, _)| child.starts_with(&inside))
                .map(|(_, lines)| lines),
        );
        let Some((first, last)) =
            within
                .flatten()
                .fold(None, |known: Option<(usize, usize)>, (first, last)| {
                    Some(known.map_or((*first, *last), |(low, high)| {
                        (low.min(*first), high.max(*last))
                    }))
                })
        else {
            continue;
        };
        let (Some(start), end) = (
            starts.get(first - 1).copied(),
            starts.get(last).copied().unwrap_or(text.len()),
        ) else {
            continue;
        };
        found.push(Item {
            pointer: pointer.clone(),
            lines: start..end,
        });
    }
    // Entries starting on one line are told apart by no line, unless all
    // but the first (in document order: the outermost) are inside it; and
    // then only the first, unless each is inside the one before (a list
    // whose first item starts on the list's line, an entry on its list
    // item's line: told apart by how far their lines go and how deep they
    // are), not siblings (a flow list's items).
    let inside = |inner: &str, outer: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('/'))
    };
    // Each line's first item, whether all are inside it, whether each is
    // inside the one before, and the last one seen.
    let mut on_line: HashMap<usize, (&str, bool, bool, &str)> = HashMap::new();
    for item in &found {
        on_line
            .entry(item.lines.start)
            .and_modify(|(first, alone, chain, last)| {
                *alone &= inside(&item.pointer, first);
                *chain &= inside(&item.pointer, last);
                *last = &item.pointer;
            })
            .or_insert((&item.pointer, true, true, &item.pointer));
    }
    let kept: Vec<bool> = found
        .iter()
        .map(|item| {
            let (first, alone, chain, _) = on_line[&item.lines.start];
            alone && (chain || first == item.pointer)
        })
        .collect();
    let mut kept = kept.into_iter();
    found.retain(|_| kept.next().unwrap_or(false));
    found
}

/// Each value's pointer under `pointer`, with whether it is an item (a
/// map entry or list item; not the root).
fn walk(value: &Value, pointer: &mut String, item: bool, out: &mut Vec<(String, bool)>) {
    out.push((pointer.clone(), item));
    let length = pointer.len();
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                pointer.push('/');
                pointer.push_str(&pointer_key(key));
                walk(child, pointer, true, out);
                pointer.truncate(length);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                pointer.push('/');
                pointer.push_str(&index.to_string());
                walk(child, pointer, true, out);
                pointer.truncate(length);
            }
        }
        _ => {}
    }
}
