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
    language_keeps_at(language, value, &Entry::schema(property), root)
}

/// [`language_keeps`] for a value `at` names every schema of, read as they
/// all expect: a list an `allOf` member beside its property asks for has
/// its one XML element read as a list.
pub(super) fn language_keeps_at(
    language: &dyn ConfigLanguage,
    value: &Value,
    at: &Entry<'_>,
    root: &Value,
) -> bool {
    let document = serde_json::json!({ "r": { "k": value } });
    let Ok(text) = language.write_document(&document, None) else {
        // Refused: the value, unless the language writes nothing.
        let empty = serde_json::json!({ "r": {} });
        return language.write_document(&empty, None).is_err();
    };
    let Ok(mut read) = language.parse(&text) else {
        return false;
    };
    let Some(read) = read.pointer_mut("/r/k").map(Value::take) else {
        return false;
    };
    super::super::data_reading::value_under(language, read, at, root) == *value
}

/// The `default` of `property` ([`schema_default`]) when `language` keeps
/// it ([`language_keeps`]) and the schemas applying to the value do not
/// refuse it (a `default` beside an `allOf` asking another type); a
/// suggestion writes it, else nothing.
pub fn kept_default<'a>(
    language: &dyn ConfigLanguage,
    property: &'a Value,
    root: &'a Value,
) -> Option<&'a Value> {
    schema_default(property, root).filter(|value| {
        decides(property, value, root) != Some(false)
            && language_keeps(language, value, property, root)
    })
}

/// The `const` of `property` ([`schema_const`]) when `language` keeps it.
pub fn kept_const<'a>(
    language: &dyn ConfigLanguage,
    property: &'a Value,
    root: &'a Value,
) -> Option<&'a Value> {
    schema_const(property, root).filter(|value| {
        decides(property, value, root) != Some(false)
            && language_keeps(language, value, property, root)
    })
}

/// What a required child past the one the caret goes to gets: its
/// [`fill_value`] unless that is an empty value (`""`, `null`, `{}`),
/// which an empty value stands for (see `required::Fill::Empty`).
pub(super) fn filled_past_caret(
    language: &dyn ConfigLanguage,
    schema: &Value,
    at: &Entry<'_>,
    root: &Value,
) -> Option<Value> {
    super::fill::fill_value_at(language, schema, at, root).filter(|value| match value {
        Value::Null => false,
        Value::String(text) => !text.is_empty(),
        Value::Object(members) => !members.is_empty(),
        _ => true,
    })
}
