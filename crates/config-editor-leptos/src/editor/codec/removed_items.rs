//! Items removed from a block list (the rest kept, in any order) where no
//! line patch can tell the items' lines apart (YAML `- !A` over the lines
//! of a tagged map, which no serializer writes): the kept items' lines,
//! comments and tags move into the first places, the rest go.

use super::super::language::ConfigLanguage;
use super::reorder::block_items;
use super::{escape, reads_as};
use serde_json::Value;

/// `previous` with the items `new` removes from one block list dropped.
pub(in super::super) fn patch_removed_items(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    let (pointer, before, after) = shrunk_list(old, new, String::new())?;
    let lines: Vec<&str> = previous.split('\n').collect();
    let slots = block_items(language, previous, &lines, &pointer)
        .filter(|slots| slots.len() == before.len())?;
    let mut used = vec![false; before.len()];
    let mut sources = Vec::with_capacity(after.len());
    for item in after {
        let from = (0..before.len()).find(|at| !used[*at] && before[*at] == *item)?;
        used[from] = true;
        sources.push(from);
    }
    let block = |slot: usize| &lines[slots[slot].0..=slots[slot].2];
    let mut patched: Vec<&str> = lines[..slots[0].0].to_vec();
    for (place, from) in sources.into_iter().enumerate() {
        // The lines between two items stay between the items in their
        // places.
        if place > 0 {
            patched.extend(&lines[slots[place - 1].2 + 1..slots[place].0]);
        }
        patched.extend(block(from));
    }
    patched.extend(&lines[slots[before.len() - 1].2 + 1..]);
    let patched = patched.join("\n");
    reads_as(language, &patched, schema, new).then_some(patched)
}

/// The one list `new` holds fewer items of than `old` (each kept item
/// one of `old`'s), all else equal: its pointer and both item lists.
fn shrunk_list<'a>(
    old: &'a Value,
    new: &'a Value,
    pointer: String,
) -> Option<(String, &'a [Value], &'a [Value])> {
    match (old, new) {
        (Value::Object(left), Value::Object(right))
            if left.len() == right.len() && left.keys().zip(right.keys()).all(|(a, b)| a == b) =>
        {
            let mut differing = left.iter().filter(|(key, value)| right[*key] != **value);
            let (key, value) = differing.next()?;
            if differing.next().is_some() {
                return None;
            }
            shrunk_list(value, &right[key], format!("{pointer}/{}", escape(key)))
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            let mut differing = (0..left.len()).filter(|index| left[*index] != right[*index]);
            let index = differing.next()?;
            if differing.next().is_some() {
                return None;
            }
            shrunk_list(&left[index], &right[index], format!("{pointer}/{index}"))
        }
        (Value::Array(left), Value::Array(right))
            if !right.is_empty() && right.len() < left.len() =>
        {
            Some((pointer, left, right))
        }
        _ => None,
    }
}
