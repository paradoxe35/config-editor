//! The schema values a suggestion writes for a key: only those the
//! language reads back as they are. A default the language cannot hold
//! (TOML has no null, XML no null either) is left out, and the caret
//! stays where the user types the value.

use super::*;

/// Whether `language` writes `value`, as the value of a key `property`
/// describes, as text it reads back equal (read the way the schema expects,
/// a lone XML element as a one-item list). A language that writes no
/// documents at all is trusted with every value.
pub fn language_keeps(
    language: &dyn ConfigLanguage,
    value: &Value,
    property: &Value,
    root: &Value,
) -> bool {
    let document = serde_json::json!({ "r": { "k": value } });
    let Ok(text) = language.serialize(&document, None) else {
        // Refused: the value, unless the language writes nothing.
        let empty = serde_json::json!({ "r": {} });
        return language.serialize(&empty, None).is_err();
    };
    let text_scalars = language.scalars() == Scalars::Text;
    let read = if text_scalars {
        language.parse_text(&text)
    } else {
        language.parse(&text)
    };
    let Ok(mut read) = read else {
        return false;
    };
    let Some(read) = read.pointer_mut("/r/k").map(Value::take) else {
        return false;
    };
    let mut read = read;
    // Read as the schema types it there.
    if text_scalars {
        super::text_typing::type_texts(&mut read, property, root, language);
    }
    if language.repetition() != Repetition::None {
        conform(
            &mut read,
            property,
            root,
            language.repetition(),
            0,
            text_scalars,
        );
    }
    read == *value
}

/// The `default` of `property` ([`schema_default`]) when `language` keeps
/// it ([`language_keeps`]); a suggestion writes it, else nothing.
pub fn kept_default<'a>(
    language: &dyn ConfigLanguage,
    property: &'a Value,
    root: &'a Value,
) -> Option<&'a Value> {
    schema_default(property, root).filter(|value| language_keeps(language, value, property, root))
}

/// The `const` of `property` ([`schema_const`]) when `language` keeps it.
pub fn kept_const<'a>(
    language: &dyn ConfigLanguage,
    property: &'a Value,
    root: &'a Value,
) -> Option<&'a Value> {
    schema_const(property, root).filter(|value| language_keeps(language, value, property, root))
}

/// What an inserted block writes for a required child the caret does not
/// go to: its default or const, else its first example
/// ([`schema_example`]), else [`placeholder`] — an object's own required
/// children filled the same way (`{cpu: 2}`, not `{}`) — the first of them
/// that `language` keeps, else nothing (an empty value Tab stops at).
pub fn fill_value(language: &dyn ConfigLanguage, schema: &Value, root: &Value) -> Option<Value> {
    filled(language, schema, root, 0)
}

/// How deep [`fill_value`] fills required children of required children
/// (a schema requiring itself ends there).
const MAX_FILL_DEPTH: usize = 8;

fn filled(
    language: &dyn ConfigLanguage,
    schema: &Value,
    root: &Value,
    depth: usize,
) -> Option<Value> {
    let keeps = |value: &Value| language_keeps(language, value, schema, root);
    let branches = resolve_branches(schema, root);
    branches
        .iter()
        .find_map(|branch| branch.get("default").or_else(|| branch.get("const")))
        .filter(|value| keeps(value))
        .cloned()
        .or_else(|| schema_example(schema, root).filter(|value| keeps(value)))
        .or_else(|| {
            let value = match placeholder(schema, root) {
                Value::Object(members) if members.is_empty() => {
                    required_filled(language, schema, root, depth).unwrap_or(Value::Object(members))
                }
                other => other,
            };
            Some(value).filter(|value| keeps(value))
        })
}

/// An object of `schema`'s required children, each [`filled`], or `None`
/// when none is.
fn required_filled(
    language: &dyn ConfigLanguage,
    schema: &Value,
    root: &Value,
    depth: usize,
) -> Option<Value> {
    if depth >= MAX_FILL_DEPTH {
        return None;
    }
    let branch = object_branch(schema, root)?;
    let properties = branch.get("properties").and_then(Value::as_object)?;
    let mut members = serde_json::Map::new();
    for name in branch
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let key = language.required_property(name);
        let Some(child) = properties.get(key) else {
            continue;
        };
        if let Some(value) = filled(language, child, root, depth + 1) {
            members.insert(key.to_string(), value);
        }
    }
    (!members.is_empty()).then_some(Value::Object(members))
}

/// What a required child past the one the caret goes to gets: its
/// [`fill_value`] unless that is an empty value (`""`, `null`, `{}`),
/// which an empty value stands for (see `required::Fill::Empty`).
pub(super) fn filled_past_caret(
    language: &dyn ConfigLanguage,
    schema: &Value,
    root: &Value,
) -> Option<Value> {
    fill_value(language, schema, root).filter(|value| match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        Value::Object(members) => !members.is_empty(),
        _ => true,
    })
}
