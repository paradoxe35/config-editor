//! Keys a `not` keeps apart (`not: {required: [git, path]}`: "git" and
//! "path" can't be used together; `dependentSchemas: {a: {not: {required:
//! [b]}}}`: "a" and "b" can't): its problem says so in those words, and
//! once all but one of a set are written the last is not offered.

use super::*;

/// The schema `$ref` points to, when `schema` is only that reference.
fn through_ref<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    match schema.get("$ref").and_then(Value::as_str) {
        Some(reference) if schema.as_object()?.len() == 1 => ref_target(reference, root),
        _ => Some(schema),
    }
}

/// The keys `schema` (through a `$ref`) only requires: a `required` list
/// with nothing else that limits a value (a `type` of object, a title or
/// a description aside, and `properties` only naming required keys, as
/// `{git: true}` does).
fn only_required<'a>(schema: &'a Value, root: &'a Value) -> Option<Vec<&'a str>> {
    let object = through_ref(schema, root)?.as_object()?;
    let keys: Vec<&str> = object
        .get("required")?
        .as_array()?
        .iter()
        .map(Value::as_str)
        .collect::<Option<_>>()?;
    let names_required = |properties: &Value| {
        properties.as_object().is_some_and(|properties| {
            properties
                .iter()
                .all(|(key, property)| names_only(property) && keys.contains(&key.as_str()))
        })
    };
    let only = object
        .iter()
        .all(|(keyword, value)| match keyword.as_str() {
            "required" | "title" | "description" | "$comment" => true,
            "type" => value == "object",
            "properties" => names_required(value),
            _ => false,
        });
    (only && !keys.is_empty()).then_some(keys)
}

/// The sets of keys a `not` schema forbids together: one `required` list
/// of two or more keys (see [`only_required`]), or an `anyOf` of them,
/// each set refused (`not: {anyOf: [{required: [branch, tag]}, …]}`).
pub(crate) fn keys_kept_apart<'a>(schema: &'a Value, root: &'a Value) -> Option<Vec<Vec<&'a str>>> {
    let pairs = |keys: Vec<&'a str>| (keys.len() >= 2).then_some(keys);
    if let Some(keys) = only_required(schema, root) {
        return pairs(keys).map(|keys| vec![keys]);
    }
    let object = through_ref(schema, root)?.as_object()?;
    let only_any_of = object.keys().all(|keyword| {
        matches!(
            keyword.as_str(),
            "anyOf" | "title" | "description" | "$comment"
        )
    });
    if !only_any_of {
        return None;
    }
    object
        .get("anyOf")?
        .as_array()?
        .iter()
        .map(|option| only_required(option, root).and_then(pairs))
        .collect()
}

/// The keys a `not` under a key's dependent schema (`dependentSchemas`,
/// or draft 7's `dependencies`, at `trigger`) forbids beside that key:
/// the key and each set it keeps apart from it.
pub(crate) fn dependent_keys_kept_apart<'a>(
    trigger: &'a str,
    not: &'a Value,
    root: &'a Value,
) -> Option<Vec<Vec<&'a str>>> {
    let with_trigger = |keys: Vec<&'a str>| {
        let mut set = vec![trigger];
        set.extend(keys.into_iter().filter(|key| *key != trigger));
        (set.len() >= 2).then_some(set)
    };
    match only_required(not, root) {
        Some(keys) => with_trigger(keys).map(|set| vec![set]),
        None => keys_kept_apart(not, root),
    }
}

/// Every set of keys `branch` keeps apart: by its `not`, and by the `not`
/// of each of its keys' dependent schemas.
fn sets_of<'a>(branch: &'a Value, root: &'a Value) -> Vec<Vec<&'a str>> {
    let mut sets: Vec<Vec<&str>> = branch
        .get("not")
        .and_then(|not| keys_kept_apart(not, root))
        .unwrap_or_default();
    for keyword in ["dependentSchemas", "dependencies"] {
        let Some(dependents) = branch.get(keyword).and_then(Value::as_object) else {
            continue;
        };
        for (trigger, dependent) in dependents {
            if let Some(not) = dependent.get("not") {
                sets.extend(dependent_keys_kept_apart(trigger, not, root).unwrap_or_default());
            }
        }
    }
    sets
}

