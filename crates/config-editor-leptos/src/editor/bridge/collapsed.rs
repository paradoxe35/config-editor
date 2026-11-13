//! The one difference a save may read back with: a list the host already
//! held, left with one item or none, in a language that writes a list as
//! its items repeated (XML elements, HCL blocks, TOML `[[tables]]`). Its
//! one item reads as the item, and no items as no key, because the
//! language has no other spelling: the host's own edit writes the same
//! text, and its reader (a schema, Terraform's block types) knows the
//! list. A map of HCL block labels emptied goes with its key the same way.
//! Anything else the host reads back otherwise (a list typed over a
//! string, a nested list in XML) waits.

use serde_json::Value;

/// Whether `read`, what the host reads back from the save, differs from
/// `value`, what the view holds, at `pointer` only as a list (or label
/// map) of `old`, the host's value before, collapses there.
pub(super) fn host_spelling(old: &Value, value: &Value, read: &Value, pointer: &str) -> bool {
    let (Some(before), Some(now)) = (old.pointer(pointer), value.pointer(pointer)) else {
        return false;
    };
    let read = read.pointer(pointer);
    match (before, now) {
        (Value::Array(_) | Value::Object(_), _) if holds_nothing(now) => read.is_none(),
        (Value::Array(_), Value::Array(items)) => items.len() == 1 && read == items.first(),
        _ => false,
    }
}

/// Whether `value` is an empty list or map, or a map of only those (HCL
/// labels whose blocks are all gone).
fn holds_nothing(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.values().all(holds_nothing),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::host_spelling;
    use serde_json::json;

    #[test]
    fn only_a_list_the_host_held_may_collapse() {
        let old = json!({"s": ["a", "b"], "t": "a", "m": {"x": {}}});
        // Left with one item, read as the item; emptied, read as no key.
        assert!(host_spelling(
            &old,
            &json!({"s": ["a"]}),
            &json!({"s": "a"}),
            "/s"
        ));
        assert!(host_spelling(&old, &json!({"s": []}), &json!({}), "/s"));
        assert!(host_spelling(&old, &json!({"m": {}}), &json!({}), "/m"));
        assert!(host_spelling(
            &old,
            &json!({"m": {"x": {}}}),
            &json!({}),
            "/m"
        ));
        // Read back as anything else.
        assert!(!host_spelling(
            &old,
            &json!({"s": ["a"]}),
            &json!({"s": "b"}),
            "/s"
        ));
        assert!(!host_spelling(
            &old,
            &json!({"s": []}),
            &json!({"s": ""}),
            "/s"
        ));
        assert!(!host_spelling(
            &old,
            &json!({"m": {}}),
            &json!({"m": ""}),
            "/m"
        ));
        assert!(!host_spelling(
            &old,
            &json!({"m": {"x": 1}}),
            &json!({}),
            "/m"
        ));
        // A list where the host held a string, or no value.
        assert!(!host_spelling(
            &old,
            &json!({"t": ["a"]}),
            &json!({"t": "a"}),
            "/t"
        ));
        assert!(!host_spelling(&old, &json!({"t": []}), &json!({}), "/t"));
        assert!(!host_spelling(
            &old,
            &json!({"n": ["a"]}),
            &json!({"n": "a"}),
            "/n"
        ));
    }
}
