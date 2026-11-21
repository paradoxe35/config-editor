//! What the schema says of how XML writes a value: the OpenAPI XML object's
//! `"xml": {"attribute": true}` on a property makes it an attribute.
//! Other views ignore the keyword (as the validator does), so one schema
//! serves every language. The schema is read with the one walker
//! (`applies`); the suggestions' side of the keyword is decided in core
//! (`InsertPlan::attribute`, `InsertContext::attributes_marked`).

use super::*;
use super::super::super::applies::{Next, Walking, each_value};

/// The key paths (array indices left out, as [`AttributePaths`] holds
/// them) of the scalars of `value` that `schema` marks attributes, but not
/// those `old` (the document before) holds as elements: the text keeps
/// what it writes, the schema only decides what is new.
pub(super) fn schema_attribute_paths(value: &Value, schema: &Value, old: Option<&Value>) -> AttributePaths {
    let mut found = AttributePaths::new();
    each_value(
        value,
        schema,
        Walking { decided: false, repeats_arrays: true },
        &mut |at| {
            let Some((keys, old_here)) = follow(value, old, at.pointer) else {
                return Next::Past;
            };
            let scalar = matches!(
                value.pointer(at.pointer),
                Some(Value::String(_) | Value::Number(_) | Value::Bool(_))
            );
            let marked = at
                .here
                .branches()
                .iter()
                .any(|branch| branch.pointer("/xml/attribute") == Some(&Value::Bool(true)));
            if scalar && marked && old_here.is_none() && !keys.is_empty() {
                found.insert(keys);
            }
            Next::Into
        },
    );
    found
}

/// The keys on the way to `pointer` in `value` (array indices left out),
/// and what `old` holds there.
fn follow<'o>(value: &Value, old: Option<&'o Value>, pointer: &str) -> Option<(Vec<String>, Option<&'o Value>)> {
    let mut keys = Vec::new();
    let (mut here, mut before) = (value, old);
    for token in pointer_segments(pointer) {
        match here {
            Value::Object(map) => {
                here = map.get(&token)?;
                before = before.and_then(|before| before.get(&token));
                keys.push(token);
            }
            Value::Array(items) => {
                let index: usize = token.parse().ok()?;
                here = items.get(index)?;
                before = before.and_then(|before| match before {
                    Value::Array(items) => items.get(index),
                    // A lone element read where the list now is.
                    other if index == 0 => Some(other),
                    _ => None,
                });
            }
            _ => return None,
        }
    }
    Some((keys, before))
}
