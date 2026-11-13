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

use std::collections::{HashMap, VecDeque};

use serde_json::{Map, Value};

use super::super::work_count::{Work, note};
use super::canonical;

/// The values between `old` and `new` when some list of `old` holds items
/// `new` keeps in another order (some of them edited) and `new` also edits,
/// adds or removes items there: `old` with those items moved, then edited,
/// then without the removed ones (`new` is the last stage). Empty when no
/// list both moves and changes otherwise.
pub(in super::super) fn stages(old: &Value, new: &Value) -> Vec<Value> {
    let mut found = false;
    let staged = staged(old, new, &mut found);
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

/// Which item of `old` each item of `new` is: the equal one in its place,
/// else the first unused equal one, else the unused one most like it (an
/// item edited as it moved).
fn matching(old: &[Value], new: &[Value]) -> Vec<Option<usize>> {
    let mut used = vec![false; old.len()];
    let mut found: Vec<Option<usize>> = new
        .iter()
        .enumerate()
        .map(|(slot, item)| {
            note(Work::ItemComparison);
            let stays = old.get(slot) == Some(item);
            if stays {
                used[slot] = true;
            }
            stays.then_some(slot)
        })
        .collect();
    // The unused items of `old` by what they are, in order: each new item
    // finds its first unused equal one in a lookup.
    let mut equal: HashMap<String, VecDeque<usize>> = HashMap::new();
    if found.iter().any(Option::is_none) {
        for (at, item) in old.iter().enumerate().filter(|(at, _)| !used[*at]) {
            note(Work::ItemComparison);
            equal.entry(canonical(item)).or_default().push_back(at);
        }
    }
    for (slot, item) in new.iter().enumerate() {
        if found[slot].is_some() {
            continue;
        }
        note(Work::ItemComparison);
        if let Some(at) = equal
            .get_mut(&canonical(item))
            .and_then(VecDeque::pop_front)
        {
            used[at] = true;
            found[slot] = Some(at);
        }
    }
    // The likest pairs first, the nearest of equally like ones. Every
    // unmatched pair is compared up to a budget; past it (thousands of items
    // edited at once, a replace-all) each new item is compared with the
    // unused ones nearest its place, as many as the budget allows.
    let open: Vec<usize> = (0..new.len())
        .filter(|slot| found[*slot].is_none())
        .collect();
    let free: Vec<usize> = (0..old.len()).filter(|at| !used[*at]).collect();
    let reach = if open.len().saturating_mul(free.len()) <= PAIRS_COMPARED {
        free.len()
    } else {
        (PAIRS_COMPARED / open.len().max(1) / 2).max(1)
    };
    let mut pairs: Vec<(usize, usize, usize)> = Vec::new();
    for &slot in &open {
        // The unused items around the same place.
        let middle = free.partition_point(|at| *at < slot);
        let from = middle.saturating_sub(reach);
        let to = (middle + reach).min(free.len());
        for &at in &free[from..to] {
            note(Work::ItemComparison);
            let score = likeness(&old[at], &new[slot]);
            if score > 0 {
                pairs.push((score, at, slot));
            }
        }
    }
    pairs.sort_by_key(|(score, at, slot)| (std::cmp::Reverse(*score), at.abs_diff(*slot), *slot));
    for (_, at, slot) in pairs {
        if !used[at] && found[slot].is_none() {
            used[at] = true;
            found[slot] = Some(at);
        }
    }
    found
}

/// The most pairs of unmatched items whose likeness one list's save
/// compares (see [`matching`]).
const PAIRS_COMPARED: usize = 250_000;

/// What one scalar `new` keeps of `old` whole counts: more than any part.
const WHOLE: usize = 1000;

/// How much `new` still holds of `old`: each scalar a map (under the same
/// key) or a list keeps counts [`WHOLE`], a string edited the characters
/// its text keeps at its start and end. Zero when nothing ties them.
fn likeness(old: &Value, new: &Value) -> usize {
    match (old, new) {
        _ if old == new => WHOLE * leaves(old),
        (Value::Object(left), Value::Object(right)) => left
            .iter()
            .filter_map(|(key, value)| Some(likeness(value, right.get(key)?)))
            .sum(),
        (Value::Array(left), Value::Array(right)) => {
            left.iter()
                .filter(|item| right.contains(item))
                .map(leaves)
                .sum::<usize>()
                * WHOLE
        }
        (Value::String(left), Value::String(right)) => {
            let prefix = left
                .chars()
                .zip(right.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let shorter = left.chars().count().min(right.chars().count());
            let suffix = left
                .chars()
                .rev()
                .zip(right.chars().rev())
                .take_while(|(a, b)| a == b)
                .count()
                .min(shorter - prefix);
            (prefix + suffix).min(WHOLE - 1)
        }
        // A number or flag changed tells nothing of what it was.
        _ => 0,
    }
}

/// The scalars in `value` (at least one).
fn leaves(value: &Value) -> usize {
    match value {
        Value::Object(map) => map.values().map(leaves).sum::<usize>().max(1),
        Value::Array(items) => items.iter().map(leaves).sum::<usize>().max(1),
        _ => 1,
    }
}

/// The stages of [`stages`]: `old` with the kept items of each list `new`
/// reorders and changes otherwise in `new`'s order, in the places kept
/// items held (removed items stay where they are); that with the kept items
/// edited as `new` has them; and that without the removed items. `found` is
/// set when such a list is found. An item `new` changes in place stays
/// itself.
fn staged(old: &Value, new: &Value, found: &mut bool) -> [Value; 3] {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            let mut maps = [Map::new(), Map::new(), Map::new()];
            for (key, value) in left {
                let stages = match right.get(key) {
                    Some(other) => staged(value, other, found),
                    None => [value.clone(), value.clone(), value.clone()],
                };
                for (map, stage) in maps.iter_mut().zip(stages) {
                    map.insert(key.clone(), stage);
                }
            }
            maps.map(Value::Object)
        }
        (Value::Array(left), Value::Array(right)) => {
            let matched = matching(left, right);
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
                for (item, other) in left.iter().zip(right) {
                    for (list, stage) in lists.iter_mut().zip(staged(item, other, found)) {
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
    use super::stages;
    use serde_json::{Value, json};

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

    /// Matching a list's items to the ones they were compares each a few
    /// times, however many: thousands of items edited at once (a
    /// replace-all in a view) compared every unmatched pair (3–4 s at 5,000),
    /// and each moved item was looked for among all the others.
    #[test]
    fn matching_many_items_compares_each_a_few_times() {
        use super::super::super::work_count::{Work, counted};
        use super::matching;
        fn items(count: usize, text: &str) -> Vec<Value> {
            (0..count)
                .map(|index| json!(format!("{text} {index}")))
                .collect()
        }
        /// A list before and after an edit of `count` items.
        type Lists = fn(usize) -> (Vec<Value>, Vec<Value>);
        let cases: [(&str, Lists); 3] = [
            ("every item edited", |count| {
                (items(count, "value"), items(count, "REPLACED"))
            }),
            ("every item moved", |count| {
                let old = items(count, "value");
                let new = old.iter().rev().cloned().collect();
                (old, new)
            }),
            ("every item moved and edited", |count| {
                let old = items(count, "value");
                let new = items(count, "REPLACED").into_iter().rev().collect();
                (old, new)
            }),
        ];
        let mut failures = Vec::new();
        for (name, case) in cases {
            let compared = |count: usize| {
                let (old, new) = case(count);
                let (found, compared) = counted(Work::ItemComparison, || matching(&old, &new));
                assert_eq!(found.len(), count);
                compared
            };
            let (few, many) = (compared(1_000), compared(10_000));
            if many > 12 * few {
                failures.push(format!(
                    "{name}: {few} comparisons for 1,000 items, {many} for 10,000"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
