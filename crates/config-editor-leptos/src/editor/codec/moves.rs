//! Lists whose items moved while others were edited, added or removed,
//! saved in stages: the kept items moved first (each with its comment
//! lines, as [`super::patch_reorder`] moves them), then the moved items
//! edited where they now are, then the removed ones dropped, then the new
//! ones added. Kept items only shifted by items removed and others added
//! (`[a, b]` made `[b, c]`) are saved with the new items added first, then
//! the kept ones edited, then the removed ones dropped (a list left with
//! one item on the way would read as that item where items repeat). Each
//! stage is one kind of change the patches make line by line, so a comment
//! stays with its item through all of them.

use serde_json::{Map, Value};

use super::super::item_sources::Pairing;

/// The values between `old` and `new` (paired by `pairing`) when some list of `old` holds items
/// `new` keeps in another order (some of them edited) and `new` also edits,
/// adds or removes items there: `old` with those items moved, then edited,
/// then without the removed ones (`new` is the last stage). Empty when no
/// list both moves and changes otherwise.
pub(in super::super) fn stages(old: &Value, new: &Value, pairing: &Pairing<'_>) -> Vec<Value> {
    let mut found = false;
    let staged = staged(old, new, (&mut String::new(), pairing), &mut found);
    if !found {
        return Vec::new();
    }
    let mut out: Vec<Value> = Vec::new();
    for stage in staged {
        if stage != *old && stage != *new && out.last() != Some(&stage) {
            out.push(stage);
        }
    }
    out
}

/// The stages of [`stages`]: `old` with the kept items of each list `new`
/// reorders and changes otherwise in `new`'s order, in the places kept
/// items held (removed items stay where they are); that with the kept items
/// edited as `new` has them; and that without the removed items. `found` is
/// set when such a list is found. An item `new` changes in place stays
/// itself.
fn staged(
    old: &Value,
    new: &Value,
    (pointer, pairing): (&mut String, &Pairing<'_>),
    found: &mut bool,
) -> [Value; 3] {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            let mut maps = [Map::new(), Map::new(), Map::new()];
            for (key, value) in left {
                let length = pointer.len();
                pointer.push('/');
                pointer.push_str(&super::escape(key));
                let stages = match right.get(key) {
                    Some(other) => staged(value, other, (pointer, pairing), found),
                    None => [value.clone(), value.clone(), value.clone()],
                };
                pointer.truncate(length);
                for (map, stage) in maps.iter_mut().zip(stages) {
                    map.insert(key.clone(), stage);
                }
            }
            maps.map(Value::Object)
        }
        (Value::Array(left), Value::Array(right)) => {
            let matched = pairing.list(pointer, left, right);
            let kept: Vec<(usize, usize)> = matched
                .iter()
                .enumerate()
                .filter_map(|(slot, from)| Some(((*from)?, slot)))
                .collect();
            let moves = kept.windows(2).any(|pair| pair[0].0 > pair[1].0);
            // Kept items in other places (others removed or added before
            // them): written in place, each would take the comments of the
            // item that was there.
            let shifted = kept.iter().any(|(from, slot)| from != slot);
            let edited = kept.iter().any(|(from, slot)| left[*from] != right[*slot]);
            let resized = kept.len() < left.len().max(right.len());
            if !moves && shifted && resized {
                *found = true;
                let grown = grown(left, right, &matched);
                let edited = grown
                    .iter()
                    .map(|(item, slot)| slot.map_or(*item, |slot| &right[slot]).clone())
                    .collect();
                let grown = grown.into_iter().map(|(item, _)| item.clone()).collect();
                return [
                    Value::Array(grown),
                    Value::Array(edited),
                    Value::Array(right.clone()),
                ];
            }
            if moves && (edited || resized) {
                *found = true;
                let mut places: Vec<usize> = kept.iter().map(|(from, _)| *from).collect();
                places.sort_unstable();
                let (mut moved, mut changed) = (left.clone(), left.clone());
                for (place, (from, slot)) in places.into_iter().zip(&kept) {
                    moved[place] = left[*from].clone();
                    changed[place] = right[*slot].clone();
                }
                let trimmed = kept.iter().map(|(_, slot)| right[*slot].clone()).collect();
                return [
                    Value::Array(moved),
                    Value::Array(changed),
                    Value::Array(trimmed),
                ];
            }
            if left.len() == right.len() {
                let mut lists = [Vec::new(), Vec::new(), Vec::new()];
                for (index, (item, other)) in left.iter().zip(right).enumerate() {
                    let length = pointer.len();
                    pointer.push('/');
                    pointer.push_str(&index.to_string());
                    let stages = staged(item, other, (pointer, pairing), found);
                    pointer.truncate(length);
                    for (list, stage) in lists.iter_mut().zip(stages) {
                        list.push(stage);
                    }
                }
                return lists.map(Value::Array);
            }
            [old.clone(), old.clone(), old.clone()]
        }
        _ => [old.clone(), old.clone(), old.clone()],
    }
}

