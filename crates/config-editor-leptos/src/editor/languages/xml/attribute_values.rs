//! An attribute holds one value: a list or a map for one (`@a`, or a key
//! the previous text writes as an attribute) is refused, not written as
//! `a="1 2"` or turned into child elements.

use super::convert::AttributePaths;
use serde_json::Value;

/// Why `value` cannot be written over `previous` (whose attributes are
/// `attributes`, read as `old`): the first attribute holding a list or a
/// map, with its pointer.
pub(super) fn first_container_attribute(
    value: &Value,
    old: Option<&Value>,
    attributes: &AttributePaths,
) -> Option<String> {
    let mut path = Vec::new();
    find(value, old, attributes, &mut path, &mut String::new())
}

fn find(
    value: &Value,
    old: Option<&Value>,
    attributes: &AttributePaths,
    path: &mut Vec<String>,
    pointer: &mut String,
) -> Option<String> {
    let length = pointer.len();
    let found = match value {
        Value::Object(map) => map.iter().find_map(|(key, child)| {
            let at = pointer.len();
            pointer.push('/');
            pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
            path.push(key.clone());
            let was = old.and_then(|old| old.get(key));
            let container = child.is_array() || child.is_object();
            // A plain key the previous text writes as an attribute, which
            // held a single value there (beside `@` its name, it is the
            // child element).
            let attribute = key.starts_with('@')
                || (attributes.contains(path.as_slice())
                    && !map.contains_key(&format!("@{key}"))
                    && was.is_some_and(|was| !was.is_array() && !was.is_object()));
            let found = if container && attribute {
                let name = key.strip_prefix('@').unwrap_or(key);
                let what = if child.is_array() { "a list" } else { "a map" };
                Some(format!(
                    "an XML attribute holds a single value, not {what}: `{name}` is saved once it holds one again (at {pointer})"
                ))
            } else {
                find(child, was, attributes, path, pointer)
            };
            path.pop();
            pointer.truncate(at);
            found
        }),
        Value::Array(items) => items.iter().enumerate().find_map(|(index, item)| {
            let at = pointer.len();
            pointer.push_str(&format!("/{index}"));
            let was = old.and_then(|old| match old {
                Value::Array(olds) => olds.get(index),
                // A lone element the list grew from.
                lone if index == 0 => Some(lone),
                _ => None,
            });
            let found = find(item, was, attributes, path, pointer);
            pointer.truncate(at);
            found
        }),
        _ => None,
    };
    pointer.truncate(length);
    found
}
