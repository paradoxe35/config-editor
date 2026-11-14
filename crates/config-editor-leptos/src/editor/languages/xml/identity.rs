//! Which element of a host an element of a value written over it is: the
//! same key's element, and of repeated elements the item the edit kept
//! there, wherever it moved (items equal in both, in order, then those
//! moved, then the changed ones left in the same place between them). An
//! item the edit added, or one of two now where it had one copy, is no
//! host element. A rewrite takes each element's markup and layout from its
//! own host element (`attribute_layout`), and a save checks that it did
//! (`markup`).

use super::Seg;
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::HashMap;

/// An edit of `old` (the host's value) to `new`.
pub(super) struct Identity<'a> {
    old: Option<&'a Value>,
    new: &'a Value,
    /// Each list's items' places in the host's, by the list's pointer in
    /// `new`.
    lists: RefCell<HashMap<String, Vec<Option<usize>>>>,
}

impl<'a> Identity<'a> {
    /// `old` is `None` when the host does not read: each element is the
    /// host's at its own path.
    pub(super) fn of(old: Option<&'a Value>, new: &'a Value) -> Self {
        Identity { old, new, lists: RefCell::default() }
    }

    /// The host path (as the host's text writes it: an index only for a
    /// repeated element) of the element at `path` in a text written from
    /// `new`, or `None` for an element the edit added.
    pub(super) fn host_path(&self, path: &[Seg]) -> Option<Vec<String>> {
        let Some(mut old) = self.old else {
            return Some(super::strings(path));
        };
        let mut new = self.new;
        let mut pointer = String::new();
        let mut host = Vec::with_capacity(path.len());
        let mut at = 0;
        while at < path.len() {
            let Seg::Key(key) = &path[at] else { return None };
            let (Value::Object(new_map), Value::Object(old_map)) = (new, old) else { return None };
            let new_child = new_map.get(key)?;
            let old_key = host_key(old_map, new_map, key)?;
            let old_child = &old_map[old_key];
            pointer.push('/');
            pointer.push_str(key);
            let new_items = items(new_child);
            let index = match path.get(at + 1) {
                Some(Seg::Index(index)) => {
                    at += 1;
                    *index
                }
                _ if new_items.len() == 1 => 0,
                _ => return None,
            };
            let old_items = items(old_child);
            let place = {
                let mut lists = self.lists.borrow_mut();
                let places = lists.entry(pointer.clone()).or_insert_with(|| places(old_items, new_items));
                (*places.get(index)?)?
            };
            host.push(old_key.clone());
            if old_child.is_array() {
                host.push(place.to_string());
            }
            pointer.push('/');
            pointer.push_str(&index.to_string());
            old = &old_items[place];
            new = &new_items[index];
            at += 1;
        }
        Some(host)
    }
}

/// The key of `old` that `new`'s `key` is: itself, or the one key `new`
/// renamed when it renames one.
fn host_key<'m>(old: &'m Map<String, Value>, new: &Map<String, Value>, key: &str) -> Option<&'m String> {
    if let Some((held, _)) = old.get_key_value(key) {
        return Some(held);
    }
    let mut removed = old.keys().filter(|held| !new.contains_key(*held));
    let mut added = new.keys().filter(|added| !old.contains_key(*added));
    match (removed.next(), removed.next(), added.next(), added.next()) {
        (Some(held), None, Some(_), None) => Some(held),
        _ => None,
    }
}

/// A value as a list's items: a lone element is a list of one.
fn items(value: &Value) -> &[Value] {
    match value {
        Value::Array(items) => items,
        lone => std::slice::from_ref(lone),
    }
}

/// Lists longer than this (once their equal ends are set aside) are
/// matched item by item in order rather than by their longest common run.
const MATCHED_WHOLE: usize = 512 * 512;

