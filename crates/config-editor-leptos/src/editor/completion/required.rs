//! What a block inserted for an object writes for its required children,
//! decided once for every language (each only writes it in its syntax):
//! a default the language keeps, a list as an empty list (`[]`), an
//! object's own required children filled the same way, the caret at the
//! first value left to type, and past it what the language keeps of a
//! value. A child whose key or value the language cannot write as text
//! reading back the same is left out, not written wrong.

use super::*;

/// One required child of an object, and what an inserted block writes for
/// it (see [`required_children`]).
#[derive(Clone, Debug, PartialEq)]
pub struct RequiredChild<'a> {
    /// Its key (as the schema names it, without a language's spelling of
    /// it: `ConfigLanguage::required_property`).
    pub key: &'a str,
    /// Its schema.
    pub schema: &'a Value,
    /// What is written for it.
    pub fill: Fill<'a>,
}

/// What an inserted block writes for a required child.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill<'a> {
    /// This value (a default, an empty list, a value Tab selects).
    Value(Value),
    /// An object of required children of its own, each filled the same way
    /// (written as a block below its key where the language writes one).
    Children(Vec<RequiredChild<'a>>),
    /// The value the caret goes to, left empty for the user to type; a
    /// string starts quoted, the caret inside, where the language quotes
    /// strings.
    Caret {
        /// Whether the value is text.
        string: bool,
    },
    /// This value with the caret at it (before it, inside an empty list):
    /// the block leaves no value to type, so the caret goes to its first.
    CaretAt(Value),
    /// No value to write: an empty value where the language has one (a
    /// YAML `key:`, an XML `<key></key>`), else this one (`""`, `null`,
    /// `{}`), else the child is left out.
    Empty(Option<Value>),
}

/// How deep required children of required children are filled (a schema
/// requiring itself ends there).
const MAX_DEPTH: usize = 8;

/// The required children of the object `schema` describes (or of its
/// list's items, for a key a list of objects repeats), in the order the
/// schema lists them, each with what a block inserted for it writes.
pub fn required_children<'a>(
    language: &dyn ConfigLanguage,
    schema: &'a Value,
    root: &'a Value,
) -> Vec<RequiredChild<'a>> {
    let mut caret = false;
    let mut found = children(language, schema, root, &mut caret, 0);
    if !caret {
        caret_at_first_value(&mut found);
    }
    found
}

/// Puts the caret at the first value of `children`, depth first; `false`
/// when there is none.
fn caret_at_first_value(children: &mut [RequiredChild<'_>]) -> bool {
    children.iter_mut().any(|child| match &mut child.fill {
        Fill::Value(value) => {
            child.fill = Fill::CaretAt(value.take());
            true
        }
        Fill::Children(nested) => caret_at_first_value(nested),
        _ => false,
    })
}

fn children<'a>(
    language: &dyn ConfigLanguage,
    schema: &'a Value,
    root: &'a Value,
    caret: &mut bool,
    depth: usize,
) -> Vec<RequiredChild<'a>> {
    if depth >= MAX_DEPTH {
        return Vec::new();
    }
    let items = || {
        array_branch(schema, root)
            .and_then(|array| array.get("items"))
            .and_then(|items| object_branch(items, root))
    };
    let Some(branch) = object_branch(schema, root).or_else(items) else {
        return Vec::new();
    };
    let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for name in branch
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let key = language.required_property(name);
        let Some((key, child)) = properties.get_key_value(key) else {
            continue;
        };
        if !writes_key(language, key) {
            continue;
        }
        let Some(fill) = fill(language, child, root, caret, depth) else {
            continue;
        };
        found.push(RequiredChild {
            key: key.as_str(),
            schema: child,
            fill,
        });
    }
    found
}

/// What is written for one required child; `None` leaves it out.
fn fill<'a>(
    language: &dyn ConfigLanguage,
    child: &'a Value,
    root: &'a Value,
    caret: &mut bool,
    depth: usize,
) -> Option<Fill<'a>> {
    if let Some(default) = kept_default(language, child, root) {
        return Some(Fill::Value(default.clone()));
    }
    if array_branch(child, root).is_some() && object_branch(child, root).is_none() {
        let empty = Value::Array(Vec::new());
        return language_keeps(language, &empty, child, root).then_some(Fill::Value(empty));
    }
    if object_branch(child, root).is_some() {
        let nested = children(language, child, root, caret, depth + 1);
        if !nested.is_empty() {
            return Some(Fill::Children(nested));
        }
    }
    if !*caret {
        *caret = true;
        return Some(Fill::Caret {
            string: placeholder(child, root).is_string(),
        });
    }
    Some(match filled_past_caret(language, child, root) {
        Some(value) => Fill::Value(value),
        None => Fill::Empty(fill_value(language, child, root)),
    })
}

/// Whether `language` writes `key` (of an object) as text that reads back
/// as that key: as its own writer does, which refuses a key it cannot
/// hold (an XML name with a space). A language that writes no documents
/// at all is trusted with every key.
pub fn writes_key(language: &dyn ConfigLanguage, key: &str) -> bool {
    let document = serde_json::json!({ "r": { key: 0 } });
    let Ok(text) = language.serialize(&document, None) else {
        let empty = serde_json::json!({ "r": {} });
        return language.serialize(&empty, None).is_err();
    };
    language.parse(&text).ok().is_some_and(|read| {
        read.get("r")
            .and_then(Value::as_object)
            .is_some_and(|members| members.contains_key(key))
    })
}
