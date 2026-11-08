//! Lists whose items moved while others were added or removed, saved in
//! stages: the kept items moved first (each with its comment lines, as
//! [`super::patch_reorder`] moves them), then the removed ones dropped,
//! then the new ones added. Each stage is one kind of change the patches
//! make line by line, so a comment stays with its item through all three.

use serde_json::{Map, Value};

/// The values between `old` and `new` when some list of `old` holds items
/// `new` keeps in another order and `new` also adds or removes items there:
/// `old` with those items moved, then without the removed ones (`new` is
/// the last stage). Empty when no list both moves and changes length.
pub(in super::super) fn stages(old: &Value, new: &Value) -> Vec<Value> {
    let mut resized = false;
    let (moved, trimmed) = staged(old, new, &mut resized);
    if !resized {
        return Vec::new();
    }
    let mut out = vec![moved];
    if trimmed != out[0] && trimmed != *new {
        out.push(trimmed);
    }
    out
}

/// Which item of `old` each item of `new` is (the first unused equal one).
fn matching(old: &[Value], new: &[Value]) -> Vec<Option<usize>> {
    let mut used = vec![false; old.len()];
    new.iter()
        .map(|item| {
            let at = (0..old.len()).find(|at| !used[*at] && old[*at] == *item)?;
            used[at] = true;
            Some(at)
        })
        .collect()
}

/// `old` with the kept items of each list `new` both reorders and resizes
/// in `new`'s order, in the places kept items held (removed items stay
/// where they are); and that without the removed items. `resized` is set
/// when such a list is found. An item `new` changes in place stays itself.
fn staged(old: &Value, new: &Value, resized: &mut bool) -> (Value, Value) {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            let (mut moved, mut trimmed) = (Map::new(), Map::new());
            for (key, value) in left {
                let (one, two) = match right.get(key) {
                    Some(other) => staged(value, other, resized),
                    None => (value.clone(), value.clone()),
                };
                moved.insert(key.clone(), one);
                trimmed.insert(key.clone(), two);
            }
            (Value::Object(moved), Value::Object(trimmed))
        }
        (Value::Array(left), Value::Array(right)) => {
            let kept: Vec<usize> = matching(left, right).into_iter().flatten().collect();
            let moves = kept.windows(2).any(|pair| pair[0] > pair[1]);
            if moves && kept.len() < left.len().max(right.len()) {
                *resized = true;
                let mut slots = kept.clone();
                slots.sort_unstable();
                let mut items = left.clone();
                for (slot, from) in slots.into_iter().zip(&kept) {
                    items[slot] = left[*from].clone();
                }
                let trimmed = kept.iter().map(|at| left[*at].clone()).collect();
                return (Value::Array(items), Value::Array(trimmed));
            }
            if left.len() == right.len() {
                let (moved, trimmed) = left
                    .iter()
                    .zip(right)
                    .map(|(item, other)| staged(item, other, resized))
                    .unzip();
                return (Value::Array(moved), Value::Array(trimmed));
            }
            (old.clone(), old.clone())
        }
        _ => (old.clone(), old.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::stages;
    use serde_json::json;

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
        // Only moved, only resized, or an item edited in place: one stage.
        assert!(stages(&old, &json!({"l": ["c", "b", "a"]})).is_empty());
        assert!(stages(&old, &json!({"l": ["a", "b", "c", "d"]})).is_empty());
        assert!(stages(&old, &json!({"l": ["a", "x", "c"]})).is_empty());
    }
}
