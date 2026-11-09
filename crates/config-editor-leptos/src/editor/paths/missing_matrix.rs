//! A path the document does not have is written nowhere: a missing parent
//! (`/svc_3/host` beside `svc_0..svc_2`), a missing list index (`/l/7` on a
//! 3-item list) and a missing leaf each answer `None` from every lookup
//! (`key_span`, `value_span`, `range_of`, `ranges_of`, the lock and problem
//! spans), in every language: never another field's place. The missing
//! paths are made from the parsed value of each document.

use serde_json::{Value, json};

use super::super::Language;
use super::ranges_matrix::cases;

fn child(at: &str, key: &str) -> String {
    format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"))
}

/// Paths `value` does not have, near the ones it has: a leaf under each
/// value, the next index past each list, and each path with one of its
/// steps renamed (a sibling key that is not there, an index past its list).
fn missing(value: &Value, at: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(members) => {
            out.push(child(at, "zz_missing"));
            for (key, member) in members {
                // The member's whole subtree under a key that is not there.
                let mut below = Vec::new();
                super::ranges_matrix::pointers(member, "", &mut below);
                let renamed = child(at, &format!("{key}_zz"));
                out.extend(below.iter().map(|rest| format!("{renamed}{rest}")));
                missing(member, &child(at, key), out);
            }
        }
        Value::Array(items) => {
            for past in [items.len(), items.len() + 4] {
                let index = past.to_string();
                out.push(child(at, &index));
                // The first item's subtree under an index that is not there.
                if let Some(first) = items.first() {
                    let mut below = Vec::new();
                    super::ranges_matrix::pointers(first, "", &mut below);
                    out.extend(
                        below
                            .iter()
                            .map(|rest| format!("{}{rest}", child(at, &index))),
                    );
                }
            }
            for (index, item) in items.iter().enumerate() {
                missing(item, &child(at, &index.to_string()), out);
            }
        }
        _ => out.push(child(at, "zz_missing")),
    }
}

/// Documents with numbered siblings (`svc_0`..`svc_2`) and short lists, in
/// each language, besides the range matrix's documents.
fn all_cases() -> Vec<(Language, String)> {
    let services = json!({
        "svc_0": {"host": "a", "port": 1},
        "svc_1": {"host": "b", "port": 2},
        "svc_2": {"host": "c", "port": 3},
        "l": ["x", "y", "z"],
        "nested": {"l": [{"host": "d"}, {"host": "e"}, {"host": "f"}]}
    });
    let mut out = cases();
    for &language in Language::ALL {
        let value = if language == Language::XML {
            json!({ "root": services })
        } else {
            services.clone()
        };
        if let Ok(text) = language.serialize(&value) {
            out.push((language, text));
        }
    }
    out
}

#[test]
fn a_path_the_document_does_not_have_is_written_nowhere() {
    let mut failures = Vec::new();
    for (language, text) in all_cases() {
        let Ok(value) = language.parse(&text) else {
            failures.push(format!("{language}: does not parse:\n{text}"));
            continue;
        };
        let mut paths = Vec::new();
        missing(&value, "", &mut paths);
        paths.retain(|path| value.pointer(path).is_none());
        let behavior = language.behavior();
        let many = language.ranges_of(&text, paths.iter().map(String::as_str));
        for (path, many) in paths.iter().zip(many) {
            let found = [
                (
                    "key_span",
                    behavior
                        .key_span(&text, path)
                        .map(|span| format!("{span:?}")),
                ),
                (
                    "value_span",
                    behavior
                        .value_span(&text, path)
                        .map(|span| format!("{span:?}")),
                ),
                (
                    "range_of",
                    language
                        .range_of(&text, path.as_str())
                        .map(|range| range.slice(&text)),
                ),
                ("ranges_of", many.map(|range| range.slice(&text))),
                (
                    "problem_span",
                    super::problem_span(behavior, &text, path, false, false)
                        .map(|span| format!("{span:?}")),
                ),
                (
                    "required problem_span",
                    super::problem_span(behavior, &text, path, true, false)
                        .map(|span| format!("{span:?}")),
                ),
                #[cfg(feature = "protect")]
                (
                    "key_range",
                    super::key_range(behavior, &text, path).map(|range| range.slice(&text)),
                ),
            ];
            for (lookup, found) in found {
                if let Some(found) = found {
                    failures.push(format!("{language} {path}: {lookup} gives {found:?}"));
                }
            }
        }
    }
    failures.sort();
    failures.dedup();
    let shown: Vec<String> = failures.iter().take(400).cloned().collect();
    assert!(
        failures.is_empty(),
        "{} failing case(s), first {}:\n{}",
        failures.len(),
        shown.len(),
        shown.join("\n")
    );
}
