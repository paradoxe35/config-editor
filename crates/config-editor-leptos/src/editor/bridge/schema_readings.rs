//! Which of two readings of a value the schema prefers, for a view that
//! writes every scalar as text (XML): `8` for "an integer ≥ 1 or `auto`"
//! where the host held text. One question, asked twice: the save proposes
//! the view's reading where the schema prefers it ([`keep`]), and its last
//! check (`read_back`) asks the same question of the text the host reads
//! back, to tell a documented spelling from a value read otherwise.

use super::super::applies::{self, Applies, Entry};
use super::super::language::{SchemaPathSegment, pointer_segments};
use serde_json::Value;

/// Puts back each of `retyped` (a pointer into `value` and the view's
/// reading of the value there, which the host's type changed) where the
/// schema [`prefers`] the reading.
pub(super) fn keep(value: &mut Value, retyped: Vec<(String, Value)>, schema: &Value) {
    for (pointer, read) in retyped {
        if prefers(value, &pointer, &read, schema)
            && let Some(slot) = value.pointer_mut(&pointer)
        {
            *slot = read;
        }
    }
}

/// Whether `schema` prefers `reading` at `pointer` to the value `document`
/// holds there: it takes the reading and refuses the value, or takes both
/// and lists the reading (`enum: ["true"]`) and not the value.
///
/// The schemas judging the value are those the path leads to and, above
/// it, every value whose schemas judge it together with its siblings
/// (options, `contains`, see [`Here::judges_children_together`]). The
/// outermost that tells the two readings apart decides: what it judges
/// holds what the nearer ones do, and a nearer schema may be one of its
/// options only. A part of the schemas at a level refusing both (the
/// document fails there whichever is saved) tells nothing.
///
/// [`Here::judges_children_together`]: applies::Here::judges_children_together
pub(super) fn prefers(document: &Value, pointer: &str, reading: &Value, schema: &Value) -> bool {
    let Some(held) = document.pointer(pointer) else {
        return false;
    };
    let path = path_in(document, pointer);
    let whole = || Some(document);
    let applies = Applies::for_types(schema, false, &whole);
    // Each judging value above, outermost first, as the pointer to it and
    // what applies there.
    let above: Vec<(&str, Entry<'_>)> = (0..path.len())
        .filter_map(|depth| {
            let here = applies.at(&path[..depth]);
            here.judges_children_together().then(|| {
                (
                    parent_pointer(pointer, path.len() - depth),
                    here.entry().clone(),
                )
            })
        })
        .collect();
    // The value at `above` with `reading` at the pointer.
    let with_reading = |above: &str| {
        let relative = pointer.strip_prefix(above)?;
        let mut value = document.pointer(above)?.clone();
        *value.pointer_mut(relative)? = reading.clone();
        Some(value)
    };
    let decided = above.iter().find_map(|(at, entry)| {
        let as_held = document.pointer(at)?;
        let told = entry.refusing(as_held, &with_reading(at)?, schema);
        match (told.first, told.second) {
            (true, false) => Some(true),
            (_, true) => Some(false),
            (false, false) => None,
        }
    });
    decided.unwrap_or_else(|| {
        let here = applies.at(&path);
        match (here.accepts(held), here.accepts(reading)) {
            (Some(false), Some(true)) => true,
            (Some(true), Some(true)) => {
                let listed: Vec<&Value> = here
                    .branches()
                    .into_iter()
                    .flat_map(applies::listed)
                    .collect();
                listed.contains(&reading) && !listed.contains(&held)
            }
            _ => false,
        }
    })
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
