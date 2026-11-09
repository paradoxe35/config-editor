//! A placeholder in a list item follows its item: an item deleted or
//! inserted before it, or the item moved, changes the index in the field's
//! path (`/env/1/value` is then `/env/0/value`), not the field. An item is
//! told by its whole value, the placeholder included: the list holding
//! exactly one item equal to the host's is that item, so a placeholder
//! pasted into another item (whose value is then new) is still that item's
//! plain text.

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

/// The prints of the list items on the way to `pointer` in `document`,
/// outermost first (empty when the path holds no list), each item printed
/// once for all the pointers read with `printed` (the item's pointer and
/// its print).
pub(super) fn item_prints_in(
    document: &Value,
    pointer: &str,
    printed: &mut HashMap<String, u64>,
) -> Vec<u64> {
    let mut prints = Vec::new();
    let mut current = document;
    let mut at = String::new();
    for segment in segments(pointer) {
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
        if current.is_array() {
            prints.push(*printed.entry(at.clone()).or_insert_with(|| print(next)));
        }
        current = next;
    }
    prints
}

fn segments(pointer: &str) -> Vec<String> {
    pointer
        .split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// `known` with each field in a list item that `document` holds at
/// another index (the one item equal to the host's) at that index, its
/// prints read again; `None` when no field moved.
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
        for segment in segments(field) {
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
                        let by_print = lists.entry(pointer.clone()).or_insert_with(|| {
                            let mut by_print: HashMap<u64, Vec<usize>> = HashMap::new();
                            for (index, item) in items.iter().enumerate() {
                                by_print.entry(print(item)).or_default().push(index);
                            }
                            by_print
                        });
                        match by_print.get(&wanted).map(Vec::as_slice) {
                            Some([index]) => {
                                changed = true;
                                index.to_string()
                            }
                            _ => break,
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
