//! The list items and map entries of a view's text and the lines each is
//! written on: from the first line holding one of its keys or values to
//! the last. An entry sharing its first line with another that is not
//! inside it (the items of a flow list) has none: lines do not tell such
//! entries apart.

use super::super::super::language::ConfigLanguage;
use super::super::super::paths;
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Range;

/// A list item or map entry of a view's text.
#[derive(Clone, Debug)]
pub(super) struct Item {
    pub(super) pointer: String,
    /// A map entry's key; `None` for a list item.
    pub(super) key: Option<String>,
    /// Its lines, from the start of the first.
    pub(super) lines: Range<usize>,
}

/// The list items and map entries of `text` in `language` with lines of
/// their own, in document order, from its value `value`.
pub(super) fn items(language: &dyn ConfigLanguage, text: &str, value: &Value) -> Vec<Item> {
    // Every value's pointer, each item's marked, in document order.
    let mut pointers = Vec::new();
    walk(value, &mut String::new(), None, &mut pointers);
    let (all, marked): (Vec<String>, Vec<Option<Option<String>>>) = pointers.into_iter().unzip();
    // Every span from one read of the text.
    let (values, keys): (Vec<_>, Vec<_>) = paths::with_spans(language, text, &all, || {
        all.iter()
            .map(|pointer| {
                (
                    paths::memo_value_span(language, text, pointer),
                    paths::memo_key_span(language, text, pointer),
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
        let Some(key) = &marked[index] else {
            continue;
        };
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
            key: key.clone(),
            lines: start..end,
        });
    }
    // Entries starting on one line are told apart by no line, unless all
    // but the first (in document order: the outermost) are inside it.
    let mut first_on_line: HashMap<usize, (&str, bool)> = HashMap::new();
    for item in &found {
        first_on_line
            .entry(item.lines.start)
            .and_modify(|(first, alone)| {
                *alone &= item
                    .pointer
                    .strip_prefix(*first)
                    .is_some_and(|rest| rest.starts_with('/'));
            })
            .or_insert((&item.pointer, true));
    }
    let kept: Vec<bool> = found
        .iter()
        .map(|item| first_on_line[&item.lines.start] == (item.pointer.as_str(), true))
        .collect();
    let mut kept = kept.into_iter();
    found.retain(|_| kept.next().unwrap_or(false));
    found
}

/// Each value's pointer under `pointer`, with `Some` of its key for a map
/// entry (`Some(None)` for a list item), `None` for the root.
fn walk(
    value: &Value,
    pointer: &mut String,
    entry: Option<Option<String>>,
    out: &mut Vec<(String, Option<Option<String>>)>,
) {
    out.push((pointer.clone(), entry));
    let length = pointer.len();
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                pointer.push('/');
                pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
                walk(child, pointer, Some(Some(key.clone())), out);
                pointer.truncate(length);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                pointer.push('/');
                pointer.push_str(&index.to_string());
                walk(child, pointer, Some(None), out);
                pointer.truncate(length);
            }
        }
        _ => {}
    }
}