/// For each item of `new`, the place of the item of `old` it is: the
/// longest run of items equal in both, in order; then an item equal to one
/// not taken (moved); then, between two of those, the items changed in
/// place (as many as there were, in order, or each the one sharing the most
/// with it). `None` for an item added.
fn places(old: &[Value], new: &[Value]) -> Vec<Option<usize>> {
    let mut places = vec![None; new.len()];
    let mut taken = vec![false; old.len()];
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..].iter().rev().zip(new[head..].iter().rev()).take_while(|(a, b)| a == b).count();
    for index in (0..head).chain(new.len() - tail..new.len()) {
        let place = if index < head { index } else { old.len() - (new.len() - index) };
        places[index] = Some(place);
        taken[place] = true;
    }
    let (old_middle, new_middle) = (head..old.len() - tail, head..new.len() - tail);
    for (from, to) in common_run(&old[old_middle.clone()], &new[new_middle.clone()]) {
        places[new_middle.start + to] = Some(old_middle.start + from);
        taken[old_middle.start + from] = true;
    }
    // Between two items kept in order, the rest: those changed in place.
    let anchors: Vec<usize> = (0..new.len()).filter(|index| places[*index].is_some()).collect();
    // Moved: equal to an item not taken.
    for index in new_middle.clone() {
        if places[index].is_none()
            && let Some(place) = (0..old.len()).find(|place| !taken[*place] && old[*place] == new[index])
        {
            places[index] = Some(place);
            taken[place] = true;
        }
    }
    let mut bounds = vec![(None, None)];
    for anchor in anchors {
        bounds.last_mut().expect("a gap").1 = Some(anchor);
        bounds.push((Some(anchor), None));
    }
    for (after, before) in bounds {
        let new_gap = after.map_or(0, |at| at + 1)..before.unwrap_or(new.len());
        let old_gap = after.and_then(|at| places[at]).map_or(0, |at| at + 1)
            ..before.and_then(|at| places[at]).unwrap_or(old.len());
        let unmatched_new: Vec<usize> = new_gap.filter(|index| places[*index].is_none()).collect();
        let unmatched_old: Vec<usize> = old_gap.filter(|place| !taken[*place]).collect();
        if unmatched_new.len() == unmatched_old.len() {
            for (index, place) in unmatched_new.into_iter().zip(unmatched_old) {
                places[index] = Some(place);
                taken[place] = true;
            }
            continue;
        }
        for index in unmatched_new {
            let best = unmatched_old
                .iter()
                .filter(|place| !taken[**place])
                .map(|place| (shared(&old[*place], &new[index]), *place))
                .filter(|(shared, _)| *shared > 0)
                .max_by_key(|(shared, place)| (*shared, std::cmp::Reverse(*place)));
            if let Some((_, place)) = best {
                places[index] = Some(place);
                taken[place] = true;
            }
        }
    }
    places
}

/// The pairs (place in `old`, place in `new`) of a longest run of items
/// equal in both, in order.
fn common_run(old: &[Value], new: &[Value]) -> Vec<(usize, usize)> {
    if old.is_empty() || new.is_empty() {
        return Vec::new();
    }
    if old.len() * new.len() > MATCHED_WHOLE {
        // In order, each with the next equal item.
        let mut pairs = Vec::new();
        let mut from = 0;
        for (to, item) in new.iter().enumerate() {
            if let Some(offset) = old[from..].iter().position(|held| held == item) {
                pairs.push((from + offset, to));
                from += offset + 1;
            }
        }
        return pairs;
    }
    let width = new.len() + 1;
    let mut lengths = vec![0u32; (old.len() + 1) * width];
    for from in (0..old.len()).rev() {
        for to in (0..new.len()).rev() {
            lengths[from * width + to] = if old[from] == new[to] {
                lengths[(from + 1) * width + to + 1] + 1
            } else {
                lengths[(from + 1) * width + to].max(lengths[from * width + to + 1])
            };
        }
    }
    let (mut from, mut to, mut pairs) = (0, 0, Vec::new());
    while from < old.len() && to < new.len() {
        if old[from] == new[to] {
            pairs.push((from, to));
            from += 1;
            to += 1;
        } else if lengths[(from + 1) * width + to] >= lengths[from * width + to + 1] {
            from += 1;
        } else {
            to += 1;
        }
    }
    pairs
}

/// How many of their keys two items hold alike.
fn shared(old: &Value, new: &Value) -> usize {
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => {
            new.iter().filter(|(key, value)| old.get(*key) == Some(*value)).count()
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::places;
    use serde_json::{Value, json};

    fn items(values: &[i64]) -> Vec<Value> {
        values.iter().map(|value| json!({"v": value, "k": value % 10})).collect()
    }

    #[test]
    fn items_keep_their_places_wherever_the_edit_moves_them() {
        let old = items(&[1, 2, 3]);
        let cases: [(&[i64], &[Option<usize>]); 9] = [
            (&[2, 3], &[Some(1), Some(2)]),
            (&[1, 3], &[Some(0), Some(2)]),
            (&[0, 1, 2, 3], &[None, Some(0), Some(1), Some(2)]),
            (&[1, 2, 2, 3], &[Some(0), Some(1), None, Some(2)]),
            (&[2, 1, 3], &[Some(1), Some(0), Some(2)]),
            (&[3, 1, 2], &[Some(2), Some(0), Some(1)]),
            // Changed in place, all of them.
            (&[11, 12, 13], &[Some(0), Some(1), Some(2)]),
            // One removed, one added elsewhere: no pair.
            (&[1, 3, 9], &[Some(0), Some(2), None]),
            // One removed beside one changed: the one sharing a key.
            (&[1, 13], &[Some(0), Some(2)]),
        ];
        for (new, wanted) in cases {
            assert_eq!(places(&old, &items(new)), wanted, "{new:?}");
        }
    }
}
