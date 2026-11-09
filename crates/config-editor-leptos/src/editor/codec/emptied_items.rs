//! A list the host writes as items of lines of their own (HCL repeated
//! blocks, TOML `[[array]]` tables), or a map it writes as the labels of
//! blocks (HCL `svc "a" { … }`), emptied: written as no items, as XML
//! writes an emptied repeated element, not as an attribute (`tag = []`) or
//! an empty block (`svc {}`), which Terraform refuses for a block type.
//! The key goes with them.

use super::super::language::ConfigLanguage;
use serde_json::Value;
use std::borrow::Cow;

/// `value` without the empty lists `previous` writes as item lines, nor
/// the empty maps it writes as labels (nor the maps of labels left empty
/// by them).
pub(in super::super) fn without_emptied<'a>(
    language: &dyn ConfigLanguage,
    previous: Option<&str>,
    value: Cow<'a, Value>,
) -> Cow<'a, Value> {
    let Some(previous) = previous else {
        return value;
    };
    let mut value = value;
    loop {
        let mut found = Vec::new();
        emptied(language, previous, &value, String::new(), &mut found);
        if found.is_empty() {
            return value;
        }
        let owned = value.to_mut();
        for (parent, key) in found {
            if let Some(Value::Object(map)) = owned.pointer_mut(&parent) {
                map.shift_remove(&key);
            }
        }
    }
}

/// The keys (parent pointer, key) of the empty lists in `value` whose
/// items `previous` writes as lines of their own, and of the empty maps it
/// writes as labels.
fn emptied(
    language: &dyn ConfigLanguage,
    previous: &str,
    value: &Value,
    pointer: String,
    found: &mut Vec<(String, String)>,
) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_pointer =
                    format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                let lines = child.as_array().is_some_and(Vec::is_empty)
                    && language
                        .item_lines(previous, &format!("{child_pointer}/0"))
                        .is_some();
                let labels = child.as_object().is_some_and(serde_json::Map::is_empty)
                    && language.writes_as_labels(previous, &child_pointer);
                if lines || labels {
                    found.push((pointer.clone(), key.clone()));
                } else {
                    emptied(language, previous, child, child_pointer, found);
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                emptied(
                    language,
                    previous,
                    item,
                    format!("{pointer}/{index}"),
                    found,
                );
            }
        }
        _ => {}
    }
}
