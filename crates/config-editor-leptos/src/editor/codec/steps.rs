//! A save holding several edits (a value changed, an item added elsewhere)
//! split into steps the line-preserving patches can each make: every value
//! change first, then one structural change at a time.

use super::super::item_sources::ItemSources;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// The values between `old` and `new`, ending with `new`: `old` with its
/// changed scalars set, then with each container whose keys or length
/// changed made like `new`'s, one at a time. Empty when there is one step.
pub(in super::super) fn steps(
    old: &Value,
    new: &Value,
    sources: Option<&ItemSources>,
) -> Vec<Value> {
    let mut steps = Vec::new();
    let scalars = with_scalar_changes(old, new);
    if scalars != *old && scalars != *new {
        steps.push(scalars);
    }
    let mut current = steps.last().cloned().unwrap_or_else(|| old.clone());
    // The keys the edit brought from elsewhere (cut and pasted, its links
    // tell) are added first, while each is still where it came from: the
    // save writes it from there, in its host's form (an XML element pasted over
    // another's line keeps its markup).
    if let Some(sources) = sources {
        let brought = with_brought(&current, new, "", sources);
        if brought != current && brought != *new {
            steps.push(brought.clone());
            current = brought;
        }
    }
    while let Some(next) = one_structural_step(&current, new, "") {
        steps.push(next.clone());
        current = next;
        if steps.len() > 64 {
            return Vec::new();
        }
    }
    if steps.len() < 2 { Vec::new() } else { steps }
}

/// `old` with the keys renamed that the save's links tell were renamed in
/// `new`, each holding what it held; `None` when they tell none.
pub(in super::super) fn renamed_by_links(
    old: &Value,
    new: &Value,
    sources: Option<&ItemSources>,
) -> Option<Value> {
    let renamed = with_told_renames(old, new, "", sources?);
    (renamed != *old).then_some(renamed)
}

