//! What an accepted key suggestion writes as the key's value, decided once
//! for every language: the schema's `const`, else for a list its default or
//! an empty list, for an object its default or its required children, else
//! its default, else a value left to type. A language only spells the plan
//! ([`ConfigLanguage::write_insert`]).

use super::*;

/// What a key suggestion writes for the key's value (see the module docs).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct InsertPlan<'a> {
    /// The value written.
    pub value: Planned<'a>,
    /// The required children an inserted block writes, in the order the
    /// schema lists them, each filled ([`Fill`]): the object's, or each
    /// item's where the key is a list of objects. Empty for a value the
    /// schema gives.
    pub children: Vec<RequiredChild<'a>>,
    /// A schema at the key describes an object (whatever is written).
    pub object: bool,
    /// A schema at the key describes a list.
    pub list: bool,
    /// The key is a list whose items are objects (a TOML `[[array]]`).
    pub list_of_objects: bool,
}

/// The value an [`InsertPlan`] writes.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Planned<'a> {
    /// A value the schema gives, its `const` or `default`, that the
    /// language reads back as written: written as it is.
    Value(&'a Value),
    /// A list, empty, to type its items into.
    List {
        /// Its items are text: a language quoting strings may write one
        /// empty item, the caret in it (`[""]`).
        string_items: bool,
        /// The schema's default is the empty list.
        empty_default: bool,
    },
    /// An object of the plan's [`children`](InsertPlan::children); with
    /// none, an empty one to type its entries into.
    Object {
        /// The schema names entries for it (`properties`,
        /// `patternProperties`, `additionalProperties`): an empty block
        /// waits for them on a line of its own where the language writes
        /// one, rather than an empty value (`{}`).
        named: bool,
    },
    /// A scalar left to type, nothing written for it.
    Empty {
        /// It is text: a language quoting strings writes empty quotes,
        /// the caret inside.
        string: bool,
    },
}

impl<'a> InsertPlan<'a> {
    /// The plan for a key whose value `property` describes.
    pub fn of(language: &dyn ConfigLanguage, property: &'a Value, root: &'a Value) -> Self {
        Self::at(language, property, None, root)
    }

    /// [`Self::of`] for a key `at` names every schema of (see
    /// `applies`): the required children every one of them asks for are
    /// written, those of a branch the document decides (the `then` of an
    /// `if` it takes) included.
    pub(crate) fn at(
        language: &dyn ConfigLanguage,
        property: &'a Value,
        at: Option<&Entry<'a>>,
        root: &'a Value,
    ) -> Self {
        // A list or an object only where every schema at the key allows
        // one (a pattern typing the key text beside the property's list).
        let types = at.map(|at| Here::walk(at.clone(), root, Seen::default()).types());
        let allows = |kind: &str| types.is_none_or(|types| types.allows_type(kind));
        let array = array_branch(property, root).filter(|_| allows("array"));
        let object = object_branch(property, root).filter(|_| allows("object"));
        let list_of_objects = array
            .and_then(|array| array.get("items"))
            .is_some_and(|items| object_branch(items, root).is_some());
        let default = || kept_default(language, property, root);
        let value = if let Some(value) = kept_const(language, property, root) {
            Planned::Value(value)
        } else if let Some(array) = array {
            match default() {
                Some(Value::Array(items)) if items.is_empty() => Planned::List {
                    string_items: string_items(array, root),
                    empty_default: true,
                },
                Some(default) => Planned::Value(default),
                None => Planned::List {
                    string_items: string_items(array, root),
                    empty_default: false,
                },
            }
        } else if let Some(branch) = object {
            match default().filter(|value| !value.as_object().is_some_and(Map::is_empty)) {
                Some(default) => Planned::Value(default),
                None => Planned::Object {
                    named: ["properties", "patternProperties", "additionalProperties"]
                        .iter()
                        .any(|keyword| branch.get(*keyword).is_some()),
                },
            }
        } else if let Some(default) = default() {
            Planned::Value(default)
        } else {
            Planned::Empty {
                string: placeholder(property, root).is_string()
                    || types.is_some_and(|types| types.is_only("string")),
            }
        };
        let children = match value {
            Planned::Value(_) | Planned::Empty { .. } => Vec::new(),
            _ => super::required::required_children_at(language, property, at, root),
        };
        Self {
            value,
            children,
            object: object.is_some(),
            list: array.is_some(),
            list_of_objects,
        }
    }
}

/// Whether the items of the list `array` describes are text.
fn string_items(array: &Value, root: &Value) -> bool {
    array
        .get("items")
        .is_some_and(|items| placeholder(items, root).is_string())
}

use serde_json::Map;
