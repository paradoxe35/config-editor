//! What a suggestion and a hover card say of a key of an object (its
//! schema, its type, whether the object requires it): one reading for
//! both, so the two agree on every key.

use super::*;

/// What is said of a key (see [`key_info`]).
pub(crate) struct KeyInfo<'a> {
    /// The key's schema where a branch describes it, not the one of a
    /// branch only naming it (`"version": true`, as an `anyOf` saying "at
    /// least one of" does).
    pub(crate) schema: &'a Value,
    /// Its type: the one the `oneOf`/`anyOf` options offering it give it
    /// where they describe it, else its schema's.
    pub(crate) type_label: String,
    /// Whether the object requires it: one of its own branches does, not
    /// only one of its options (a key one option requires is optional when
    /// another option is taken).
    pub(crate) required: bool,
}

/// What is said of `key` of the object whose schema branches (for its
/// value, see `branches_for`) are `branches`, `choices` its options;
/// `None` when no branch names it.
pub(crate) fn key_info<'a>(
    key: &str,
    branches: &[&'a Value],
    choices: &ChoicesAt<'a>,
    root: &'a Value,
) -> Option<KeyInfo<'a>> {
    let declared: Vec<&'a Value> = branches
        .iter()
        .filter_map(|branch| branch.get("properties")?.get(key))
        .collect();
    let schema = declared
        .iter()
        .find(|schema| !names_only(schema))
        .or(declared.first())
        .copied()?;
    let typed_by: Vec<&Value> = choices
        .holding(key)
        .iter()
        .map(|(_, schema)| *schema)
        .filter(|schema| !names_only(schema))
        .collect();
    let type_label = if typed_by.is_empty() {
        schema_type_label(schema, root, "property")
    } else {
        type_label_of_all(&typed_by, root, "property")
    };
    let required = branches
        .iter()
        .filter(|branch| choices.of(branch).is_none())
        .any(|branch| {
            branch
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|names| names.iter().any(|name| name.as_str() == Some(key)))
        });
    Some(KeyInfo {
        schema,
        type_label,
        required,
    })
}

/// Whether a property's schema only names the key (`true`, `{}`),
/// describing nothing of its value.
pub(crate) fn names_only(property: &Value) -> bool {
    match property {
        Value::Bool(true) => true,
        Value::Object(members) => members.is_empty(),
        _ => false,
    }
}
