//! The host value with the user's edits applied, from what the edit
//! language showed of it.

use super::super::codec::same_order;
use super::super::language::{ConfigLanguage, Repetition};
use serde_json::{Map, Value};

use super::super::work_count::{Work, note};

/// Counts a key visited, passing `keep` on.
fn visit(keep: bool) -> bool {
    note(Work::MergeKeyVisit);
    keep
}

/// The host value with the user's edits applied: where the edit equals what
/// the edit language showed, the host's original value is kept; elsewhere
/// the edit wins. Keys and items added or removed follow the edit.
///
/// `unshown` are the pointers of host values the edit language could not
/// show exactly (XML has no empty or nested lists). An edit inside one of
/// them that cannot be mapped back onto the host's value is ambiguous: the
/// `Err` names where, and the save waits rather than write the view's
/// approximation over the host's data.
pub(super) fn merge_changes(
    host: &Value,
    seen: &Value,
    edited: &Value,
    view: &View<'_>,
) -> Result<Value, String> {
    merge_at(host, seen, edited, "", view)
}

/// What the edit language could show of the host value.
pub(super) struct View<'a> {
    /// The pointers of host values it could not show exactly.
    pub(super) unshown: &'a [String],
    /// How it repeats items for a list (XML elements, HCL blocks), so a
    /// list cut down to one item shows as that item.
    pub(super) repetition: Repetition,
    /// Whether keys the edit holds in another order than it showed them
    /// were moved by the user, not by the edit language's writer.
    pub(super) moves: bool,
}

/// Whether the keys `edited` holds in another order than `seen` were moved
/// by the user: the edit language could write them in `seen`'s order (TOML
/// writes a key turned into a table after the plain keys, whatever was
/// typed where).
pub(super) fn moved_by_user(edit: &dyn ConfigLanguage, seen: &Value, edited: &Value) -> bool {
    if same_order(seen, edited) {
        return false;
    }
    let in_seen_order = ordered_like(edited, seen);
    let written = super::super::codec::serialize(edit, &in_seen_order, None)
        .ok()
        .and_then(|text| super::super::codec::read(edit, &text, None).ok());
    written.is_some_and(|written| !same_order(&written, edited))
}

/// `value` with each map's keys that `like` holds in `like`'s order, the
/// others after them.
fn ordered_like(value: &Value, like: &Value) -> Value {
    match (value, like) {
        (Value::Object(map), Value::Object(like)) => {
            let places: std::collections::HashMap<&String, usize> = like
                .keys()
                .enumerate()
                .map(|(index, key)| (key, index))
                .collect();
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by_key(|(key, _)| places.get(key).copied().unwrap_or(usize::MAX));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, child)| {
                        let child = match like.get(key) {
                            Some(like) => ordered_like(child, like),
                            None => child.clone(),
                        };
                        (key.clone(), child)
                    })
                    .collect(),
            )
        }
        (Value::Array(items), Value::Array(like)) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| match like.get(index) {
                    Some(like) => ordered_like(item, like),
                    None => item.clone(),
                })
                .collect(),
        ),
        (value, _) => value.clone(),
    }
}

