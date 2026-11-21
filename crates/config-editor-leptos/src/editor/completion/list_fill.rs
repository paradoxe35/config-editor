//! A list an inserted block writes with items (its least list, see
//! [`least_list`]) as [`Fill::List`]: each item filled as a required child
//! is, so an object item's members carry the marks a single object's do
//! (an XML attribute, written first in the start tag).

use super::required::required_keys;
use super::*;

/// The fill of the least list `items` written at `at`; `caret`: the caret
/// goes to its first item's value.
pub(super) fn list_fill<'a>(
    language: &dyn ConfigLanguage,
    items: Vec<Value>,
    at: &Entry<'a>,
    root: &'a Value,
    caret: bool,
) -> Fill<'a> {
    Fill::List {
        items: items_filled(language, items, at, root),
        caret,
    }
}

/// Each of `items` (of the list at `at`) as a fill.
fn items_filled<'a>(
    language: &dyn ConfigLanguage,
    items: Vec<Value>,
    at: &Entry<'a>,
    root: &'a Value,
) -> Vec<Fill<'a>> {
    let list = Here::walk(at.clone(), root, Seen::default());
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let at = list.child(&SchemaPathSegment::ArrayItem(index));
            value_filled(language, item, &at, root)
        })
        .collect()
}

/// `value` (written at `at`) as a fill: an object as its members, each a
/// required child of it, a list with items as its items, else the value.
fn value_filled<'a>(
    language: &dyn ConfigLanguage,
    value: Value,
    at: &Entry<'a>,
    root: &'a Value,
) -> Fill<'a> {
    match value {
        Value::Array(items) if !items.is_empty() => Fill::List {
            items: items_filled(language, items, at, root),
            caret: false,
        },
        Value::Object(members) if !members.is_empty() => {
            match members_filled(language, &members, at, root) {
                Some(children) => Fill::Children(children),
                None => Fill::Value(Value::Object(members)),
            }
        }
        value => Fill::Value(value),
    }
}

/// The members of an object item as its required children (by the keys
/// its schemas require, the ones the fill wrote it for), attributes first
/// as [`required_children`] orders them; `None` where those keys are not
/// its members (the item is then written as its value).
fn members_filled<'a>(
    language: &dyn ConfigLanguage,
    members: &serde_json::Map<String, Value>,
    at: &Entry<'a>,
    root: &'a Value,
) -> Option<Vec<RequiredChild<'a>>> {
    let schema = at
        .schemas()
        .into_iter()
        .find(|schema| object_branch(schema, root).is_some())?;
    let object = Value::Object(members.clone());
    let keys = required_keys(language, schema, Some(at), root, Some(&object));
    if keys.len() != members.len() || keys.iter().any(|(key, ..)| !members.contains_key(*key)) {
        return None;
    }
    let mut children: Vec<RequiredChild<'a>> = keys
        .into_iter()
        .map(|(key, child, at)| {
            let attribute =
                super::insert_plan::marks_attribute(&Here::walk(at.clone(), root, Seen::default()));
            RequiredChild {
                key,
                schema: child,
                fill: value_filled(language, members[key].clone(), &at, root),
                attribute,
            }
        })
        .collect();
    children.sort_by_key(|child| !child.attribute);
    Some(children)
}
