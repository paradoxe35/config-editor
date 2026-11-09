//! A placeholder follows its field when the path to it changes but not
//! the field: a list item deleted or inserted before it, or its item
//! moved, changes the index in the field's path (`/env/1/value` is then
//! `/env/0/value`); a key above it renamed changes that key
//! (`/db/password` is then `/database/password`). An item or entry is told
//! by its whole value, the placeholder included: the list holding exactly
//! one item equal to the host's is that item, and the map that no longer
//! holds the host's key but exactly one entry equal to its value holds it
//! under the new key; so a placeholder pasted into another item (whose
//! value is then new) is still that item's plain text. The field's own key
//! renamed makes it another field: there the placeholder is plain text.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use serde_json::Value;

use super::marks::escape;
use super::tokens::Known;

/// A print of a value: equal values print alike.
fn print(value: &Value) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.to_string().hash(&mut hasher);
    hasher.finish()
}

/// The prints of the list items and map entries on the way to `pointer`
/// in `document` (the field's own entry not counted), outermost first,
/// each printed once for all the pointers read with `printed` (the item's
/// pointer and its print).
pub(super) fn item_prints_in(
    document: &Value,
    pointer: &str,
    printed: &mut HashMap<String, u64>,
) -> Vec<u64> {
    let mut prints = Vec::new();
    let mut current = document;
    let mut at = String::new();
    let segments = segments(pointer);
    let last = segments.len().saturating_sub(1);
    for (depth, segment) in segments.into_iter().enumerate() {
        at = format!("{at}/{}", escape(&segment));
        let next = match current {
            Value::Array(items) => segment
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get(index)),
            Value::Object(map) => map.get(&segment),
            _ => None,
        };
        let Some(next) = next else {
            break;
        };
        if current.is_array() || depth < last {
            prints.push(*printed.entry(at.clone()).or_insert_with(|| print(next)));
        }
        current = next;
    }
    prints
}

/// The index of the one entry of `entries` whose print is `wanted`, the
/// entries printed once per container (kept in `by_print`).
fn the_one<'a>(
    by_print: &mut HashMap<String, HashMap<u64, Vec<usize>>>,
    container: &str,
    entries: impl Iterator<Item = &'a Value>,
    wanted: u64,
) -> Option<usize> {
    let prints = by_print.entry(container.to_string()).or_insert_with(|| {
        let mut prints: HashMap<u64, Vec<usize>> = HashMap::new();
        for (index, entry) in entries.enumerate() {
            prints.entry(print(entry)).or_default().push(index);
        }
        prints
    });
    match prints.get(&wanted).map(Vec::as_slice) {
        Some([index]) => Some(*index),
        _ => None,
    }
}

fn segments(pointer: &str) -> Vec<String> {
    pointer
        .split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// `known` with each field in a list item that `document` holds at
/// another index (the one item equal to the host's) at that index, and
/// each under a key `document` no longer holds under the one key of that
/// map holding the same value, its prints read again; `None` when no
/// field moved.
pub(crate) fn followed(document: &Value, known: &[Known]) -> Option<Vec<Known>> {
    // Each list's items by print, read once per list.
    let mut lists: HashMap<String, HashMap<u64, Vec<usize>>> = HashMap::new();
    let mut printed: HashMap<String, u64> = HashMap::new();
    let mut moved = false;
    let mut out = known.to_vec();
    for known in &mut out {
        let Some(field) = &known.field else {
            continue;
        };
        if known.items.is_empty() {
            continue;
        }
        let mut current = document;
        let mut pointer = String::new();
        let mut prints = known.items.iter();
        let mut changed = false;
        let field_segments = segments(field);
        let last = field_segments.len().saturating_sub(1);
        for (depth, segment) in field_segments.into_iter().enumerate() {
            let segment = match current {
                Value::Array(items) => {
                    let Some(&wanted) = prints.next() else {
                        break;
                    };
                    let index = segment.parse::<usize>().ok();
                    let here = index
                        .and_then(|index| items.get(index))
                        .is_some_and(|item| {
                            *printed
                                .entry(format!("{pointer}/{segment}"))
                                .or_insert_with(|| print(item))
                                == wanted
                        });
                    if here {
                        segment
                    } else {
                        match the_one(&mut lists, &pointer, items.iter(), wanted) {
                            Some(index) => {
                                changed = true;
                                index.to_string()
                            }
                            None => break,
                        }
                    }
                }
                Value::Object(map) if depth < last => {
                    let Some(&wanted) = prints.next() else {
                        break;
                    };
                    if map.contains_key(&segment) {
                        segment
                    } else {
                        // Its key renamed: the one entry holding its value.
                        match the_one(&mut lists, &pointer, map.values(), wanted)
                            .and_then(|index| map.keys().nth(index))
                        {
                            Some(key) => {
                                changed = true;
                                key.clone()
                            }
                            None => break,
                        }
                    }
                }
                _ => segment,
            };
            let next = match current {
                Value::Array(items) => segment
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| items.get(index)),
                Value::Object(map) => map.get(&segment),
                _ => None,
            };
            pointer = format!("{pointer}/{}", escape(&segment));
            match next {
                Some(next) => current = next,
                None => break,
            }
        }
        if changed && segments(field).len() == segments(&pointer).len() {
            known.items = item_prints_in(document, &pointer, &mut printed);
            known.field = Some(pointer);
            moved = true;
        }
    }
    moved.then_some(out)
}