fn merge_at(
    host: &Value,
    seen: &Value,
    edited: &Value,
    at: &str,
    view: &View<'_>,
) -> Result<Value, String> {
    let unshown = view.unshown;
    // Keys only put in another order are a change too.
    if seen == edited && (!view.moves || same_order(seen, edited)) {
        return Ok(host.clone());
    }
    let child = |key: &str| format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
    // The edit replaces the host's value whole: only where the view showed
    // all of it.
    let replaced = || {
        let hidden = (host.is_array() || host.is_object())
            && unshown
                .iter()
                .any(|pointer| pointer == at || pointer.starts_with(&format!("{at}/")));
        if hidden {
            Err(if at.is_empty() {
                "/".to_string()
            } else {
                at.to_string()
            })
        } else {
            Ok(edited.clone())
        }
    };
    match (host, seen, edited) {
        (Value::Object(host), Value::Object(seen), Value::Object(edited)) => {
            // In the host's key order (the edit language may order keys its
            // own way: TOML puts plain keys before tables).
            // A renamed key: gone from the edit, and a new key holding what
            // the edit showed for it. It keeps its place and the host's own
            // value (which the edit language may not show exactly).
            // (Each looked up in maps: a paste of thousands of keys into a
            // map of thousands is no pass over all of them per key.)
            // The host's keys the edit no longer has, by what the edit
            // language showed for them (its text), in the host's order.
            let mut gone: std::collections::HashMap<String, std::collections::VecDeque<&String>> =
                std::collections::HashMap::new();
            if edited.keys().any(|key| !host.contains_key(key)) {
                for old in host.keys().filter(|old| visit(!edited.contains_key(*old))) {
                    if let Some(shown) = seen.get(old) {
                        gone.entry(canonical(shown)).or_default().push_back(old);
                    }
                }
            }
            let mut renamed_to: std::collections::HashMap<&String, &String> =
                std::collections::HashMap::new();
            let mut renamed_keys: std::collections::HashSet<&String> =
                std::collections::HashSet::new();
            for (key, value) in edited {
                note(Work::MergeKeyVisit);
                if host.contains_key(key) || gone.is_empty() {
                    continue;
                }
                let from = gone
                    .get_mut(&canonical(value))
                    .and_then(|olds| olds.pop_front());
                if let Some(from) = from {
                    renamed_to.insert(from, key);
                    renamed_keys.insert(key);
                }
            }
            let mut entries: Vec<(String, Value)> = Vec::new();
            for (key, host_value) in host {
                note(Work::MergeKeyVisit);
                match (seen.get(key), edited.get(key)) {
                    (Some(seen_value), Some(value)) => {
                        let merged = merge_at(host_value, seen_value, value, &child(key), view)?;
                        entries.push((key.clone(), merged));
                    }
                    // Renamed by the user.
                    (Some(_), None) if let Some(to) = renamed_to.get(key) => {
                        entries.push(((*to).clone(), host_value.clone()));
                    }
                    // Removed by the user.
                    (Some(_), None) => {}
                    // Re-added after the edit language could not show it, or
                    // never shown: the edit wins, else the host's stays.
                    (None, Some(value)) => entries.push((key.clone(), value.clone())),
                    (None, None) => entries.push((key.clone(), host_value.clone())),
                }
            }
            // A new key goes after the key before it in the edit, so a renamed
            // key keeps its place: the new keys after each key already placed
            // (or at the start), in the edit's order.
            let mut added: std::collections::HashMap<Option<String>, Vec<(String, Value)>> =
                std::collections::HashMap::new();
            let mut anchor: Option<&String> = None;
            for (key, value) in edited {
                note(Work::MergeKeyVisit);
                if !host.contains_key(key) && !renamed_keys.contains(key) {
                    added
                        .entry(anchor.cloned())
                        .or_default()
                        .push((key.clone(), value.clone()));
                } else {
                    anchor = Some(key);
                }
            }
            if !added.is_empty() {
                let mut placed = added.remove(&None).unwrap_or_default();
                placed.reserve(entries.len());
                for entry in entries {
                    note(Work::MergeKeyVisit);
                    let after = added.remove(&Some(entry.0.clone()));
                    placed.push(entry);
                    placed.extend(after.into_iter().flatten());
                }
                entries = placed;
            }
            if view.moves {
                moved_as_edited(&mut entries, seen, edited);
            }
            Ok(Value::Object(entries.into_iter().collect()))
        }
        (Value::Array(host), Value::Array(seen), Value::Array(edited))
            if host.len() == seen.len() && seen.len() == edited.len() =>
        {
            let mut items = Vec::with_capacity(host.len());
            for (index, ((host, seen), edited)) in host.iter().zip(seen).zip(edited).enumerate() {
                items.push(merge_at(
                    host,
                    seen,
                    edited,
                    &child(&index.to_string()),
                    view,
                )?);
            }
            Ok(Value::Array(items))
        }
        // A view that writes nested lists flat (XML repeats elements): the
        // view's items are the host's leaves in order, so an item edited
        // there is merged into its leaf and the nesting stays.
        (Value::Array(host_items), Value::Array(seen), Value::Array(edited))
            if seen.len() == edited.len() =>
        {
            let mut leaves = Vec::new();
            array_leaves(host_items, &mut String::new(), &mut leaves);
            if leaves.len() != seen.len() {
                return replaced();
            }
            let mut merged = host.clone();
            for ((pointer, leaf), (seen, edited)) in leaves.iter().zip(seen.iter().zip(edited)) {
                if seen != edited
                    && let Some(slot) = merged.pointer_mut(pointer)
                {
                    *slot = merge_at(leaf, seen, edited, &format!("{at}{pointer}"), view)?;
                }
            }
            Ok(merged)
        }
        // A one-item list the view shows as its lone item (XML).
        (Value::Array(host), seen, edited)
            if host.len() == 1 && !seen.is_array() && !edited.is_array() =>
        {
            Ok(Value::Array(vec![merge_at(
                &host[0],
                seen,
                edited,
                &child("0"),
                view,
            )?]))
        }
        // A list the view showed as repeated items cut down to one, which
        // it shows as that item (an XML element, an HCL block): still a
        // list of it.
        (Value::Array(host_items), Value::Array(_), edited)
            if !host_items.iter().any(Value::is_array)
                && match view.repetition {
                    Repetition::Elements => !edited.is_array() && !edited.is_null(),
                    Repetition::Blocks => {
                        edited.is_object() && host_items.iter().all(Value::is_object)
                    }
                    Repetition::None => false,
                } =>
        {
            merge_at(host, seen, &Value::Array(vec![edited.clone()]), at, view)
        }
        _ => replaced(),
    }
}

