//! Every field of a parsed document, by pointer: what a reading of the
//! spans of all of a text's fields asks for.

use serde_json::Value;

/// Every field of a document, by pointer (the root left out), and whether
/// it holds a scalar.
pub(crate) fn fields(document: &Value) -> Vec<(String, bool)> {
    fn walk(value: &Value, pointer: &mut String, found: &mut Vec<(String, bool)>) {
        let mut visit = |segment: &str, child: &Value, found: &mut Vec<(String, bool)>| {
            let length = pointer.len();
            pointer.push('/');
            pointer.push_str(&segment.replace('~', "~0").replace('/', "~1"));
            found.push((pointer.clone(), !(child.is_object() || child.is_array())));
            walk(child, pointer, found);
            pointer.truncate(length);
        };
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    visit(key, child, found);
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    visit(&index.to_string(), child, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(document, &mut String::new(), &mut found);
    found
}
