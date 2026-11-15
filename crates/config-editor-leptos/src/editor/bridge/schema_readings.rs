//! Values a view that writes every scalar as text (XML) read by the schema
//! (`8` for "an integer ≥ 1 or `auto`"), saved as read where the host's
//! type of them is one the schema does not allow.

use super::super::completion::{decides, schema_at_path_in};
use super::super::language::{SchemaPathSegment, pointer_segments};
use serde_json::Value;

/// Puts back each of `retyped` (a pointer into `value` and the view's
/// reading of the value there, which the host's type changed) where
/// `schema` refuses the host's type of it and takes the reading.
pub(super) fn keep(value: &mut Value, retyped: Vec<(String, Value)>, schema: &Value) {
    for (pointer, read) in retyped {
        let path = path_in(value, &pointer);
        let schemas = schema_at_path_in(schema, &path, false, Some(value));
        // Taken where one of them takes it, unknown where one cannot tell.
        let takes = |candidate: &Value| {
            let decided: Vec<Option<bool>> = schemas
                .iter()
                .map(|at| decides(at, candidate, schema))
                .collect();
            if decided.contains(&Some(true)) {
                Some(true)
            } else if decided.contains(&None) {
                None
            } else {
                Some(false)
            }
        };
        let Some(typed) = value.pointer(&pointer) else {
            continue;
        };
        if takes(typed) == Some(false)
            && takes(&read) == Some(true)
            && let Some(slot) = value.pointer_mut(&pointer)
        {
            *slot = read;
        }
    }
}

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
