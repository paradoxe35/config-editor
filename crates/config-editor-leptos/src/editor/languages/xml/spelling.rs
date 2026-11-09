//! An attribute is read `@a` beside a child element `a`, and `a` alone: a
//! text written from a value holding `@a` reads back `a` once the child is
//! gone. Read back, the keys are spelled as the value spells them, so a
//! save compares the attribute with the attribute.

use serde_json::Value;

/// `read` with each attribute read `a` spelled `@a` where `like` (the
/// value it was written from) spells it so.
pub(super) fn spelled_like(read: Value, like: &Value) -> Value {
    match (read, like) {
        (Value::Object(map), Value::Object(like)) => {
            let marked = |key: &str, child: &Value| {
                let spelled = format!("@{key}");
                (!key.starts_with('@')
                    && !child.is_object()
                    && !child.is_array()
                    && !like.contains_key(key)
                    && like.contains_key(&spelled)
                    && !map.contains_key(&spelled))
                .then_some(spelled)
            };
            let renamed: Vec<(String, Value)> = map
                .iter()
                .map(|(key, child)| (marked(key, child).unwrap_or_else(|| key.clone()), child.clone()))
                .collect();
            Value::Object(
                renamed
                    .into_iter()
                    .map(|(key, child)| {
                        let child = match like.get(&key) {
                            Some(like) => spelled_like(child, like),
                            None => child,
                        };
                        (key, child)
                    })
                    .collect(),
            )
        }
        (Value::Array(items), Value::Array(like)) => Value::Array(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| match like.get(index) {
                    Some(like) => spelled_like(item, like),
                    None => item,
                })
                .collect(),
        ),
        (read, _) => read,
    }
}
