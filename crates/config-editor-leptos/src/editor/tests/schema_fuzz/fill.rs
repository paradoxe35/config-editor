//! Accepting a key at the root writes its required children and least
//! lists so that what it filled in validates: no value it wrote is
//! refused, no required child or list item is missing (only the empty
//! slots left to type may be wrong), and each value it wrote gets a Tab
//! stop. Only keys a witness document holds validly are judged (their
//! schema can be met), in schemas whose defaults and examples fit.

use crate::editor::language::pointer_key;
use serde_json::Value;

use super::documents::{Shape, document};
use super::random::Rng;
use super::reference::{Both, annotations_fit, compile, holds_empty};
use super::runner::{Case, Property, Verdict, failure, language, run};
use super::schemas::{Draw, schema};
use crate::editor::Language;
use crate::editor::completion::{apply_completion, completions, stop_ranges};

/// Most levels a fill writes (`completion::fill::MAX_FILL_DEPTH`): a
/// child missing this deep is the fill's documented bound.
const FILL_DEPTH: usize = 8;

pub(super) fn draw(rng: &mut Rng, index: usize) -> Case {
    let draw = Draw {
        marks: false,
        docs: rng.chance(70),
        depth: 2 + rng.below(2),
    };
    let schema = schema(rng, draw);
    // A witness: the keys it holds validly can be met.
    let shape = Shape {
        wrong: 0,
        secrets: false,
        every_key: true,
    };
    let doc = document(rng, &schema, shape);
    Case {
        language: language(index),
        pick: 0,
        schema,
        doc,
        other: Value::Null,
    }
}

/// Where a new key is typed at the root, `|` the caret.
pub(super) fn root(language: Language) -> &'static str {
    match language.label() {
        "JSON" => "{\n  |\n}\n",
        "XML" => "<|\n",
        _ => "|\n",
    }
}

pub(super) fn check(case: &Case) -> Verdict {
    let Some(both) = compile(&case.schema) else {
        return Verdict::Skipped;
    };
    if !annotations_fit(&case.schema) {
        return Verdict::Skipped;
    }
    let language = case.language;
    let marked = root(language);
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let offered = completions(
        &source,
        at,
        true,
        Some(&both.editor.bundle),
        true,
        language.behavior(),
    );
    let mut checked = false;
    let keys = offered
        .iter()
        .filter(|option| !option.is_value && both.met(&case.doc, &option.label));
    for option in keys {
        let (text, _, _) = apply_completion(&source, option);
        let stops = stop_ranges(&option.insert, option.cursor_offset, &option.fills);
        let seen = |what: &str| {
            format!(
                "{language}, accepting {}:\n{text}fills {:?}\n{what}",
                option.label, option.fills
            )
        };
        if stops.len() != option.fills.len() {
            return Verdict::Fails(failure(
                "a filled value gets no Tab stop",
                seen(&format!("stops {stops:?}")),
            ));
        }
        let Some(read) = both.read(language, &text) else {
            // Empty slots a language does not read (JSON `"a": `).
            continue;
        };
        let pointer = format!("/{}", pointer_key(&option.label));
        if read.pointer(&pointer).is_none() {
            return Verdict::Fails(failure(
                "an accepted key does not read",
                seen(&format!("reads {read}")),
            ));
        }
        checked = true;
        for error in both.reference.iter_errors(&read) {
            let at = error.instance_path().as_str().to_string();
            if at != pointer && !at.starts_with(&format!("{pointer}/")) {
                continue;
            }
            if at.matches('/').count() > FILL_DEPTH {
                continue;
            }
            if language == Language::XML && in_a_list_in_a_list(&read, &at) {
                continue;
            }
            if only_unwritable_left_out(
                &both,
                language,
                &option.label,
                &read,
                (&at, error.kind().keyword()),
            ) {
                continue;
            }
            let keyword = error.kind().keyword();
            let class = match keyword {
                "required" => "a fill leaves out a required child",
                "minItems" | "minContains" | "contains"
                    if both.no_list_meets(language, &read, &at) =>
                {
                    "a fill writes an empty list where no list meets every schema"
                }
                "minItems" | "minContains" | "contains" => "a fill writes too few list items",
                _ if holds_empty(error.instance()) => continue,
                _ => "a fill writes a value its schema refuses",
            };
            return Verdict::Fails(failure(
                format!("{class} ({keyword})"),
                seen(&format!("reads {read}\n{at}: {error}")),
            ));
        }
    }
    if checked {
        Verdict::Holds
    } else {
        Verdict::Skipped
    }
}

