//! Items pasted where their lines read otherwise (another indent, a JSON
//! item's comma) or after a switch, told by their data: each is the first
//! item cut, not told already, holding the same data under the same key.
//! The cut items are looked up by their key and a hash of their data, so
//! a paste compares its items with the cut ones holding the same only
//! (QA `SP37-1`: compared with every cut item, a large edit's save was
//! quadratic in its items).

use super::super::super::work_count::{Work, note};
use serde_json::Value;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// A hash of `value`, the same for equal values (a map's entries in any
/// order).
pub(super) fn value_hash(value: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    match value {
        Value::Null => 0u8.hash(&mut hasher),
        Value::Bool(flag) => (1u8, flag).hash(&mut hasher),
        Value::Number(number) => (2u8, number).hash(&mut hasher),
        Value::String(text) => (3u8, text).hash(&mut hasher),
        Value::Array(items) => {
            4u8.hash(&mut hasher);
            for item in items {
                value_hash(item).hash(&mut hasher);
            }
        }
        Value::Object(map) => {
            let entries = map.iter().fold(0u64, |sum, (key, child)| {
                let mut entry = DefaultHasher::new();
                (key, value_hash(child)).hash(&mut entry);
                sum.wrapping_add(entry.finish())
            });
            (5u8, map.len(), entries).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// The cut items holding data of one hash under one key, in order, and
/// how many of the first ones are told.
type Group = (Vec<usize>, usize);

/// For each of `pasted` (a key, `None` for a list item, and data), the
/// index of the first of `cut` (likewise; data `None` when unknown) not
/// told for an earlier one, holding the same data under the same key.
pub(super) fn told(
    cut: &[(Option<&str>, Option<&Value>)],
    pasted: &[(Option<&str>, &Value)],
) -> Vec<Option<usize>> {
    // The cut items by key and data's hash.
    let mut groups: HashMap<(Option<&str>, u64), Group> = HashMap::new();
    for (index, (key, value)) in cut.iter().enumerate() {
        if let Some(value) = value {
            groups
                .entry((*key, value_hash(value)))
                .or_default()
                .0
                .push(index);
        }
    }
    let mut taken = vec![false; cut.len()];
    pasted
        .iter()
        .map(|(key, value)| {
            note(Work::TrailStep);
            let (group, told) = groups.get_mut(&(*key, value_hash(value)))?;
            while group.get(*told).is_some_and(|index| taken[*index]) {
                *told += 1;
            }
            let index = group[*told..].iter().copied().find(|index| {
                !taken[*index] && {
                    note(Work::ItemComparison);
                    cut[*index].1 == Some(*value)
                }
            })?;
            taken[index] = true;
            Some(index)
        })
        .collect()
}