/// `old`'s items with `new`'s added ones among them, each kept item (by
/// `matched`, in order) where it is: every item with the slot of `new` it
/// is, when it is kept or added.
fn grown<'a>(
    old: &'a [Value],
    new: &'a [Value],
    matched: &[Option<usize>],
) -> Vec<(&'a Value, Option<usize>)> {
    let mut out = Vec::with_capacity(old.len() + new.len());
    let mut next = 0;
    for (slot, item) in new.iter().enumerate() {
        match matched[slot] {
            Some(from) => {
                out.extend(old[next..from].iter().map(|removed| (removed, None)));
                out.push((&old[from], Some(slot)));
                next = from + 1;
            }
            None => out.push((item, Some(slot))),
        }
    }
    out.extend(old[next..].iter().map(|removed| (removed, None)));
    out
}

#[cfg(test)]
mod tests {
    use crate::editor::item_sources::Pairing;
    use serde_json::{Value, json};

    fn stages(old: &Value, new: &Value) -> Vec<Value> {
        super::stages(old, new, &Pairing::of(Some(old), new, false))
    }

    #[test]
    fn a_list_moved_and_resized_is_saved_moved_then_trimmed_then_added() {
        let old = json!({"tags": ["web", "api"], "n": 1});
        assert_eq!(
            stages(&old, &json!({"tags": ["api", "web", "new"], "n": 2})),
            vec![json!({"tags": ["api", "web"], "n": 1})]
        );
        let old = json!({"l": ["a", "b", "c"]});
        assert_eq!(
            stages(&old, &json!({"l": ["c", "x", "a"]})),
            vec![json!({"l": ["c", "b", "a"]}), json!({"l": ["c", "a"]})]
        );
        // Shifted by one removed and one added: added first.
        assert_eq!(
            stages(&old, &json!({"l": ["b", "c", "d"]})),
            vec![json!({"l": ["a", "b", "c", "d"]})]
        );
        assert_eq!(
            stages(&old, &json!({"l": ["x", "a", "cc"]})),
            vec![
                json!({"l": ["x", "a", "b", "c"]}),
                json!({"l": ["x", "a", "b", "cc"]}),
            ]
        );
        // Only moved, only resized, or an item edited in place: one stage.
        assert!(stages(&old, &json!({"l": ["c", "b", "a"]})).is_empty());
        assert!(stages(&old, &json!({"l": ["b", "c"]})).is_empty());
        assert!(stages(&old, &json!({"l": ["x", "a", "b", "c"]})).is_empty());
        assert!(stages(&old, &json!({"l": ["a", "b", "x"]})).is_empty());
        // An item edited to equal the next, which stays: edited in place.
        assert!(stages(&old, &json!({"l": ["a", "c", "c"]})).is_empty());
        assert!(stages(&old, &json!({"l": ["a", "b", "c", "d"]})).is_empty());
        assert!(stages(&old, &json!({"l": ["a", "x", "c"]})).is_empty());
    }

    #[test]
    fn an_item_edited_as_it_moves_is_moved_then_edited() {
        let old = json!({"l": ["a", "b", "c"]});
        assert_eq!(
            stages(&old, &json!({"l": ["c", "a", "bb"]})),
            vec![json!({"l": ["c", "a", "b"]})]
        );
        let old = json!({"l": [{"n": "a", "p": 1}, {"n": "b", "p": 2}]});
        assert_eq!(
            stages(
                &old,
                &json!({"l": [{"n": "b", "p": 20}, {"n": "a", "p": 1}]})
            ),
            vec![json!({"l": [{"n": "b", "p": 2}, {"n": "a", "p": 1}]})]
        );
        // Moved, edited, one removed, one added.
        let old = json!({"l": ["a", "b", "c", "d"]});
        assert_eq!(
            stages(&old, &json!({"l": ["dd", "x", "a"]})),
            vec![
                json!({"l": ["d", "b", "c", "a"]}),
                json!({"l": ["dd", "b", "c", "a"]}),
                json!({"l": ["dd", "a"]}),
            ]
        );
    }
}
