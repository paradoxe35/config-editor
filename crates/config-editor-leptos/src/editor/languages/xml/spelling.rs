//! An attribute is read `@a` beside a child element `a`, and `a` alone: a
//! text written from a value holding `@a` reads back `a` once the child is
//! gone, and a view that showed `@a` keeps spelling it so after a save
//! removed the child. Read back, the keys are spelled as the value spells
//! them, so a save compares the attribute with the attribute (and a child
//! `a` added back beside `@a` is the child).

use super::convert::{AttributePaths, attribute_paths};
use serde_json::Value;

/// `read` (read from `source`) with each attribute read `a` spelled `@a`
/// where `like` spells it so.
pub(super) fn spelled_like(source: &str, read: Value, like: &Value) -> Value {
    if !spells_attributes(like) {
        return read;
    }
    respelled(&attribute_paths(source), &mut Vec::new(), read, like)
}

/// Whether `value` holds an `@` key anywhere.
fn spells_attributes(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, child)| key.starts_with('@') || spells_attributes(child)),
        Value::Array(items) => items.iter().any(spells_attributes),
        _ => false,
    }
}

fn respelled(
    attributes: &AttributePaths,
    path: &mut Vec<String>,
    read: Value,
    like: &Value,
) -> Value {
    match (read, like) {
        (Value::Object(map), Value::Object(like)) => {
            let marked = |path: &mut Vec<String>, key: &str, child: &Value| {
                let spelled = format!("@{key}");
                if key.starts_with('@')
                    || child.is_object()
                    || child.is_array()
                    || !like.contains_key(&spelled)
                    || map.contains_key(&spelled)
                {
                    return None;
                }
                path.push(key.to_string());
                let attribute = attributes.contains(path.as_slice());
                path.pop();
                attribute.then_some(spelled)
            };
            let renamed: Vec<(String, Value)> = map
                .iter()
                .map(|(key, child)| {
                    let key = marked(path, key, child).unwrap_or_else(|| key.clone());
                    (key, child.clone())
                })
                .collect();
            Value::Object(
                renamed
                    .into_iter()
                    .map(|(key, child)| {
                        let child = match like.get(&key) {
                            Some(like) => {
                                path.push(key.clone());
                                let child = respelled(attributes, path, child, like);
                                path.pop();
                                child
                            }
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
                    Some(like) => respelled(attributes, path, item, like),
                    None => item,
                })
                .collect(),
        ),
        (read, _) => read,
    }
}

#[cfg(test)]
mod tests {
    use super::spelled_like;
    use serde_json::json;

    #[test]
    fn only_an_attribute_is_spelled_as_one() {
        let like = json!({"r": {"@a": 1}});
        // The attribute `a` reads `a` alone: spelled `@a` as `like` does.
        assert_eq!(spelled_like("<r a=\"1\"/>", json!({"r": {"a": 1}}), &like), like);
        // A child element `a` stays `a`: a save that wrote the attribute
        // as an element reads back otherwise.
        let read = json!({"r": {"a": 1}});
        assert_eq!(spelled_like("<r><a>1</a></r>", read.clone(), &like), read);
    }
}
