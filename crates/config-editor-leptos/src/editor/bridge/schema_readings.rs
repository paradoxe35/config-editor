//! Values a view that writes every scalar as text (XML) read by the schema
//! (`8` for "an integer ≥ 1 or `auto`"), saved as read where the host's
//! type of them is one the schema does not allow.

use super::super::applies::{self, Applies};
use super::super::language::{SchemaPathSegment, pointer_segments};
use serde_json::Value;

/// Puts back each of `retyped` (a pointer into `value` and the view's
/// reading of the value there, which the host's type changed) where
/// `schema` refuses the host's type of it and takes the reading (by the
/// schemas at the value, or by the `oneOf`/`anyOf` of the object holding
/// it), or takes both and lists the reading (`enum: ["true"]`) and not
/// the host's type of it.
pub(super) fn keep(value: &mut Value, retyped: Vec<(String, Value)>, schema: &Value) {
    for (pointer, read) in retyped {
        let path = path_in(value, &pointer);
        // Every subschema at the path: the base property, the `allOf` and
        // `then` beside it, one of the options; and at the nearest object
        // above holding options, a few levels up at most.
        let (at, listed, above) = {
            let document = &*value;
            let whole = || Some(document);
            let applies = Applies::for_types(schema, false, &whole);
            let here = applies.at(&path);
            let listed: Vec<Value> = here
                .branches()
                .into_iter()
                .flat_map(|branch| applies::listed(branch).cloned())
                .collect();
            let above = (1..=OPTIONS_ABOVE.min(path.len())).find_map(|up| {
                let held = applies.at(&path[..path.len() - up]);
                held.has_options().then(|| (held.entry().clone(), up))
            });
            (here.entry().clone(), listed, above)
        };
        let takes = |candidate: &Value| at.accepts(candidate, schema);
        let Some(typed) = value.pointer(&pointer).cloned() else {
            continue;
        };
        // Whether the options above take the value with `candidate` there.
        let above_takes = |value: &mut Value, candidate: &Value| {
            let Some((entry, up)) = &above else {
                return false;
            };
            if let Some(slot) = value.pointer_mut(&pointer) {
                *slot = candidate.clone();
            }
            let held = (0..*up).fold(pointer.as_str(), |pointer, _| {
                pointer.rsplit_once('/').map_or("", |(parent, _)| parent)
            });
            value
                .pointer(held)
                .is_some_and(|held| entry.accepts_below(held, schema) == Some(true))
        };
        // The options above decide where they take one reading only (the
        // schemas at the value leave out an option silent on it).
        let put_back = (above_takes(value, &read) && !above_takes(value, &typed))
            || match (takes(&typed), takes(&read)) {
                (Some(false), Some(true)) => true,
                (Some(true), Some(true)) => listed.contains(&read) && !listed.contains(&typed),
                _ => false,
            };
        if let Some(slot) = value.pointer_mut(&pointer) {
            *slot = if put_back { read } else { typed };
        }
    }
}

/// Levels above a value [`keep`] looks for the options deciding it.
const OPTIONS_ABOVE: usize = 3;

/// The schema path of `pointer` in `value`: an array's segments are items.
fn path_in(value: &Value, pointer: &str) -> Vec<SchemaPathSegment> {
    let mut at = Some(value);
    pointer_segments(pointer)
        .into_iter()
        .map(|segment| {
            let index = at
                .and_then(Value::as_array)
                .and_then(|_| segment.parse::<usize>().ok());
            at = at.and_then(|value| match index {
                Some(index) => value.get(index),
                None => value.get(&segment),
            });
            match index {
                Some(index) => SchemaPathSegment::ArrayItem(index),
                None => SchemaPathSegment::Key(segment),
            }
        })
        .collect()
}
