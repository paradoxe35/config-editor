//! A key with a prefix (`x:child`) is an attribute: the model keys elements
//! by their local name. So an element renamed to one cannot be written as
//! the element it was: the save waits, rather than turn the element into an
//! attribute.

use super::convert::AttributePaths;
use serde_json::Value;

/// Why `value` cannot be written over `old` (the host's value, whose
/// attributes are `attributes`): the first element of `old` it holds
/// renamed to a key with a prefix, in its place with its value. `None`
/// when there is none.
pub(super) fn renamed_element(value: &Value, old: &Value, attributes: &AttributePaths) -> Option<String> {
    walk(value, old, attributes, &mut Vec::new(), String::new())
}

fn walk(value: &Value, old: &Value, attributes: &AttributePaths, path: &mut Vec<String>, pointer: String) -> Option<String> {
    match (value, old) {
        (Value::Object(map), Value::Object(was)) => {
            if map.len() == was.len() {
                for ((key, child), (old_key, old_child)) in map.iter().zip(was) {
                    let renamed = key != old_key && !was.contains_key(key) && !map.contains_key(old_key);
                    let element = {
                        path.push(old_key.clone());
                        let attribute = attributes.contains(path.as_slice());
                        path.pop();
                        !attribute && !old_key.starts_with(['@', '#'])
                    };
                    if renamed && element && key.contains(':') && child == old_child {
                        let at = format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                        return Some(format!(
                            "XML keeps a key with a prefix ({key}) only as an attribute, so the element {old_key} renamed so would become one: rename it without the prefix, or edit it as XML (at {at})"
                        ));
                    }
                }
            }
            map.iter().find_map(|(key, child)| {
                let was = was.get(key)?;
                path.push(key.clone());
                let found = walk(child, was, attributes, path, format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")));
                path.pop();
                found
            })
        }
        (Value::Array(items), Value::Array(was)) => items.iter().zip(was).enumerate().find_map(|(index, (item, was))| {
            walk(item, was, attributes, path, format!("{pointer}/{index}"))
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::convert::attribute_paths;
    use super::renamed_element;
    use serde_json::json;

    #[test]
    fn an_element_renamed_with_a_prefix_is_named_and_an_attribute_is_not() {
        let host = "<r xmlns:x=\"urn:a\" k=\"2\"><child>1</child></r>";
        let old = json!({"r": {"k": 2, "child": 1}});
        let attributes = attribute_paths(host);
        let renamed = json!({"r": {"k": 2, "x:child": 1}});
        assert!(renamed_element(&renamed, &old, &attributes).is_some_and(|reason| reason.ends_with("(at /r/x:child)")));
        let attribute = json!({"r": {"x:k": 2, "child": 1}});
        assert_eq!(renamed_element(&attribute, &old, &attributes), None);
        let added = json!({"r": {"k": 2, "child": 1, "x:n": 3}});
        assert_eq!(renamed_element(&added, &old, &attributes), None);
    }
}
