//! A save holding several edits (a value changed, an item added elsewhere)
//! split into steps the line-preserving patches can each make: every value
//! change first, then one structural change at a time.

use serde_json::{Map, Value};

/// The values between `old` and `new`, ending with `new`: `old` with its
/// changed scalars set, then with each container whose keys or length
/// changed made like `new`'s, one at a time. Empty when there is one step.
pub(in super::super) fn steps(old: &Value, new: &Value) -> Vec<Value> {
    let mut steps = Vec::new();
    let scalars = with_scalar_changes(old, new);
    if scalars != *old && scalars != *new {
        steps.push(scalars);
    }
    let mut current = steps.last().cloned().unwrap_or_else(|| old.clone());
    while let Some(next) = one_structural_step(&current, new) {
        steps.push(next.clone());
        current = next;
        if steps.len() > 64 {
            return Vec::new();
        }
    }
    if steps.len() < 2 { Vec::new() } else { steps }
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
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            Value::Array(left.iter().zip(right).map(|(a, b)| with_scalar_changes(a, b)).collect())
        }
        (left, right) if !left.is_object() && !left.is_array() && !right.is_object() && !right.is_array() => right.clone(),
        (left, _) => left.clone(),
    }
}

/// `current` with its first container that differs from `target` in keys or
/// length made like it (keys kept with their current values), or with a
/// value that changed kind replaced; `None` when they are equal.
fn one_structural_step(current: &Value, target: &Value) -> Option<Value> {
    if current == target {
        return None;
    }
    match (current, target) {
        (Value::Object(left), Value::Object(right)) => {
            let same_keys = left.len() == right.len() && left.keys().all(|key| right.contains_key(key));
            if !same_keys {
                let rebuilt: Map<String, Value> = right
                    .iter()
                    .map(|(key, value)| (key.clone(), left.get(key).unwrap_or(value).clone()))
                    .collect();
                return Some(Value::Object(rebuilt));
            }
            for (key, value) in left {
                if let Some(next) = one_structural_step(value, &right[key]) {
                    let mut map = left.clone();
                    map.insert(key.clone(), next);
                    return Some(Value::Object(map));
                }
            }
            None
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, value) in left.iter().enumerate() {
                if let Some(next) = one_structural_step(value, &right[index]) {
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

#[cfg(test)]
mod tests {
    use super::steps;
    use serde_json::json;

    #[test]
    fn a_save_of_several_edits_splits_into_values_then_one_structure_at_a_time() {
        let old = json!({"listen": 1, "udp": {"max_peers": 2}, "self": {"allow": ["a"]}, "pool": {"min": 1}});
        let new = json!({"listen": 2, "udp": {"max_peers": 2}, "self": {"allow": ["a", "b"]}, "pool": {"min": 1, "max": 3}});
        let steps = steps(&old, &new);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0]["listen"], 2);
        assert_eq!(steps[0]["self"]["allow"], json!(["a"]));
        assert_eq!(steps[1]["self"]["allow"], json!(["a", "b"]));
        assert_eq!(steps[1]["pool"], json!({"min": 1}));
        assert_eq!(steps[2], new);
        // One edit: no steps.
        assert!(super::steps(&old, &json!({"listen": 1, "udp": {"max_peers": 3}, "self": {"allow": ["a"]}, "pool": {"min": 1}})).is_empty());
    }
}
