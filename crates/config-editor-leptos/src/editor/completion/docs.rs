//! Documentation keywords of a schema beyond `description`: deprecation,
//! a docs link, and the title and description of one allowed value.

use super::*;

/// Whether a property is deprecated (`deprecated: true` on it, through
/// `$ref` and the combinators).
pub(crate) fn is_deprecated(schema: &Value, root: &Value) -> bool {
    resolve_branches(schema, root)
        .into_iter()
        .any(|branch| branch.get("deprecated").and_then(Value::as_bool) == Some(true))
}

/// What to use instead of a deprecated property, from VS Code's
/// `deprecationMessage`.
pub(crate) fn deprecation_message(schema: &Value, root: &Value) -> Option<String> {
    resolve_branches(schema, root)
        .into_iter()
        .find_map(|branch| {
            branch
                .get("deprecationMessage")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

/// The property's `x-docs-url`, when it is a web address: anything else
/// (a `javascript:` URL) is never made a link.
pub(crate) fn docs_url(schema: &Value, root: &Value) -> Option<String> {
    resolve_branches(schema, root)
        .into_iter()
        .find_map(|branch| {
            branch
                .get("x-docs-url")
                .and_then(Value::as_str)
                .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                .map(str::to_string)
        })
}

/// The `x-docs-url` of the value at JSON `pointer` (its property's, or an
/// item's), so a problem there links to its docs as the hover does.
/// `repeats_arrays` as in [`Applies`].
#[cfg(feature = "schema-validation")]
pub(crate) fn docs_url_at(root: &Value, pointer: &str, repeats_arrays: bool) -> Option<String> {
    let path: Vec<SchemaPathSegment> = crate::editor::language::pointer_segments(pointer)
        .into_iter()
        .map(SchemaPathSegment::from_pointer_segment)
        .collect();
    Applies::blind(root, repeats_arrays)
        .at(&path)
        .schemas()
        .into_iter()
        .find_map(|schema| docs_url(schema, root))
}

/// The documentation of one allowed value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ValueDoc {
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
}

impl ValueDoc {
    /// One line for the suggestion list: `Title — description`, or either.
    pub(crate) fn detail(&self) -> Option<String> {
        match (&self.title, &self.description) {
            (Some(title), Some(description)) => Some(format!("{title} — {description}")),
            (Some(text), None) | (None, Some(text)) => Some(text.clone()),
            (None, None) => None,
        }
    }
}

/// What a resolved branch says about `value` alone: the entry of
/// `x-enum-descriptions` parallel to its `enum` (and of `x-enum-titles`),
/// or the branch's own `title` and `description` when it is `const` (a
/// `oneOf` of documented constants).
pub(crate) fn value_doc(branch: &Value, value: &Value) -> Option<ValueDoc> {
    ValueDocs::of(branch).of_value(value)
}

/// The docs of a branch's values, its `enum` indexed once: a list of
/// thousands of values looks each one's docs up without a pass over the
/// others.
pub(crate) struct ValueDocs<'a> {
    branch: &'a Value,
    /// The place of each `enum` value (the first, for one written twice),
    /// by its label.
    places: std::collections::HashMap<String, usize>,
}

impl<'a> ValueDocs<'a> {
    pub(crate) fn of(branch: &'a Value) -> Self {
        let mut places = std::collections::HashMap::new();
        let values = branch.get("enum").and_then(Value::as_array);
        for (index, value) in values.into_iter().flatten().enumerate() {
            places.entry(value_label(value)).or_insert(index);
        }
        Self { branch, places }
    }

    /// What the branch says about `value` (see [`value_doc`]).
    pub(crate) fn of_value(&self, value: &Value) -> Option<ValueDoc> {
        let branch = self.branch;
        let label = value_label(value);
        if let Some(&index) = self.places.get(&label) {
            let text_at = |keyword: &str| {
                branch
                    .get(keyword)
                    .and_then(Value::as_array)
                    .and_then(|texts| texts.get(index))
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(str::to_string)
            };
            let doc = ValueDoc {
                title: text_at("x-enum-titles"),
                description: text_at("x-enum-descriptions"),
            };
            return (doc != ValueDoc::default()).then_some(doc);
        }
        if branch
            .get("const")
            .is_some_and(|constant| value_label(constant) == label)
        {
            let text = |keyword: &str| {
                branch
                    .get(keyword)
                    .and_then(Value::as_str)
                    .map(str::to_string)
            };
            let doc = ValueDoc {
                title: text("title"),
                description: text("description"),
            };
            return (doc != ValueDoc::default()).then_some(doc);
        }
        None
    }
}

/// Whether any schema in `root` is marked deprecated, so documents are
/// only searched for deprecated keys when there can be some.
pub(crate) fn mentions_deprecated(root: &Value) -> bool {
    match root {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (key == "deprecated" && value == &Value::Bool(true)) || mentions_deprecated(value)
        }),
        Value::Array(items) => items.iter().any(mentions_deprecated),
        _ => false,
    }
}

