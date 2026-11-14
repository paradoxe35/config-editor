//! Keys a `not` keeps apart (`not: {required: [git, path]}`: "git" and
//! "path" can't be used together): its problem says so in those words,
//! and once all but one of them are written the last is not offered.

use super::*;

/// The keys a `not` schema forbids together: `schema` (through a `$ref`)
/// is only a `required` list of two or more keys, with nothing else that
/// limits a value (a `type` of object, a title or description aside).
pub(crate) fn keys_kept_apart<'a>(schema: &'a Value, root: &'a Value) -> Option<Vec<&'a str>> {
    let schema = match schema.get("$ref").and_then(Value::as_str) {
        Some(reference) if schema.as_object()?.len() == 1 => ref_target(reference, root)?,
        _ => schema,
    };
    let object = schema.as_object()?;
    let only_required = object
        .iter()
        .all(|(keyword, value)| match keyword.as_str() {
            "required" | "title" | "description" | "$comment" => true,
            "type" => value == "object",
            _ => false,
        });
    let keys: Vec<&str> = object
        .get("required")?
        .as_array()?
        .iter()
        .map(Value::as_str)
        .collect::<Option<_>>()?;
    (only_required && keys.len() >= 2).then_some(keys)
}

/// Whether one of `branches` (an object's, see `branches_for`) has a `not`
/// that writing `key` beside the `written` keys would break: every other
/// key it keeps apart from `key` is written.
pub(crate) fn excluded_by_not(
    key: &str,
    branches: &[&Value],
    written: &HashSet<String>,
    root: &Value,
) -> bool {
    branches.iter().any(|branch| {
        branch
            .get("not")
            .and_then(|not| keys_kept_apart(not, root))
            .is_some_and(|keys| {
                keys.contains(&key)
                    && keys
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
    fn only_a_list_of_required_keys_keeps_keys_apart() {
        let root = json!({"definitions": {"pair": {"required": ["a", "b"]}}});
        let kept = |schema: Value| {
            keys_kept_apart(&schema, &root)
                .map(|keys| keys.iter().map(|key| key.to_string()).collect::<Vec<_>>())
        };
        assert_eq!(
            kept(json!({"required": ["a", "b"]})),
            Some(vec!["a".into(), "b".into()])
        );
        assert_eq!(
            kept(json!({"type": "object", "required": ["a", "b", "c"]})).map(|keys| keys.len()),
            Some(3)
        );
        assert_eq!(
            kept(json!({"$ref": "#/definitions/pair"})),
            Some(vec!["a".into(), "b".into()])
        );
        assert_eq!(kept(json!({"required": ["a"]})), None);
        assert_eq!(
            kept(json!({"required": ["a", "b"], "properties": {"a": {"const": 1}}})),
            None
        );
        assert_eq!(kept(json!({"type": "string"})), None);
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