/// Whether the problem at `at` (a `keyword` one) goes once the values
/// `language` cannot write are put back where YAML's fill of the key
/// `label` has them. TOML writes no `null` and XML no `null` nor empty
/// list (documented): a fill leaves such a value out (a required child, a
/// list's only kind of item), which only its schema refuses.
fn only_unwritable_left_out(
    both: &Both,
    language: Language,
    label: &str,
    read: &Value,
    (at, keyword): (&str, &str),
) -> bool {
    let unwritable = |value: &Value| match language.label() {
        "TOML" => value.is_null(),
        "XML" => value.is_null() || value.as_array().is_some_and(Vec::is_empty),
        _ => false,
    };
    if !matches!(language.label(), "TOML" | "XML") {
        return false;
    }
    let source = root(Language::YAML).replacen('|', "", 1);
    let offered = completions(
        &source,
        0,
        true,
        Some(&both.editor.bundle),
        true,
        Language::YAML.behavior(),
    );
    let Some(yaml) = offered
        .iter()
        .find(|option| !option.is_value && option.label == label)
        .and_then(|option| both.read(Language::YAML, &apply_completion(&source, option).0))
    else {
        return false;
    };
    let restored = put_back(read, &yaml, &unwritable);
    restored != *read
        && !both
            .reference
            .iter_errors(&restored)
            .any(|error| error.instance_path().as_str() == at && error.kind().keyword() == keyword)
}

/// `read` with each value of `full` that `unwritable` names and `read`
/// lacks put back: a member, or the items of a list `read` holds without
/// them.
fn put_back(read: &Value, full: &Value, unwritable: &dyn Fn(&Value) -> bool) -> Value {
    match (read, full) {
        (Value::Object(read), Value::Object(full)) => {
            let mut restored = read.clone();
            for (key, value) in full {
                match read.get(key) {
                    Some(held) => {
                        restored.insert(key.clone(), put_back(held, value, unwritable));
                    }
                    None if unwritable(value) => {
                        restored.insert(key.clone(), value.clone());
                    }
                    None => {}
                }
            }
            Value::Object(restored)
        }
        (Value::Array(items), Value::Array(full)) => {
            let kept: Vec<&Value> = full.iter().filter(|item| !unwritable(item)).collect();
            if kept.len() < full.len() && kept.iter().copied().eq(items.iter()) {
                Value::Array(full.clone())
            } else if items.len() == full.len() {
                Value::Array(
                    items
                        .iter()
                        .zip(full)
                        .map(|(item, full)| put_back(item, full, unwritable))
                        .collect(),
                )
            } else {
                Value::Array(items.clone())
            }
        }
        _ => read.clone(),
    }
}

/// Whether the place `at` in `read` is a list that is an item of a list,
/// or inside one. XML writes no list in a list (documented, see
/// `xml_cannot_hold`): an empty element where the schema wants one (the
/// least list of a list of lists, written `<a></a>`) reads as a list of
/// one inner list, which is not what the fill wrote, so its items are
/// not judged.
fn in_a_list_in_a_list(read: &Value, at: &str) -> bool {
    let mut value = read;
    for segment in at.split('/').skip(1) {
        let next = match value {
            Value::Array(items) => segment
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get(index)),
            Value::Object(map) => map.get(&segment.replace("~1", "/").replace("~0", "~")),
            _ => None,
        };
        let Some(next) = next else {
            return false;
        };
        if value.is_array() && next.is_array() {
            return true;
        }
        value = next;
    }
    false
}

