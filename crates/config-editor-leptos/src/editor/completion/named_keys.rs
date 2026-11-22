//! The keys an object's `propertyNames` offers beyond those it declares:
//! the names its `x-config-editor-keys` gives, then its `enum`/`examples`,
//! in the order and with the sources a value of that schema is offered
//! them.

use super::candidates::ValueNames;
use super::*;
use crate::editor::key_names::{KeyOrigin, key_branches};

/// A key to offer for an object, with what its suggestion shows: a key
/// the object declares, or one its `propertyNames` names.
pub(super) struct KeyEntry<'s> {
    pub(super) key: String,
    /// The schema of its value.
    pub(super) property: &'s Value,
    /// Added to its match score (a required key, the matched option's).
    pub(super) bonus: i64,
    pub(super) type_label: String,
    pub(super) detail: String,
    /// Where a name comes from (`x-config-editor-keys`).
    pub(super) origin: Option<KeyOrigin>,
    /// Named by `propertyNames`: offered after the declared keys, in the
    /// order found, not sorted by name.
    pub(super) named: bool,
}

/// The schema of a value no schema describes.
static ANY: Value = Value::Bool(true);

/// The keys `here` (what applies to an object) offers through its
/// `propertyNames` that it does not declare: the names first, then the
/// `enum`/`examples`, each once; none a `propertyNames` certain to apply
/// refuses.
pub(super) fn named_keys<'s>(
    here: &Here<'s>,
    object: &ObjectAt<'s>,
    names: &ValueNames<'_>,
) -> Vec<KeyEntry<'s>> {
    let root = here.root();
    let branches = key_branches(&object.branches, root);
    if branches.is_empty() {
        return Vec::new();
    }
    let declared = |key: &str| {
        object.branches.iter().any(|branch| {
            branch
                .get("properties")
                .and_then(|named| named.get(key))
                .is_some()
        })
    };
    let certain: Vec<&Value> = object
        .certain
        .iter()
        .filter_map(|branch| branch.get("propertyNames"))
        .collect();
    let refused = |key: &str| {
        let name = Value::String(key.to_string());
        certain
            .iter()
            .any(|schema| decides(schema, &name, root) == Some(false))
    };
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let mut add = |key: String, detail: String, origin: Option<KeyOrigin>| {
        if declared(&key) || refused(&key) || !seen.insert(key.clone()) {
            return;
        }
        let schemas = here.child(&SchemaPathSegment::Key(key.clone())).schemas();
        let property = schemas
            .iter()
            .copied()
            .find(|schema| !names_only(schema))
            .or(schemas.first().copied())
            .unwrap_or(&ANY);
        entries.push(KeyEntry {
            type_label: schema_type_label(property, root, "property"),
            key,
            property,
            bonus: 0,
            detail,
            origin,
            named: true,
        });
    };
    for offered in names.lookup.offered(&branches, || (names.document)()) {
        add(
            offered.name,
            offered.detail.unwrap_or_default(),
            Some(offered.origin),
        );
    }
    for branch in &branches {
        let description = branch
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let docs = ValueDocs::of(branch);
        for value in schema_value_candidates(branch, true) {
            let Value::String(key) = &value else {
                continue;
            };
            let detail = docs
                .of_value(&value)
                .and_then(|doc| doc.detail())
                .unwrap_or_else(|| description.to_string());
            add(key.clone(), detail, None);
        }
    }
    entries
}
