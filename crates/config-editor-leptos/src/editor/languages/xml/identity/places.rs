//! Which item of a host list each item of an edited list is, by their
//! values: where the edit does not tell (`item_sources`).

use serde_json::Value;

/// What the edit tells of an item of the edited list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Told {
    /// Nothing: its values tell.
    Nothing,
    /// It is the host item at this place.
    At(usize),
    /// It is no item of this host list (added, or one of another list).
    Elsewhere,
}

/// [`places`], each item the edit tells of where it tells it, the rest by
/// their values among the host items left.
pub(super) fn told_places(old: &[Value], new: &[Value], told: &[Told]) -> Vec<Option<usize>> {
    if told.iter().all(|told| *told == Told::Nothing) {
        return places(old, new);
    }
    let mut placed = vec![None; new.len()];
    let mut taken = vec![false; old.len()];
    for (index, told) in told.iter().enumerate() {
        if let Told::At(place) = told
            && *place < old.len()
            && !taken[*place]
        {
            placed[index] = Some(*place);
            taken[*place] = true;
        }
    }
    let rest_new: Vec<usize> = (0..new.len()).filter(|index| told[*index] == Told::Nothing).collect();
    let rest_old: Vec<usize> = (0..old.len()).filter(|place| !taken[*place]).collect();
    let values = |indices: &[usize], from: &[Value]| indices.iter().map(|index| from[*index].clone()).collect::<Vec<_>>();
    let by_values = places(&values(&rest_old, old), &values(&rest_new, new));
    for (at, place) in by_values.into_iter().enumerate() {
        placed[rest_new[at]] = place.map(|place| rest_old[place]);
    }
    placed
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
    let head = old.iter().zip(new).take_while(|(a, b)| alike(a, b)).count();
    let tail = old[head..].iter().rev().zip(new[head..].iter().rev()).take_while(|(a, b)| alike(a, b)).count();
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
            && let Some(place) = (0..old.len()).find(|place| !taken[*place] && alike(&old[*place], &new[index]))
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
            if let Some(offset) = old[from..].iter().position(|held| alike(held, item)) {
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
            lengths[from * width + to] = if alike(&old[from], &new[to]) {
                lengths[(from + 1) * width + to + 1] + 1
            } else {
                lengths[(from + 1) * width + to].max(lengths[from * width + to + 1])
            };
        }
    }
    let (mut from, mut to, mut pairs) = (0, 0, Vec::new());
    while from < old.len() && to < new.len() {
        if alike(&old[from], &new[to]) {
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

/// Whether two values hold the same data as XML writes it: scalars by
/// their text (the host reads `<n>2</n>` as a number a view may hold as a
/// string, and the reverse), so the writer and the save's check, reading
/// the same items in other types, pair them alike (QA `E32-4`).
pub(in super::super) fn alike(old: &Value, new: &Value) -> bool {
    fn text(value: &Value) -> Option<String> {
        match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(flag) => Some(flag.to_string()),
            Value::Null => Some(String::new()),
            _ => None,
        }
    }
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => {
            old.len() == new.len() && new.iter().all(|(key, value)| old.get(key).is_some_and(|held| alike(held, value)))
        }
        (Value::Array(old), Value::Array(new)) => old.len() == new.len() && old.iter().zip(new).all(|(old, new)| alike(old, new)),
        _ => old == new || text(old).is_some_and(|old| Some(old) == text(new)),
    }
}

/// How many of their keys two items hold alike.
fn shared(old: &Value, new: &Value) -> usize {
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => {
            new.iter().filter(|(key, value)| old.get(*key).is_some_and(|held| alike(held, value))).count()
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{Told, places, told_places};
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

    #[test]
    fn items_the_edit_tells_of_keep_their_places_and_values_place_the_rest() {
        let old = items(&[1, 1, 3]);
        // The first of two equal items removed: the edit says which.
        let new = items(&[1, 3]);
        assert_eq!(places(&old, &new), [Some(0), Some(2)]);
        assert_eq!(told_places(&old, &new, &[Told::At(1), Told::Nothing]), [Some(1), Some(2)]);
        // Pasted back from elsewhere: no item of this list.
        assert_eq!(told_places(&old, &items(&[1, 1, 3]), &[Told::Elsewhere, Told::Nothing, Told::Nothing]), [None, Some(0), Some(2)]);
    }
}
