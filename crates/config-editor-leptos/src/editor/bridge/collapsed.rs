//! The one difference a save may read back with: a list the host already
//! held, left with one item, in a language that writes a list as its
//! items repeated (XML elements, HCL blocks, TOML `[[tables]]`). Its one
//! item reads as the item, because the language has no other spelling:
//! the host's own edit writes the same text, and its reader (a schema,
//! Terraform's block types) knows the list. Anything else the host reads
//! back otherwise waits: a list typed over a string, a nested list in
//! XML, and a list (or map of HCL block labels) emptied, which such a
//! language writes as no key at all.

use serde_json::Value;

/// Whether `read`, what the host reads back from the save, differs from
/// `value`, what the view holds, at `pointer` only as a list of `old`, the
/// host's value before, left with one item, collapses there.
pub(super) fn host_spelling(old: &Value, value: &Value, read: &Value, pointer: &str) -> bool {
    match (old.pointer(pointer), value.pointer(pointer)) {
        (Some(Value::Array(_)), Some(Value::Array(items))) => {
            items.len() == 1 && read.pointer(pointer) == items.first()
        }
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
        // Left with one item, read as the item.
        assert!(host_spelling(
            &old,
            &json!({"s": ["a"]}),
            &json!({"s": "a"}),
            "/s"
        ));
        // Read back as anything else.
        assert!(!host_spelling(
            &old,
            &json!({"s": ["a"]}),
            &json!({"s": "b"}),
            "/s"
        ));
        // Emptied, read as no key: no spelling of the view's empty list.
        assert!(!host_spelling(&old, &json!({"s": []}), &json!({}), "/s"));
        assert!(!host_spelling(&old, &json!({"m": {}}), &json!({}), "/m"));
        // A list where the host held a string, or no value.
        assert!(!host_spelling(
            &old,
            &json!({"t": ["a"]}),
            &json!({"t": "a"}),
            "/t"
        ));
        assert!(!host_spelling(
            &old,
            &json!({"n": ["a"]}),
            &json!({"n": "a"}),
            "/n"
        ));
    }

    /// P2: a list emptied in a view of a host that writes a list as its
    /// items repeated (XML elements, HCL blocks, TOML `[[tables]]`), at
    /// the root or inside an object, waits to save: written as no items,
    /// the key would be gone where the view holds `[]`.
    #[cfg(all(
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    ))]
    #[test]
    fn a_list_emptied_in_a_view_of_a_host_writing_repeated_items_waits() {
        use super::super::Bridge;
        use crate::editor::Language;
        use crate::editor::edit_as::SaveError;

        let hosts = [
            (Language::XML, "<a>1</a>\n<c>1</c>\n<c>2</c>\n"),
            (Language::XML, "<x>\n  <c>false</c>\n</x>\n"),
            (
                Language::XML,
                "<x>\n  <a>1</a>\n  <c>1</c>\n  <c>2</c>\n</x>\n",
            ),
            (
                Language::HCL,
                "a = 1\n\nc {\n  b = 1\n}\n\nc {\n  b = 2\n}\n",
            ),
            (Language::TOML, "a = 1\n\n[[c]]\nb = 1\n\n[[c]]\nb = 2\n"),
        ];
        for (host, host_text) in hosts {
            for &edit in Language::ALL {
                if edit == host || edit == Language::XML {
                    continue;
                }
                let bridge = Bridge::new(host, edit, None).unwrap();
                let shown = bridge.to_edit(host_text).unwrap();
                let mut value = crate::editor::codec::read(edit.behavior(), &shown.text, None)
                    .unwrap_or_else(|error| panic!("{edit} reads {:?}: {error:?}", shown.text));
                let pointer = if value.get("x").is_some() {
                    "/x/c"
                } else {
                    "/c"
                };
                *value.pointer_mut(pointer).unwrap() = json!([]);
                let typed = crate::editor::codec::serialize(edit.behavior(), &value, None).unwrap();
                let saved = bridge.to_host(&typed, host_text);
                assert_eq!(
                    saved,
                    Err(SaveError::CannotKeep {
                        pointer: pointer.to_string()
                    }),
                    "{host} host:\n{host_text}{edit} view:\n{typed}"
                );
            }
        }
    }
}
