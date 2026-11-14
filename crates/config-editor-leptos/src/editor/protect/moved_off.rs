//! A placeholder the host gave in a field that an Edit-as save no longer
//! holds there, though it still writes it in other fields (the field's
//! element copied in an XML view, making the field a list of copies): the
//! host would read each copy as plain text and the secret would be gone,
//! so the save waits, as the copies are flagged.

use std::collections::HashMap;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::super::paths::field_key;
use super::tokens::{Known, SecretMatcher, field_value};

/// Whether `document` holds `token` in `field` itself (a lone element read
/// as a list of one included), not in a list of several made of it.
pub(crate) fn held_in_own_field(document: &Value, field: &str, token: &str) -> bool {
    match field_value(document, field) {
        Some(Value::String(text)) => text.contains(token),
        Some(Value::Array(items)) if items.len() == 1 => {
            items[0].as_str().is_some_and(|text| text.contains(token))
        }
        _ => false,
    }
}

/// Whether the place `pointer` of a token is the field `field` the host
/// gave it in: the same pointer, or the pointer of a lone item of it (an
/// XML element read as a list of one) while `document` (when it parses)
/// holds the field as one value, not as a list of several.
pub(crate) fn is_own_field(document: Option<&Value>, field: &str, pointer: &str) -> bool {
    field == pointer
        || (super::super::paths::same_field(field, pointer)
            && document.is_none_or(|document| {
                field_value(document, field)
                    .is_some_and(|value| value.as_array().is_none_or(|items| items.len() == 1))
            }))
}

/// The first field of the host text `new` (in `behavior`) holding one of
/// `known`'s field tokens that is no longer in its own field, with that
/// field (the token's owner); `None` when every such token is still held
/// where the host gave it, or is gone.
pub(crate) fn moved_off_in_save(
    behavior: &dyn ConfigLanguage,
    matcher: &SecretMatcher,
    known: &[Known],
    new: &str,
    reading: Option<&Value>,
) -> Option<(String, String)> {
    if known.iter().all(|known| known.field.is_none()) {
        return None;
    }
    let document = super::super::codec::read(behavior, new, reading).ok()?;
    let holders = matcher.holders(&document);
    // Each token's fields by what names them alike: a lookup per holder,
    // never a pass over them (one mask in thousands of fields).
    let mut fields: HashMap<&str, HashMap<String, Vec<&str>>> = HashMap::new();
    for known in known {
        if let Some(field) = known.field.as_deref() {
            fields
                .entry(known.token.as_str())
                .or_default()
                .entry(field_key(field))
                .or_default()
                .push(field);
        }
    }
    known.iter().find_map(|secret| {
        let field = secret.field.as_deref()?;
        if held_in_own_field(&document, field, &secret.token) {
            return None;
        }
        // Not a field the host gave the same token in too.
        let own = |pointer: &str| {
            fields
                .get(secret.token.as_str())
                .and_then(|fields| fields.get(&field_key(pointer)))
                .is_some_and(|others| {
                    others
                        .iter()
                        .any(|other| is_own_field(Some(&document), other, pointer))
                })
        };
        let elsewhere = holders
            .get(&secret.token)?
            .iter()
            .find(|pointer| !own(pointer))?;
        Some((elsewhere.clone(), field.to_string()))
    })
}
