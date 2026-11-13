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
    let Ok(mut read) = language.parse(&text) else {
        return false;
    };
    let Some(read) = read.pointer_mut("/r/k").map(Value::take) else {
        return false;
    };
    let mut read = read;
    if language.repetition() != Repetition::None {
        conform(&mut read, property, root, language.repetition(), 0);
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
/// ([`schema_example`]), else [`placeholder`] — the first of them that
/// `language` keeps, else nothing (an empty value Tab stops at).
pub fn fill_value(language: &dyn ConfigLanguage, schema: &Value, root: &Value) -> Option<Value> {
    let keeps = |value: &Value| language_keeps(language, value, schema, root);
    let branches = resolve_branches(schema, root);
    branches
        .iter()
        .find_map(|branch| branch.get("default").or_else(|| branch.get("const")))
        .filter(|value| keeps(value))
        .cloned()
        .or_else(|| schema_example(schema, root).filter(|value| keeps(value)))
        .or_else(|| Some(placeholder(schema, root)).filter(|value| keeps(value)))
}

/// What a required child gets in a block where an empty value reads as
/// none (a YAML `key:`, an XML `<key></key>`): its kept default, and past
/// the child the caret goes to, its [`fill_value`] unless that is an empty
/// value (`""`, `null`, `{}`, `[]`), which the empty value stands for.
pub fn child_fill(
    language: &dyn ConfigLanguage,
    schema: &Value,
    root: &Value,
    takes_caret: bool,
) -> Option<Value> {
    if let Some(default) = kept_default(language, schema, root) {
        return Some(default.clone());
    }
    if takes_caret {
        return None;
    }
    fill_value(language, schema, root).filter(|value| match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(members) => !members.is_empty(),
        _ => true,
    })
}