/// `value` written so that equal values (`==`, whatever their maps' key
/// order) are written the same.
fn canonical(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(&String, String)> = map
                .iter()
                .map(|(key, item)| (key, canonical(item)))
                .collect();
            entries.sort_unstable();
            let body: Vec<String> = entries
                .into_iter()
                .map(|(key, item)| format!("{}:{item}", Value::String(key.clone())))
                .collect();
            format!("{{{}}}", body.join(","))
        }
        Value::Array(items) => {
            format!(
                "[{}]",
                items.iter().map(canonical).collect::<Vec<_>>().join(",")
            )
        }
        other => other.to_string(),
    }
}

/// The non-list items of `items`, nested lists read through, with their
/// pointers relative to `items`.
fn array_leaves<'a>(items: &'a [Value], prefix: &mut String, out: &mut Vec<(String, &'a Value)>) {
    for (index, item) in items.iter().enumerate() {
        let length = prefix.len();
        prefix.push_str(&format!("/{index}"));
        match item {
            Value::Array(nested) => array_leaves(nested, prefix, out),
            leaf => out.push((prefix.clone(), leaf)),
        }
        prefix.truncate(length);
    }
}

/// `value` with the keys of each map that `like` holds in another order
/// moved into `like`'s, each after the key before it there; keys `like`
/// does not hold stay where they are.
pub(super) fn keys_ordered_like(value: Value, like: &Value) -> Value {
    match (value, like) {
        (Value::Object(map), Value::Object(like_map)) => {
            let mut entries: Vec<(String, Value)> = map
                .into_iter()
                .map(|(key, child)| {
                    let child = match like_map.get(&key) {
                        Some(like) => keys_ordered_like(child, like),
                        None => child,
                    };
                    (key, child)
                })
                .collect();
            let shown: Map<String, Value> = entries
                .iter()
                .map(|(key, _)| (key.clone(), Value::Null))
                .collect();
            moved_as_edited(&mut entries, &shown, like_map);
            Value::Object(entries.into_iter().collect())
        }
        (Value::Array(items), Value::Array(like)) => Value::Array(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| match like.get(index) {
                    Some(like) => keys_ordered_like(item, like),
                    None => item,
                })
                .collect(),
        ),
        (value, _) => value,
    }
}

/// The most keys of one map whose moves in a view are followed.
const MOVED_KEYS_LIMIT: usize = 400;

/// `entries` (in the host's order) with the keys the edit moved among the
/// ones it showed (`seen` and `edited` hold them in other orders) each put
/// after the key before it in the edit: the keys that kept their order
/// stay where the host has them.
fn moved_as_edited(
    entries: &mut Vec<(String, Value)>,
    seen: &Map<String, Value>,
    edited: &Map<String, Value>,
) {
    let shown: Vec<&String> = seen
        .keys()
        .filter(|key| edited.contains_key(*key))
        .collect();
    let typed: Vec<&String> = edited
        .keys()
        .filter(|key| seen.contains_key(*key))
        .collect();
    // The common keys' table grows with their square: a map past a few
    // hundred keys keeps the host's order.
    if shown == typed || shown.len() > MOVED_KEYS_LIMIT {
        return;
    }
    let kept = longest_common(&shown, &typed);
    let moved: Vec<&String> = typed
        .iter()
        .copied()
        .filter(|key| !kept.contains(key))
        .collect();
    let mut taken: Vec<(String, Value)> = Vec::new();
    entries.retain(|(key, value)| {
        let is_moved = moved.contains(&key);
        if is_moved {
            taken.push((key.clone(), value.clone()));
        }
        !is_moved
    });
    for key in moved {
        let Some(at) = taken.iter().position(|(name, _)| name == key) else {
            continue;
        };
        let entry = taken.swap_remove(at);
        let before = edited
            .keys()
            .take_while(|other| *other != key)
            .filter(|other| entries.iter().any(|(name, _)| name == *other))
            .last();
        let place = before
            .and_then(|before| entries.iter().position(|(name, _)| name == before))
            .map_or(0, |index| index + 1);
        entries.insert(place, entry);
    }
}

/// The longest run of `left`'s keys that `right` holds in the same order.
fn longest_common<'a>(left: &[&'a String], right: &[&'a String]) -> Vec<&'a String> {
    let (rows, columns) = (left.len(), right.len());
    let mut table = vec![vec![0usize; columns + 1]; rows + 1];
    for row in (0..rows).rev() {
        for column in (0..columns).rev() {
            table[row][column] = if left[row] == right[column] {
                table[row + 1][column + 1] + 1
            } else {
                table[row + 1][column].max(table[row][column + 1])
            };
        }
    }
    let (mut row, mut column, mut common) = (0, 0, Vec::new());
    while row < rows && column < columns {
        if left[row] == right[column] {
            common.push(left[row]);
            row += 1;
            column += 1;
        } else if table[row + 1][column] >= table[row][column + 1] {
            row += 1;
        } else {
            column += 1;
        }
    }
    common
}
