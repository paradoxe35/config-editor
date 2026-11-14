//! The values an inserted block writes for required children it does not
//! leave to type: a default, an example, a placeholder, an object's own
//! required children and a list's least items (see [`least`]), only what
//! the language reads back as written.

use super::least::least_items;
use super::*;

/// What an inserted block writes for a required child the caret does not
/// go to: its default or const, else its first example
/// ([`schema_example`]), else [`placeholder`] — an object's own required
/// children filled the same way (`{cpu: 2}`, not `{}`) — the first of them
/// that `language` keeps, else nothing (an empty value Tab stops at).
pub fn fill_value(language: &dyn ConfigLanguage, schema: &Value, root: &Value) -> Option<Value> {
    Filler::new(language, root).filled(schema, 0)
}

/// The list a list `schema` describes is written as (see [`least_items`]).
pub(super) fn least_list(language: &dyn ConfigLanguage, schema: &Value, root: &Value) -> Value {
    least_items(&mut Filler::new(language, root), schema, 0)
}

/// How deep required children and list items are filled (a schema requiring itself ends there).
pub(super) const MAX_FILL_DEPTH: usize = 8;

/// Most list items one fill writes in all: a schema asking a million
/// items (or lists of lists of a hundred) would otherwise write a document
/// too large to edit. A list asking more than is left gets what is left.
const MAX_FILLED_ITEMS: usize = 256;

/// List items kept past [`MAX_FILLED_ITEMS`], one for each list asking
/// items after it is spent, so a list written as repeated keys (XML) is
/// still written.
pub(super) const RESERVED_ITEMS: usize = 64;

/// One fill: the language, the schema root, and the list items it may
/// still write.
pub(super) struct Filler<'a> {
    pub(super) language: &'a dyn ConfigLanguage,
    pub(super) root: &'a Value,
    /// List items still to be written at most ([`MAX_FILLED_ITEMS`] and
    /// [`RESERVED_ITEMS`]).
    pub(super) items_left: usize,
}

impl<'a> Filler<'a> {
    fn new(language: &'a dyn ConfigLanguage, root: &'a Value) -> Self {
        Self {
            language,
            root,
            items_left: MAX_FILLED_ITEMS + RESERVED_ITEMS,
        }
    }

    /// Whether the language reads `value`, as a value of `schema`, back as
    /// written ([`language_keeps`]).
    pub(super) fn keeps(&self, value: &Value, schema: &Value) -> bool {
        language_keeps(self.language, value, schema, self.root)
    }

    /// [`fill_value`] at `depth`.
    pub(super) fn filled(&mut self, schema: &Value, depth: usize) -> Option<Value> {
        let root = self.root;
        let branches = resolve_branches(schema, root);
        if let Some(value) = branches
            .iter()
            .find_map(|branch| branch.get("default").or_else(|| branch.get("const")))
            .filter(|value| self.keeps(value, schema))
        {
            return Some(value.clone());
        }
        if let Some(example) =
            schema_example(schema, root).filter(|value| self.keeps(value, schema))
        {
            return Some(example);
        }
        let value = match placeholder(schema, root) {
            Value::Object(members) if members.is_empty() => self
                .required_filled(schema, depth)
                .unwrap_or(Value::Object(members)),
            Value::Array(items) if items.is_empty() => least_items(self, schema, depth),
            other => other,
        };
        Some(value).filter(|value| self.keeps(value, schema))
    }

    /// An object of `schema`'s required children, each [`Self::filled`],
    /// or `None` when none is.
    fn required_filled(&mut self, schema: &Value, depth: usize) -> Option<Value> {
        if depth >= MAX_FILL_DEPTH {
            return None;
        }
        let branch = object_branch(schema, self.root)?;
        let properties = branch.get("properties").and_then(Value::as_object)?;
        let mut members = serde_json::Map::new();
        for name in branch
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let key = self.language.required_property(name);
            let Some(child) = properties.get(key) else {
                continue;
            };
            if let Some(value) = self.filled(child, depth + 1) {
                members.insert(key.to_string(), value);
            }
        }
        (!members.is_empty()).then_some(Value::Object(members))
    }
}
