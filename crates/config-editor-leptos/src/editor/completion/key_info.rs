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
    /// Whether the object requires it: a schema certain to apply to it
    /// does, not one of its options (a key one option requires is optional
    /// when another option is taken, "at least one of").
    pub(crate) required: bool,
}

/// What applies to an object, read once for what is said of its keys.
pub(crate) struct ObjectAt<'a> {
    /// Every schema applying to it.
    pub(crate) branches: Vec<&'a Value>,
    /// Those certain to apply (see [`Here::certain`]).
    pub(crate) certain: Vec<&'a Value>,
    /// Its `oneOf`/`anyOf` options.
    pub(crate) choices: Choices<'a>,
    pub(crate) root: &'a Value,
}

impl<'a> ObjectAt<'a> {
    pub(crate) fn of(here: &Here<'a>) -> Self {
        Self {
            branches: here.branches(),
            certain: here.certain(),
            choices: here.choices(),
            root: here.root(),
        }
    }
}

/// What is said of `key` of the object `object`; `None` when no schema
/// of it names the key.
pub(crate) fn key_info<'a>(key: &str, object: &ObjectAt<'a>) -> Option<KeyInfo<'a>> {
    let (choices, root) = (&object.choices, object.root);
    let declared: Vec<&'a Value> = object
        .branches
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
    let required = object
        .certain
        .iter()
        .filter(|branch| choices.of_branch(branch).is_none())
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