/// A key of the document its schema marks deprecated.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DeprecatedKey {
    /// JSON pointer of the key's value.
    pub(crate) pointer: String,
    pub(crate) key: String,
    /// What to use instead (`deprecationMessage`).
    pub(crate) message: Option<String>,
    /// Its schema's docs link (see [`docs_url`]).
    pub(crate) docs_url: Option<String>,
}

/// Up to `limit` keys of `document` that the schema marks deprecated, in
/// document order.
pub(crate) fn deprecated_keys(document: &Value, root: &Value, limit: usize) -> Vec<DeprecatedKey> {
    let mut found = Vec::new();
    let walking = Walking {
        decided: true,
        repeats_arrays: false,
    };
    each_value(document, root, walking, &mut |visit| {
        if found.len() >= limit {
            return Next::Stop;
        }
        let Some(SchemaPathSegment::Key(key)) = visit.segment else {
            return Next::Into;
        };
        let branches = visit.here.branches();
        if branches
            .iter()
            .any(|branch| branch.get("deprecated").and_then(Value::as_bool) == Some(true))
        {
            found.push(DeprecatedKey {
                pointer: visit.pointer.to_string(),
                key: key.clone(),
                docs_url: visit
                    .here
                    .schemas()
                    .into_iter()
                    .find_map(|schema| docs_url(schema, root)),
                message: branches.iter().find_map(|branch| {
                    branch
                        .get("deprecationMessage")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }),
            });
        }
        Next::Into
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn documentation_keywords_are_read_through_refs_and_branches() {
        let root = json!({
            "$defs": {"old": {"deprecated": true, "deprecationMessage": "Use b."}},
            "properties": {
                "a": {"$ref": "#/$defs/old", "x-docs-url": "https://example.com/a"},
                "b": {"x-docs-url": "javascript:alert(1)"}
            }
        });
        let a = &root["properties"]["a"];
        let b = &root["properties"]["b"];
        assert!(is_deprecated(a, &root));
        assert!(!is_deprecated(b, &root));
        assert_eq!(deprecation_message(a, &root).as_deref(), Some("Use b."));
        assert_eq!(docs_url(a, &root).as_deref(), Some("https://example.com/a"));
        assert_eq!(docs_url(b, &root), None, "only web addresses become links");
    }

    #[test]
    fn deprecated_keys_are_found_at_any_depth_and_only_when_present() {
        let root = json!({
            "properties": {
                "timeout": {"type": "integer", "deprecated": true, "deprecationMessage": "Use deadline."},
                "servers": {"type": "array", "items": {"properties": {"weight": {"deprecated": true}}}}
            }
        });
        assert!(mentions_deprecated(&root));
        assert!(!mentions_deprecated(
            &json!({"properties": {"deprecated": {"type": "boolean"}}})
        ));
        let document = json!({"timeout": 3, "servers": [{"weight": 1}, {"host": "a"}]});
        assert_eq!(
            deprecated_keys(&document, &root, 20),
            vec![
                DeprecatedKey {
                    pointer: "/timeout".into(),
                    key: "timeout".into(),
                    message: Some("Use deadline.".into()),
                    docs_url: None,
                },
                DeprecatedKey {
                    pointer: "/servers/0/weight".into(),
                    key: "weight".into(),
                    message: None,
                    docs_url: None,
                },
            ]
        );
        assert_eq!(deprecated_keys(&document, &root, 1).len(), 1);
    }

    #[test]
    fn values_take_their_enum_entry_or_their_const_branch() {
        let enumerated = json!({
            "enum": ["fast", "safe", 3],
            "x-enum-descriptions": ["Skips checks.", "", "Three."]
        });
        assert_eq!(
            value_doc(&enumerated, &json!("fast")).and_then(|doc| doc.detail()),
            Some("Skips checks.".to_string())
        );
        assert_eq!(value_doc(&enumerated, &json!("safe")), None);
        assert_eq!(
            value_doc(&enumerated, &json!(3)).and_then(|doc| doc.detail()),
            Some("Three.".to_string())
        );
        let constant = json!({"const": "trace", "title": "Trace", "description": "Everything."});
        assert_eq!(
            value_doc(&constant, &json!("trace")).and_then(|doc| doc.detail()),
            Some("Trace — Everything.".to_string())
        );
        assert_eq!(value_doc(&constant, &json!("info")), None);
    }
}
