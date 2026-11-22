//! Values a view that writes every scalar as text (XML) read by the schema
//! (`8` for "an integer ≥ 1 or `auto`"), saved as read where the host's
//! type of them is one the schema does not allow, by every schema judging
//! the value where it is.

use super::super::applies::{self, Applies, Entry};
use super::super::language::{SchemaPathSegment, pointer_segments};
use serde_json::Value;

/// Puts back each of `retyped` (a pointer into `value` and the view's
/// reading of the value there, which the host's type changed) where
/// `schema` takes the reading and refuses the host's type of it, or takes
/// both and lists the reading (`enum: ["true"]`) and not the host's type.
///
/// The schemas judging the value are those the path leads to and, above
/// it, every value whose schemas judge it together with its siblings
/// (options, `contains`, see [`Here::judges_children_together`]). The
/// outermost that tells the two readings apart decides: what it judges
/// holds what the nearer ones do, and a nearer schema may be one of its
/// options only.
///
/// [`Here::judges_children_together`]: applies::Here::judges_children_together
pub(super) fn keep(value: &mut Value, retyped: Vec<(String, Value)>, schema: &Value) {
    for (pointer, read) in retyped {
        let Some(typed) = value.pointer(&pointer).cloned() else {
            continue;
        };
        let path = path_in(value, &pointer);
        // Each judging value above, outermost first, as the pointer to it
        // and what applies there; then what applies at the value.
        let (above, at, listed) = {
            let document = &*value;
            let whole = || Some(document);
            let applies = Applies::for_types(schema, false, &whole);
            let above: Vec<(&str, Entry<'_>)> = (0..path.len())
                .filter_map(|depth| {
                    let held = applies.at(&path[..depth]);
                    held.judges_children_together().then(|| {
                        (
                            parent_pointer(&pointer, path.len() - depth),
                            held.entry().clone(),
                        )
                    })
                })
                .collect();
            let here = applies.at(&path);
            let listed: Vec<Value> = here
                .branches()
                .into_iter()
                .flat_map(|branch| applies::listed(branch).cloned())
                .collect();
            (above, here.entry().clone(), listed)
        };
        // The value at `held` with `candidate` at the pointer.
        let with = |value: &mut Value, held: &str, candidate: &Value| {
            if let Some(slot) = value.pointer_mut(&pointer) {
                *slot = candidate.clone();
            }
            value.pointer(held).cloned()
        };
        // A part of the schemas at a level refusing both readings (the
        // document fails there whichever is saved) tells nothing; one
        // refusing each reading refuses the reading too.
        let decided = above.iter().find_map(|(held, entry)| {
            let as_typed = with(value, held, &typed)?;
            let as_read = with(value, held, &read)?;
            let told = entry.refusing(&as_typed, &as_read, schema);
            match (told.first, told.second) {
                (true, false) => Some(true),
                (_, true) => Some(false),
                (false, false) => None,
            }
        });
        let put_back = decided.unwrap_or_else(|| {
            match (at.accepts(&typed, schema), at.accepts(&read, schema)) {
                (Some(false), Some(true)) => true,
                (Some(true), Some(true)) => listed.contains(&read) && !listed.contains(&typed),
                _ => false,
            }
        });
        if let Some(slot) = value.pointer_mut(&pointer) {
            *slot = if put_back { read } else { typed };
        }
    }
}

/// Whether the schemas at `pointer` in `document` take `candidate` there
/// (`schema`: the document's), as the reader of a host whose values are
/// all text judges which reading of a text it takes.
pub(super) fn takes(document: &Value, pointer: &str, candidate: &Value, schema: &Value) -> bool {
    let whole = || Some(document);
    let applies = Applies::for_types(schema, false, &whole);
    let here = applies.at(&path_in(document, pointer));
    here.entry().accepts(candidate, schema) != Some(false)
}

/// The pointer `up` levels above `pointer`.
fn parent_pointer(pointer: &str, up: usize) -> &str {
    (0..up).fold(pointer, |pointer, _| {
        pointer.rsplit_once('/').map_or("", |(parent, _)| parent)
    })
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
