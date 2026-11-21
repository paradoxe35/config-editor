//! Whether some list meets the schema where a fill wrote an empty one,
//! judged without the editor's own reading of the schema: a bounded
//! search over short lists of values from a small pool, each put in the
//! document in place of the empty list, checked with the reference
//! validator, and written in the language and read back as itself.
//!
//! The pool holds the witness's own list there (a valid document holds
//! the key validly), the values the schema names (`enum`, `const`,
//! `default`, `examples`), the witness's other values and plain values of
//! each kind. The lengths tried reach a bound read off the schema's text:
//! the largest `minItems`, plus every `contains`'s `minContains`, plus the
//! longest `prefixItems` (an item a tuple position refuses goes after it).
//! A bound past [`LONGEST`] leaves the case undecided.

use serde_json::{Value, json};
use std::collections::HashSet;

use super::reference::{Both, same, walk};
use crate::editor::Language;

/// What the search found at a place holding an empty list.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Search {
    /// A list meeting the schema there that the language writes.
    Found,
    /// None, every list up to the schema's length bound tried.
    None,
    /// None up to [`LONGEST`] items, the bound past it.
    Undecided,
}

/// Longest list tried.
const LONGEST: usize = 4;

/// Most values in the pool.
const POOL: usize = 12;

/// What a search for a list at `at` in `read` (the document a fill or a
/// suggestion wrote, of `schema`) finds, where `read` holds an empty list
/// there; `None` where it does not. `witness` is a document holding the
/// key validly.
pub(super) fn search(
    both: &Both,
    language: Language,
    (schema, witness): (&Value, &Value),
    read: &Value,
    at: &str,
) -> Option<Search> {
    if read.pointer(at)? != &json!([]) {
        return None;
    }
    let errors = |document: &Value| -> HashSet<(String, String)> {
        both.reference
            .iter_errors(document)
            .map(|error| {
                (
                    error.instance_path().as_str().to_string(),
                    error.kind().keyword().to_string(),
                )
            })
            .collect()
    };
    // A list meets the schema when no problem is left at it, inside it or
    // above it (an option holding it) but those the document had elsewhere.
    let judged = |path: &str| {
        path == at || path.starts_with(&format!("{at}/")) || at.starts_with(&format!("{path}/"))
    };
    let before: HashSet<(String, String)> = errors(read)
        .into_iter()
        .filter(|(path, _)| !judged(path))
        .collect();
    let meets = |list: &Value| {
        let mut document = read.clone();
        *document.pointer_mut(at).expect("the list is there") = list.clone();
        errors(&document).is_subset(&before)
            && both
                .written(language, &document)
                .and_then(|text| both.read(language, &text))
                .and_then(|again| again.pointer(at).cloned())
                .is_some_and(|again| same(&again, list))
    };
    let own = witness.pointer(at).filter(|list| list.is_array());
    if own.is_some_and(meets) {
        return Some(Search::Found);
    }
    let pool = pool(schema, witness, own);
    let bound = length_bound(schema);
    let longest = bound.min(LONGEST);
    for length in 1..=longest {
        let count = pool
            .len()
            .saturating_pow(u32::try_from(length).unwrap_or(u32::MAX));
        for mut index in 0..count {
            let mut items = Vec::with_capacity(length);
            for _ in 0..length {
                items.push(pool[index % pool.len()].clone());
                index /= pool.len();
            }
            if meets(&Value::Array(items)) {
                return Some(Search::Found);
            }
        }
    }
    Some(if bound > LONGEST {
        Search::Undecided
    } else {
        Search::None
    })
}

/// What the search finds for the first empty list at or inside `at` in
/// `read` that it finds no list for ([`Search::None`] or
/// [`Search::Undecided`]): a problem there (an option refusing what holds
/// it) may be that list's. `None` where every empty list there can be met.
pub(super) fn unmet(
    both: &Both,
    language: Language,
    documents: (&Value, &Value),
    read: &Value,
    at: &str,
) -> Option<Search> {
    let mut empty = Vec::new();
    let mut next = vec![at.to_string()];
    while let Some(pointer) = next.pop() {
        match read.pointer(&pointer) {
            Some(Value::Array(items)) if items.is_empty() => empty.push(pointer),
            Some(Value::Array(items)) => {
                next.extend((0..items.len()).map(|index| format!("{pointer}/{index}")));
            }
            Some(Value::Object(members)) => next.extend(
                members
                    .keys()
                    .map(|key| format!("{pointer}/{}", crate::editor::language::pointer_key(key))),
            ),
            _ => {}
        }
    }
    empty
        .iter()
        .filter_map(|pointer| search(both, language, documents, read, pointer))
        .find(|found| *found != Search::Found)
}