/// `current` (at `pointer` of the save) with each key renamed that the
/// save's links tell was (a key `target` gains whose source is a key of
/// this map it loses, in the host's last text), holding the value it held,
/// in its place.
fn with_told_renames(
    current: &Value,
    target: &Value,
    pointer: &str,
    sources: &ItemSources,
) -> Value {
    match (current, target) {
        (Value::Object(left), Value::Object(right)) => {
            // Each lost key's new name.
            let mut new_names: HashMap<&String, &String> = HashMap::new();
            for key in right.keys().filter(|key| !left.contains_key(*key)) {
                let to = format!("{pointer}/{}", super::escape(key));
                let Some(super::super::item_sources::ItemSource::Previous(from)) =
                    sources.items.get(&to)
                else {
                    continue;
                };
                let Some((parent, old)) = from.rsplit_once('/') else {
                    continue;
                };
                if parent != pointer {
                    continue;
                }
                if let Some(old) = left
                    .keys()
                    .find(|lost| !right.contains_key(*lost) && super::escape(lost) == old)
                {
                    new_names.entry(old).or_insert(key);
                }
            }
            Value::Object(
                left.iter()
                    .map(|(key, value)| {
                        let name = new_names.get(key).copied().unwrap_or(key);
                        let child = format!("{pointer}/{}", super::escape(name));
                        let value = match right.get(name) {
                            Some(wanted) => with_told_renames(value, wanted, &child, sources),
                            None => value.clone(),
                        };
                        (name.clone(), value)
                    })
                    .collect(),
            )
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => Value::Array(
            left.iter()
                .zip(right)
                .enumerate()
                .map(|(index, (item, wanted))| {
                    with_told_renames(item, wanted, &format!("{pointer}/{index}"), sources)
                })
                .collect(),
        ),
        _ => current.clone(),
    }
}

/// `old` with every scalar `new` changes where the shape stays (same keys,
/// same list lengths) set to `new`'s.
fn with_scalar_changes(old: &Value, new: &Value) -> Value {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => Value::Object(
            left.iter()
                .map(|(key, value)| {
                    let value = match right.get(key) {
                        Some(other) => with_scalar_changes(value, other),
                        None => value.clone(),
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => Value::Array(
            left.iter()
                .zip(right)
                .map(|(a, b)| with_scalar_changes(a, b))
                .collect(),
        ),
        (left, right)
            if !left.is_object() && !left.is_array() && !right.is_object() && !right.is_array() =>
        {
            right.clone()
        }
        (left, _) => left.clone(),
    }
}

/// `current` with its first container that differs from `target` in keys or
/// length made like it (keys kept with their current values), or with a
/// value that changed kind replaced; `None` when they are equal.
fn one_structural_step(current: &Value, target: &Value, pointer: &str) -> Option<Value> {
    if current == target {
        return None;
    }
    match (current, target) {
        (Value::Object(left), Value::Object(right)) => {
            let same_keys =
                left.len() == right.len() && left.keys().all(|key| right.contains_key(key));
            if !same_keys {
                if let Some(step) = renames_apart(left, right) {
                    return Some(Value::Object(step));
                }
                let rebuilt: Map<String, Value> = right
                    .iter()
                    .map(|(key, value)| (key.clone(), left.get(key).unwrap_or(value).clone()))
                    .collect();
                return Some(Value::Object(rebuilt));
            }
            for (key, value) in left {
                let child = format!("{pointer}/{}", super::escape(key));
                if let Some(next) = one_structural_step(value, &right[key], &child) {
                    let mut map = left.clone();
                    map.insert(key.clone(), next);
                    return Some(Value::Object(map));
                }
            }
            None
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, value) in left.iter().enumerate() {
                let child = format!("{pointer}/{index}");
                if let Some(next) = one_structural_step(value, &right[index], &child) {
                    let mut items = left.clone();
                    items[index] = next;
                    return Some(Value::Array(items));
                }
            }
            None
        }
        _ => Some(target.clone()),
    }
}

/// `current` (at `pointer` of the save) with each key `target` gains that
/// the edit brought from elsewhere (its link, in `sources`, names no key
/// the map loses: no rename) added where `target` has it, the keys the map
/// loses still where they were.
fn with_brought(current: &Value, target: &Value, pointer: &str, sources: &ItemSources) -> Value {
    match (current, target) {
        (Value::Object(left), Value::Object(right)) => {
            let lost: Vec<(usize, &String)> = left
                .keys()
                .enumerate()
                .filter(|(_, key)| !right.contains_key(*key))
                .collect();
            let renamed = |from: &str| {
                from.rsplit_once('/').is_some_and(|(parent, key)| {
                    parent == pointer && lost.iter().any(|(_, lost)| super::escape(lost) == key)
                })
            };
            let brought = |key: &String| {
                !left.contains_key(key)
                    && sources
                        .items
                        .get(&format!("{pointer}/{}", super::escape(key)))
                        .is_some_and(|source| !renamed(source.pointer()))
            };
            let places: HashMap<&String, usize> = left
                .keys()
                .enumerate()
                .map(|(place, key)| (key, place))
                .collect();
            let mut out = Map::new();
            let mut next_lost = 0;
            for (key, value) in right {
                if let Some(&place) = places.get(key) {
                    while next_lost < lost.len() && lost[next_lost].0 < place {
                        let lost_key = lost[next_lost].1;
                        out.insert(lost_key.clone(), left[lost_key].clone());
                        next_lost += 1;
                    }
                    let child = format!("{pointer}/{}", super::escape(key));
                    out.insert(
                        key.clone(),
                        with_brought(&left[key], value, &child, sources),
                    );
                } else if brought(key) {
                    out.insert(key.clone(), value.clone());
                }
            }
            for (_, lost_key) in &lost[next_lost..] {
                out.insert((*lost_key).clone(), left[*lost_key].clone());
            }
            Value::Object(out)
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => Value::Array(
            left.iter()
                .zip(right)
                .enumerate()
                .map(|(index, (item, wanted))| {
                    with_brought(item, wanted, &format!("{pointer}/{index}"), sources)
                })
                .collect(),
        ),
        _ => current.clone(),
    }
}

/// Keys renamed in a map that also loses or gains others, made a step of
/// their own so the rename patch makes them in place (an XML attribute
/// renamed stays one, which a rewrite of the map would write as an
/// element): `left` without the keys removed outright first, then with the
/// renamed keys renamed where they are. A key removed and one added
/// holding the same value, in order, are a rename. `None` when the map
/// only renames, or renames nothing.
fn renames_apart(
    left: &Map<String, Value>,
    right: &Map<String, Value>,
) -> Option<Map<String, Value>> {
    let removed: Vec<&String> = left
        .keys()
        .filter(|key| !right.contains_key(*key))
        .collect();
    let added: Vec<&String> = right
        .keys()
        .filter(|key| !left.contains_key(*key))
        .collect();
    let mut renames: Vec<(&String, &String)> = Vec::new();
    let mut from = 0;
    for old_key in &removed {
        if let Some(offset) = added[from..]
            .iter()
            .position(|new_key| right[*new_key] == left[*old_key])
        {
            renames.push((old_key, added[from + offset]));
            from += offset + 1;
        }
    }
    if renames.is_empty() {
        return None;
    }
    let renamed = |key: &String| {
        renames
            .iter()
            .find(|(old, _)| *old == key)
            .map(|(_, new)| *new)
    };
    if renames.len() < removed.len() {
        return Some(
            left.iter()
                .filter(|(key, _)| right.contains_key(*key) || renamed(key).is_some())
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        );
    }
    (renames.len() < added.len()).then(|| {
        left.iter()
            .map(|(key, value)| (renamed(key).unwrap_or(key).clone(), value.clone()))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::steps;
    use serde_json::json;

    #[test]
    fn a_save_of_several_edits_splits_into_values_then_one_structure_at_a_time() {
        let old = json!({"listen": 1, "udp": {"max_peers": 2}, "self": {"allow": ["a"]}, "pool": {"min": 1}});
        let new = json!({"listen": 2, "udp": {"max_peers": 2}, "self": {"allow": ["a", "b"]}, "pool": {"min": 1, "max": 3}});
        let steps = steps(&old, &new, None);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0]["listen"], 2);
        assert_eq!(steps[0]["self"]["allow"], json!(["a"]));
        assert_eq!(steps[1]["self"]["allow"], json!(["a", "b"]));
        assert_eq!(steps[1]["pool"], json!({"min": 1}));
        assert_eq!(steps[2], new);
        // One edit: no steps.
        assert!(super::steps(&old, &json!({"listen": 1, "udp": {"max_peers": 3}, "self": {"allow": ["a"]}, "pool": {"min": 1}}), None).is_empty());
    }
}