/// Whether one of `branches` (an object's, see `branches_for`) keeps
/// `key` apart from the `written` keys: some set holding it has every
/// other key written.
pub(crate) fn excluded_by_not(
    key: &str,
    branches: &[&Value],
    written: &HashSet<String>,
    root: &Value,
) -> bool {
    branches.iter().any(|branch| {
        sets_of(branch, root).iter().any(|set| {
            set.contains(&key)
                && set
                    .iter()
                    .all(|other| *other == key || written.contains(*other))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_lists_of_required_keys_keep_keys_apart() {
        let root = json!({"definitions": {"pair": {"required": ["a", "b"]}}});
        let kept = |schema: Value| {
            keys_kept_apart(&schema, &root)
                .map(|sets| sets.iter().map(|keys| keys.join(" ")).collect::<Vec<_>>())
        };
        let sets = |sets: &[&str]| Some(sets.iter().map(|set| set.to_string()).collect::<Vec<_>>());
        assert_eq!(kept(json!({"required": ["a", "b"]})), sets(&["a b"]));
        assert_eq!(
            kept(json!({"type": "object", "required": ["a", "b", "c"]})),
            sets(&["a b c"])
        );
        assert_eq!(kept(json!({"$ref": "#/definitions/pair"})), sets(&["a b"]));
        // Properties only naming the keys required, as Cargo's schema writes it.
        assert_eq!(
            kept(json!({"properties": {"a": true, "b": {}}, "required": ["a", "b"]})),
            sets(&["a b"])
        );
        // Each set of an `anyOf`.
        assert_eq!(
            kept(json!({"anyOf": [
                {"properties": {"a": true, "b": true}, "required": ["a", "b"]},
                {"required": ["b", "c"]}
            ]})),
            sets(&["a b", "b c"])
        );
        assert_eq!(kept(json!({"required": ["a"]})), None);
        assert_eq!(
            kept(json!({"anyOf": [{"required": ["a"]}, {"required": ["b"]}]})),
            None
        );
        assert_eq!(
            kept(json!({"required": ["a", "b"], "properties": {"a": {"const": 1}}})),
            None
        );
        assert_eq!(
            kept(json!({"required": ["a", "b"], "properties": {"c": true}})),
            None
        );
        assert_eq!(kept(json!({"type": "string"})), None);
    }

    #[test]
    fn a_dependent_schemas_not_keeps_its_key_apart_from_the_others() {
        let root = json!({});
        let written = |keys: &[&str]| {
            keys.iter()
                .map(|key| key.to_string())
                .collect::<HashSet<_>>()
        };
        for keyword in ["dependentSchemas", "dependencies"] {
            let branch = json!({keyword: {"a": {"not": {"required": ["b"]}}}});
            assert!(
                excluded_by_not("b", &[&branch], &written(&["a"]), &root),
                "{keyword}"
            );
            assert!(
                excluded_by_not("a", &[&branch], &written(&["b"]), &root),
                "{keyword}"
            );
            assert!(
                !excluded_by_not("c", &[&branch], &written(&["a"]), &root),
                "{keyword}"
            );
        }
        let any_of =
            json!({"not": {"anyOf": [{"required": ["x", "y"]}, {"required": ["x", "z"]}]}});
        assert!(excluded_by_not("y", &[&any_of], &written(&["x"]), &root));
        assert!(!excluded_by_not("y", &[&any_of], &written(&["z"]), &root));
    }

    #[test]
    fn a_key_is_excluded_once_the_others_it_is_kept_apart_from_are_written() {
        let root = json!({});
        let branch = json!({"not": {"required": ["git", "path", "x"]}});
        let written = |keys: &[&str]| {
            keys.iter()
                .map(|key| key.to_string())
                .collect::<HashSet<_>>()
        };
        assert!(excluded_by_not(
            "git",
            &[&branch],
            &written(&["path", "x"]),
            &root
        ));
        assert!(!excluded_by_not(
            "git",
            &[&branch],
            &written(&["path"]),
            &root
        ));
        assert!(!excluded_by_not(
            "version",
            &[&branch],
            &written(&["path", "x"]),
            &root
        ));
    }
}
