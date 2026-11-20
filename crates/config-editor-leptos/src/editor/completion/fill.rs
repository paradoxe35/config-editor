//! The values an inserted block writes for required children it does not
//! leave to type: a default, an example, a placeholder, an object's own
//! required children and a list's least items (see [`least`]), only what
//! the language reads back as written.

use super::least::{least_items, others};
use super::*;

/// What an inserted block writes for a required child the caret does not
/// go to: its default or const, else its first example
/// ([`schema_example`]), else [`placeholder`] — an object's own required
/// children filled the same way (`{cpu: 2}`, not `{}`) — the first of them
/// that `language` keeps, else nothing (an empty value Tab stops at).
pub fn fill_value(language: &dyn ConfigLanguage, schema: &Value, root: &Value) -> Option<Value> {
    Filler::new(language, root).filled(schema, None, 0)
}

/// [`fill_value`] for a value `at` names every schema of (its property,
/// a pattern, a branch beside them): what it writes they all allow.
pub(super) fn fill_value_at(
    language: &dyn ConfigLanguage,
    schema: &Value,
    at: &Entry<'_>,
    root: &Value,
) -> Option<Value> {
    Filler::new(language, root).filled(schema, Some(at), 0)
}

/// The list a list `schema` describes is written as (see [`least_items`]),
/// where `at` names every schema applying to the value: its own least
/// list where they take it, else the first of the other schemas' (an
/// `allOf` member's `minItems` beside the property's `type: array`) that
/// asks items and that they all take.
pub(super) fn least_list(
    language: &dyn ConfigLanguage,
    schema: &Value,
    at: &Entry<'_>,
    root: &Value,
) -> Value {
    let mut filler = Filler::new(language, root);
    let own = least_items(&mut filler, schema, 0);
    if at.accepts(&own, root) != Some(false) {
        return own;
    }
    Here::walk(at.clone(), root, Seen::default())
        .certain()
        .into_iter()
        .filter(|branch| !std::ptr::eq(*branch, schema))
        .filter_map(|branch| {
            // A schema asking items of the list (`minItems`, `contains`)
            // without naming its type is read as the list's.
            let mut list = branch.as_object()?.clone();
            if !list.contains_key("minItems") && !list.contains_key("contains") {
                return None;
            }
            list.entry("type").or_insert_with(|| Value::from("array"));
            Some(least_items(&mut filler, &Value::Object(list), 0))
        })
        .find(|list| {
            list.as_array().is_some_and(|items| !items.is_empty())
                && at.accepts(list, root) == Some(true)
        })
        .unwrap_or(own)
}

/// Values past each bound [`Filler::filled`] tries when its own are refused.
const OTHERS: usize = 4;

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

    /// Whether `schema` (every schema applying to a value of it) does not
    /// refuse `value`, as far as the quick check tells; `strict`: nor
    /// does a branch of an `if` not decided that says something of it.
    fn allows(&self, value: &Value, schema: &Value, at: Option<&Entry<'_>>, strict: bool) -> bool {
        decides(schema, value, self.root) != Some(false)
            && at.is_none_or(|at| {
                let accepted = if strict {
                    at.accepts_in_a_branch(value, self.root)
                } else {
                    at.accepts(value, self.root)
                };
                accepted != Some(false)
            })
    }

    /// [`fill_value`] at `depth`: the first of its default or const, its
    /// example and its placeholder that the language keeps and the schema
    /// allows, else another value the schema allows ([`others`]: an enum
    /// member, a number past its bound, …), each one meeting an `if` not
    /// decided whichever way it goes before one meeting only the way it
    /// may go; else the placeholder the language keeps where the schemas
    /// at the value (`at`) do not refuse it, else an empty value of a kind
    /// they allow.
    /// `at`: every schema applying to the value, where more than `schema`.
    pub(super) fn filled(
        &mut self,
        schema: &Value,
        at: Option<&Entry<'_>>,
        depth: usize,
    ) -> Option<Value> {
        let root = self.root;
        // The first value taken either way; else the first taken one way.
        let mut taken: Option<Value> = None;
        let mut take = |this: &Self, value: &Value| {
            if !this.keeps(value, schema) || !this.allows(value, schema, at, false) {
                return false;
            }
            if this.allows(value, schema, at, true) {
                return true;
            }
            taken.get_or_insert_with(|| value.clone());
            false
        };
        let branches = resolve_branches(schema, root);
        if let Some(value) = branches
            .iter()
            .find_map(|branch| branch.get("default").or_else(|| branch.get("const")))
            .filter(|value| take(self, value))
        {
            return Some(value.clone());
        }
        if let Some(example) = schema_example(schema, root).filter(|value| take(self, value)) {
            return Some(example);
        }
        let value = match placeholder(schema, root) {
            Value::Object(members) if members.is_empty() => self
                .required_filled(schema, depth)
                .unwrap_or(Value::Object(members)),
            Value::Array(items) if items.is_empty() => least_items(self, schema, depth),
            other => other,
        };
        if take(self, &value) {
            return Some(value);
        }
        let empty = [
            Value::String(String::new()),
            Value::from(0),
            Value::Bool(false),
            Value::Null,
            Value::Array(Vec::new()),
            Value::Object(serde_json::Map::new()),
        ];
        if let Some(other) = others(schema, root, std::slice::from_ref(&value), OTHERS)
            .into_iter()
            .chain(empty)
            .find(|other| take(self, other))
        {
            return Some(other);
        }
        taken
            // A schema no value meets keeps its placeholder; one the other
            // schemas at the value refuse is not written.
            .or_else(|| {
                Some(value).filter(|value| {
                    self.keeps(value, schema)
                        && at.is_none_or(|at| at.accepts(value, root) != Some(false))
                })
            })
            // Else an empty value of a kind they allow, left to type.
            .or_else(|| {
                let types = Here::walk(at?.clone(), root, Seen::default()).types();
                [
                    Value::String(String::new()),
                    Value::Object(serde_json::Map::new()),
                    Value::Null,
                ]
                .into_iter()
                .find(|empty| types.allows(empty) && self.keeps(empty, schema))
            })
    }

    /// An object of `schema`'s required children, each [`Self::filled`],
    /// or `None` when none is.
    fn required_filled(&mut self, schema: &Value, depth: usize) -> Option<Value> {
        if depth >= MAX_FILL_DEPTH {
            return None;
        }
        object_branch(schema, self.root)?;
        let mut members = serde_json::Map::new();
        for (key, child, at) in
            super::required::required_keys(self.language, schema, None, self.root)
        {
            if let Some(value) = self.filled(child, Some(&at), depth + 1) {
                members.insert(key.to_string(), value);
            }
        }
        (!members.is_empty()).then_some(Value::Object(members))
    }
}