/// The values lists are built of: the items of the witness's own list,
/// the values the schema names, the witness's other values, then plain
/// values of each kind; each once, at most [`POOL`].
fn pool(schema: &Value, witness: &Value, own: Option<&Value>) -> Vec<Value> {
    let mut found: Vec<Value> = own.and_then(Value::as_array).cloned().unwrap_or_default();
    walk(schema, &mut |node| {
        let Some(members) = node.as_object() else {
            return;
        };
        for keyword in ["enum", "examples"] {
            if let Some(Value::Array(values)) = members.get(keyword) {
                found.extend(values.iter().cloned());
            }
        }
        for keyword in ["const", "default"] {
            found.extend(members.get(keyword).cloned());
        }
    });
    walk(witness, &mut |node| {
        if !node.is_array() && !node.is_object() {
            found.push(node.clone());
        }
    });
    let plain = [
        json!(0),
        json!(""),
        Value::Null,
        json!([]),
        json!({}),
        json!(1),
        json!("a"),
        json!(true),
    ];
    // The plain values come first among what the pool keeps of the rest,
    // so each kind is tried whatever the schema names.
    let mut pool: Vec<Value> = Vec::new();
    let named = found.len().min(POOL - plain.len());
    for value in found.into_iter().take(named).chain(plain) {
        if pool.len() < POOL && !pool.contains(&value) {
            pool.push(value);
        }
    }
    pool
}

/// The longest list worth trying for `schema`, read off its text: the
/// largest `minItems`, plus each `contains`'s `minContains` (one without
/// it), plus the longest `prefixItems`.
fn length_bound(schema: &Value) -> usize {
    let mut least = 0;
    let mut contained = 0;
    let mut tuple = 0;
    let size = |value: Option<&Value>| {
        value
            .and_then(Value::as_u64)
            .map_or(0, |size| usize::try_from(size).unwrap_or(usize::MAX))
    };
    walk(schema, &mut |node| {
        let Some(members) = node.as_object() else {
            return;
        };
        least = least.max(size(members.get("minItems")));
        if members.contains_key("contains") {
            let asked = members
                .get("minContains")
                .map_or(1, |value| size(Some(value)));
            contained = usize::saturating_add(contained, asked);
        }
        for keyword in ["prefixItems", "items"] {
            if let Some(Value::Array(positions)) = members.get(keyword) {
                tuple = usize::max(tuple, positions.len());
            }
        }
    });
    least.saturating_add(contained).saturating_add(tuple)
}

#[cfg(test)]
mod tests {
    use super::super::reference::compile;
    use super::*;

    /// The search finds a list where one meets the schema, none where
    /// none does, and leaves undecided what its bound cannot reach.
    #[test]
    fn the_search_tells_a_list_that_can_be_met_from_one_that_cannot() {
        let found = |language: Language, list: Value| {
            let schema = json!({"properties": {"c": {"properties": {"l": list}}}});
            let both = compile(&schema).unwrap();
            search(
                &both,
                language,
                (&schema, &json!({})),
                &json!({"c": {"l": []}}),
                "/c/l",
            )
        };
        let none = [
            json!({"type": "array", "minItems": 1, "items": false}),
            json!({"minItems": 1, "contains": {"type": "integer"}, "items": false}),
            json!({"allOf": [{"minItems": 1}], "items": false}),
            json!({"minItems": 2, "maxItems": 1}),
            json!({"minItems": 2, "uniqueItems": true, "items": {"enum": ["a"]}}),
            json!({"contains": {"enum": []}}),
            json!({"anyOf": [{"contains": {"const": "8"}, "items": {"enum": [null]}}]}),
        ];
        for list in none {
            assert_eq!(
                found(Language::YAML, list.clone()),
                Some(Search::None),
                "{list}"
            );
        }
        let some = [
            json!({"minItems": 2, "prefixItems": [{"minimum": 0}]}),
            json!({"contains": {"type": "string"}}),
            json!({"minItems": 1, "prefixItems": [{"type": "integer"}], "items": false}),
            json!({"type": "array", "allOf": [{"minItems": 1, "contains": {"type": "integer"}}]}),
            json!({"minItems": 1, "prefixItems": [{"type": "array"}], "contains": {"type": "integer"}}),
            json!({"minItems": 1, "items": {"enum": ["x", "y"]}}),
        ];
        for list in some {
            assert_eq!(
                found(Language::YAML, list.clone()),
                Some(Search::Found),
                "{list}"
            );
        }
        // A first item only `null` meets: none TOML writes, one YAML does.
        let null_first = json!({"minItems": 1, "prefixItems": [{"type": "null"}],
            "contains": {"type": "integer"}});
        assert_eq!(
            found(Language::TOML, null_first.clone()),
            Some(Search::None)
        );
        assert_eq!(found(Language::YAML, null_first), Some(Search::Found));
        // Six items of no value in the pool: past the bound.
        let long = json!({"minItems": 6, "items": {"const": "z"}, "maxItems": 5});
        assert_eq!(found(Language::YAML, long), Some(Search::Undecided));
    }
}
