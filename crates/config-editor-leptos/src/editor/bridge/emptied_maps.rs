//! A map whose last entry a view deleted: a YAML view left with `deps:`
//! and nothing under it reads a null, which a TOML or XML host cannot
//! hold (the save waited) and a JSON or HCL host would hold as another
//! value than the map it was. Where the host held a map and the view
//! writes no value there, the save keeps an empty map; a null the user
//! wrote (`deps: null`, `deps: ~`) stays one.

use super::super::language::ConfigLanguage;
use serde_json::{Map, Value};

/// `value` (the view's edit) with each null the view writes no value for,
/// where `old` (the host's value) holds a map, an empty map.
pub(super) fn kept(
    edit: &dyn ConfigLanguage,
    edit_text: &str,
    old: &Value,
    mut value: Value,
) -> Value {
    let mut emptied = Vec::new();
    find(&value, old, &mut String::new(), &mut emptied);
    for pointer in emptied {
        if !writes_value(edit, edit_text, &pointer)
            && let Some(slot) = value.pointer_mut(&pointer)
        {
            *slot = Value::Object(Map::new());
        }
    }
    value
}

/// The pointers of the nulls of `value` where `old` holds a map: only
/// where both hold a container is the walk taken further.
fn find(value: &Value, old: &Value, pointer: &mut String, found: &mut Vec<String>) {
    match (value, old) {
        (Value::Null, Value::Object(_)) => found.push(pointer.clone()),
        (Value::Object(map), Value::Object(was)) => {
            for (key, child) in map {
                if let Some(was) = was.get(key) {
                    let length = pointer.len();
                    pointer.push_str(&format!("/{}", key.replace('~', "~0").replace('/', "~1")));
                    find(child, was, pointer, found);
                    pointer.truncate(length);
                }
            }
        }
        (Value::Array(items), Value::Array(was)) => {
            for (index, (item, was)) in items.iter().zip(was).enumerate() {
                let length = pointer.len();
                pointer.push_str(&format!("/{index}"));
                find(item, was, pointer, found);
                pointer.truncate(length);
            }
        }
        _ => {}
    }
}

/// Whether the view's text writes a value at `pointer`: its value's span
/// holds anything but a comment.
fn writes_value(edit: &dyn ConfigLanguage, edit_text: &str, pointer: &str) -> bool {
    let Some(span) = edit.value_span(edit_text, pointer) else {
        return false;
    };
    let line =
        super::super::paths::text_line(edit_text, span.line.saturating_sub(1)).unwrap_or_default();
    let written: String = line
        .chars()
        .skip(span.start_column.saturating_sub(1))
        .take(span.end_column.saturating_sub(span.start_column))
        .collect();
    let written = written.trim();
    !written.is_empty() && edit.comment_start(written) != Some(0)
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::kept;
    use crate::editor::Language;
    use serde_json::json;

    #[test]
    fn a_map_left_with_no_value_is_empty_and_a_written_null_stays() {
        let yaml = Language::YAML.behavior();
        let old = json!({"a": {"deps": {"x": 1}, "n": {"y": 1}, "s": 1}});
        let text = "a:\n  deps:\n  n: null\n  s:\n";
        let value = json!({"a": {"deps": null, "n": null, "s": null}});
        assert_eq!(
            kept(yaml, text, &old, value),
            json!({"a": {"deps": {}, "n": null, "s": null}})
        );
        let text = "a:\n  deps: # gone\n  n: ~\n  s:\n";
        let value = json!({"a": {"deps": null, "n": null, "s": null}});
        assert_eq!(
            kept(yaml, text, &old, value),
            json!({"a": {"deps": {}, "n": null, "s": null}})
        );
    }

    /// A save looks up the view's text only at the nulls standing for a
    /// map: the same work beside few maps or many.
    #[test]
    fn the_view_is_read_only_where_a_map_was_emptied_at_any_size() {
        use crate::editor::work_count::counted_all;
        let work = |count: usize| {
            let mut old = serde_json::Map::new();
            let mut value = serde_json::Map::new();
            let mut text = String::new();
            for at in 0..count {
                old.insert(format!("m{at}"), json!({"x": at}));
                value.insert(format!("m{at}"), json!({"x": at}));
                text.push_str(&format!("m{at}:\n  x: {at}\n"));
            }
            value.insert("m0".into(), json!(null));
            let text = text.replacen("m0:\n  x: 0\n", "m0:\n", 1);
            let (kept, work) =
                counted_all(|| kept(Language::YAML.behavior(), &text, &old.into(), value.into()));
            assert_eq!(kept.pointer("/m0"), Some(&json!({})));
            work
        };
        assert_eq!(work(10), work(400));
    }
}