#[test]
fn accepted_keys_fill_what_their_schema_requires_with_a_stop_per_value() {
    run(&Property {
        name: "fill",
        check_path: "super::fill::check",
        cases: 250,
        shrink_budget: 1500,
        draw,
        check,
    });
}

/// Only a list that is an item of a list, or what is inside one, is the
/// XML spelling the check leaves alone.
#[test]
fn only_a_list_in_a_list_is_left_alone() {
    let read = serde_json::json!({"a": [[{}], {"b": [1]}], "c": [1, 2]});
    for at in ["/a/0", "/a/0/0"] {
        assert!(in_a_list_in_a_list(&read, at), "{at}");
    }
    for at in ["", "/a", "/a/1", "/a/1/b", "/a/1/b/0", "/c", "/c/0", "/x/0"] {
        assert!(!in_a_list_in_a_list(&read, at), "{at}");
    }
}

/// The one list allowance asks the fill's list plan at the list's place:
/// an empty list there is excused only where no list meets every schema
/// at it that the language writes, whatever splits the rules.
#[test]
fn an_empty_list_is_excused_only_where_the_list_plan_finds_none() {
    use serde_json::json;
    let excused = |language: Language, list: Value, read: Value| {
        let schema = json!({"properties": {"c": {"properties": {"l": list}}}});
        let both = compile(&schema).unwrap();
        both.no_list_meets(language, &json!({"c": {"l": read}}), "/c/l")
    };
    let empty = || json!([]);
    let none = [
        json!({"type": "array", "minItems": 1, "items": false}),
        json!({"minItems": 1, "contains": {"type": "integer"}, "items": false}),
        json!({"allOf": [{"minItems": 1}], "items": false}),
        json!({"minItems": 2, "maxItems": 1}),
        json!({"minItems": 2, "uniqueItems": true, "items": {"enum": ["a"]}}),
        json!({"contains": {"type": "integer"}, "minContains": 2, "maxContains": 1}),
    ];
    for list in none {
        assert!(excused(Language::YAML, list.clone(), empty()), "{list}");
        assert!(!excused(Language::YAML, list.clone(), json!([0])), "{list}");
    }
    let some = [
        json!({"minItems": 2, "prefixItems": [{"minimum": 0}]}),
        json!({"minItems": 1, "prefixItems": [{}], "items": {"type": "string"}}),
        json!({"contains": {"type": "string"}}),
        json!({"minItems": 1, "prefixItems": [{"type": "integer"}], "items": false}),
        json!({"type": "array", "allOf": [{"minItems": 1, "contains": {"type": "integer"}}]}),
        json!({"minItems": 1, "prefixItems": [{"type": "array"}], "contains": {"type": "integer"}}),
    ];
    for list in some {
        assert!(!excused(Language::YAML, list.clone(), empty()), "{list}");
    }
    // A first item only `null` meets: no list TOML writes, one YAML does.
    let null_first = json!({"minItems": 1, "prefixItems": [{"type": "null"}],
        "contains": {"type": "integer"}});
    assert!(excused(Language::TOML, null_first.clone(), empty()));
    assert!(!excused(Language::YAML, null_first, empty()));
}

/// Only the values a language cannot write are put back, where the read
/// value lacks them.
#[test]
fn only_unwritable_values_are_put_back() {
    use serde_json::json;
    let null = |value: &Value| value.is_null();
    let full = json!({"a": null, "b": [null, 1], "c": {"d": null, "e": 2}, "f": 3});
    assert_eq!(
        put_back(&json!({"b": [1], "c": {}}), &full, &null),
        json!({"a": null, "b": [null, 1], "c": {"d": null}})
    );
    // A value the read one holds otherwise, or a list with other items,
    // stays as read.
    assert_eq!(
        put_back(&json!({"a": 0, "b": [2]}), &full, &null),
        json!({"a": 0, "b": [2]})
    );
}
