//! The list items of a view's text and the lines each is written on: from
//! the first line holding one of its keys or values to the last. An item
//! sharing its first line with another (a flow list) has none: lines do
//! not tell such items apart.

use super::super::super::language::ConfigLanguage;
use super::super::super::parse_cache;
use super::super::super::paths;
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Range;

/// A list item of a view's text.
#[derive(Clone, Debug)]
pub(super) struct Item {
    pub(super) pointer: String,
    /// Its lines, from the start of the first.
    pub(super) lines: Range<usize>,
}

/// The list items of `text` in `language` with lines of their own, in
/// document order; `None` when the text does not read.
pub(super) fn items(language: &dyn ConfigLanguage, text: &str) -> Option<Vec<Item>> {
    let parsed = parse_cache::parsed(language, text);
    let value = parsed.as_ref().as_ref().ok()?;
    // Every value's pointer, each item's marked, in document order.
    let mut pointers = Vec::new();
    walk(value, &mut String::new(), false, &mut pointers);
    let (all, marked): (Vec<String>, Vec<bool>) = pointers.into_iter().unzip();
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
    // Items starting on one line are told apart by no line.
    let mut per_line: HashMap<usize, usize> = HashMap::new();
    for item in &found {
        *per_line.entry(item.lines.start).or_default() += 1;
    }
    found.retain(|item| per_line[&item.lines.start] == 1);
    Some(found)
}

fn walk(value: &Value, pointer: &mut String, item: bool, out: &mut Vec<(String, bool)>) {
    out.push((pointer.clone(), item));
    let length = pointer.len();
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                pointer.push('/');
                pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
                walk(child, pointer, false, out);
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
