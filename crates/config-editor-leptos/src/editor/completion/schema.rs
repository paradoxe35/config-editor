//! What one schema says of a value, read from the applicability model
//! (`applies`): types, defaults, the object and array branches, the values
//! it suggests (reading values to the shapes it expects is `data_reading`).

use super::*;

pub(super) fn schema_type_label(schema: &Value, root: &Value, fallback: &str) -> String {
    let mut labels = type_labels(schema, root);
    labels.sort();
    labels.dedup();
    if labels.is_empty() {
        fallback.to_string()
    } else {
        labels.join(" | ")
    }
}

/// Each type (or short `const`) `schema`'s branches allow, as a label
/// names it: `string`, `"static"`.
fn type_labels(schema: &Value, root: &Value) -> Vec<String> {
    resolve_branches(schema, root)
        .into_iter()
        .flat_map(|branch| {
            // A const is its one value (`"static"`), unless that is long.
            if branch.get("type").is_none()
                && let Some(value) = branch.get("const")
            {
                let written = value.to_string();
                return vec![if written.chars().count() <= CONST_LABEL_LIMIT {
                    written
                } else {
                    json_type_name(value).to_string()
                }];
            }
            // An enum is a constraint, not a type: show its values' type.
            if branch.get("type").is_none()
                && let Some(values) = branch.get("enum").and_then(Value::as_array)
            {
                return values
                    .iter()
                    .map(|value| json_type_name(value).to_string())
                    .collect();
            }
            match branch.get("type") {
                Some(Value::String(value)) => vec![value.clone()],
                Some(Value::Array(values)) => values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                _ => Vec::new(),
            }
        })
        .collect()
}

/// The longest `const` value a type label shows as written; a longer one
/// shows its type.
const CONST_LABEL_LIMIT: usize = 24;

/// The type labels of `schemas` (the same key in several `oneOf` options)
/// in one, each type once: `"manual" | "static"`, `integer | string |
/// boolean` for `integer | string` and `boolean | string`.
pub(super) fn type_label_of_all(schemas: &[&Value], root: &Value, fallback: &str) -> String {
    let mut labels: Vec<String> = Vec::new();
    for schema in schemas {
        let mut own = type_labels(schema, root);
        own.sort();
        for label in own {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
    }
    if labels.is_empty() {
        fallback.to_string()
    } else {
        labels.join(" | ")
    }
}

pub(crate) fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The `default` of a property schema, through `$ref` and `anyOf`/`oneOf`/`allOf`.
pub fn schema_default<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root)
        .into_iter()
        .find_map(|branch| branch.get("default"))
}

/// A valid value for a property the user has not filled yet: its default,
/// const, or first enum value, else an empty value of its first type (`""`,
/// `0`, `false`, `[]`, `{}`, `null`). Inserting it keeps the document valid.
pub fn placeholder(schema: &Value, root: &Value) -> Value {
    let branches = resolve_branches(schema, root);
    if let Some(value) = branches
        .iter()
        .find_map(|branch| branch.get("default").or_else(|| branch.get("const")))
    {
        return value.clone();
    }
    if let Some(first) = branches
        .iter()
        .find_map(|branch| branch.get("enum").and_then(Value::as_array)?.first())
    {
        return first.clone();
    }
    let types = branches.iter().flat_map(|branch| match branch.get("type") {
        Some(Value::String(name)) => vec![name.as_str()],
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    });
    for name in types {
        match name {
            "string" => return Value::String(String::new()),
            "integer" | "number" => return Value::from(0),
            "boolean" => return Value::Bool(false),
            "array" => return Value::Array(Vec::new()),
            "object" => return Value::Object(serde_json::Map::new()),
            _ => {}
        }
    }
    if branches
        .iter()
        .any(|branch| branch.get("properties").is_some() || branch.get("required").is_some())
    {
        return Value::Object(serde_json::Map::new());
    }
    Value::Null
}

/// Whether no value is valid for `schema`: `false`, or a schema whose
/// `not` admits every value (`{not: {}}`, `{not: true}`).
pub fn admits_nothing(schema: &Value) -> bool {
    match schema {
        Value::Bool(admits) => !admits,
        Value::Object(keywords) => keywords.get("not").is_some_and(|not| match not {
            Value::Bool(admits) => *admits,
            Value::Object(inner) => inner.is_empty(),
            _ => false,
        }),
        _ => false,
    }
}

/// The `const` of a property schema, through `$ref` and the combinators,
/// where the schema takes it (a `const` beside a `type` or a `not`
/// refusing it is no value of the field).
pub fn schema_const<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root)
        .into_iter()
        .find_map(|branch| branch.get("const"))
        .filter(|value| decides(schema, value, root) != Some(false))
}

/// The branch of a property schema that describes an object, if any,
/// where some object meets the schema ([`Here::may_hold`]).
pub fn object_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    branch_holding(schema, root, "object", describes_object)
}

/// The branch of a property schema that describes an array, if any,
/// where some array meets the schema ([`Here::may_hold`]).
pub fn array_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    branch_holding(schema, root, "array", describes_array)
}

/// The first branch of `schema` that `describes` a value of the kind
/// `name`, where some value of that kind meets the schema.
fn branch_holding<'a>(
    schema: &'a Value,
    root: &'a Value,
    name: &str,
    describes: fn(&Value) -> bool,
) -> Option<&'a Value> {
    let here = Here::of(schema, root);
    if !here.may_hold(name) {
        return None;
    }
    here.branches().into_iter().find(|branch| describes(branch))
}

/// Whether one schema (a branch) describes an object.
pub(super) fn describes_object(branch: &Value) -> bool {
    branch.get("properties").is_some()
        || branch.get("required").is_some()
        || branch.get("patternProperties").is_some()
        || branch.get("additionalProperties").is_some()
        || branch.get("unevaluatedProperties").is_some()
        || type_includes(branch, "object")
}

/// Whether one schema (a branch) describes an array.
pub(crate) fn describes_array(branch: &Value) -> bool {
    branch.get("items").is_some()
        || branch.get("prefixItems").is_some()
        || type_includes(branch, "array")
}

/// The branches of `schema`: itself and every schema applying to the same
/// value (through `$ref`, `anyOf`/`oneOf`/`allOf`, `if`/`then`/`else` and
/// `dependentSchemas`), the value unknown: what the applicability model
/// ([`Here::of`]) says of a value of `schema` alone.
pub(crate) fn resolve_branches<'a>(schema: &'a Value, root: &'a Value) -> Vec<&'a Value> {
    Here::of(schema, root).branches()
}

/// The values a branch suggests. `open` adds free-form ones (examples and the
/// default), which are only hints: while the user types their own value they
/// would replace it on Enter, so they are left out then.
pub(super) fn schema_value_candidates(branch: &Value, open: bool) -> Vec<Value> {
    let mut values = branch
        .get("enum")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(value) = branch.get("const") {
        values.push(value.clone());
    }
    if open {
        match branch.get("examples") {
            Some(Value::Array(examples)) => values.extend(examples.iter().cloned()),
            Some(example) => values.push(example.clone()),
            None => {}
        }
        if let Some(example) = branch.get("example") {
            values.push(example.clone());
        }
        if let Some(default) = branch.get("default") {
            values.push(default.clone());
        }
    }
    if type_includes(branch, "boolean") {
        values.extend([Value::Bool(true), Value::Bool(false)]);
    }
    if type_includes(branch, "null") {
        values.push(Value::Null);
    }
    values.retain(|value| !value.is_array() && !value.is_object());
    values
}

pub(crate) fn path_with_key(path: &[SchemaPathSegment], key: &str) -> Vec<SchemaPathSegment> {
    let mut path = path.to_vec();
    path.push(SchemaPathSegment::Key(key.to_string()));
    path
}

pub(super) fn value_label(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

/// `typed` without the quote it starts with (and the matching one ending
/// it): the query of a key or value typed in one of the language's quotes.
pub(crate) fn unquoted<'a>(behavior: &dyn ConfigLanguage, typed: &'a str) -> &'a str {
    let typed = typed.trim_start();
    let config = behavior.config();
    if let Some(quote) = typed
        .chars()
        .next()
        .filter(|first| config.quotes.iter().any(|quote| quote.starts_with(*first)))
    {
        let inner = &typed[quote.len_utf8()..];
        return inner.strip_suffix(quote).unwrap_or(inner);
    }
    typed
}

#[cfg(test)]
mod label_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_type_label_names_each_type_once_in_order() {
        let root = json!({});
        let first = json!({"anyOf": [{"type": "integer"}, {"type": "string"}]});
        let second = json!({"anyOf": [{"type": "boolean"}, {"type": "string"}]});
        let third = json!({"type": ["null", "integer"]});
        let listed = json!({"enum": ["a", 1, "b"]});
        assert_eq!(
            type_label_of_all(&[&first, &second], &root, "property"),
            "integer | string | boolean"
        );
        assert_eq!(
            type_label_of_all(&[&second, &third, &listed], &root, "property"),
            "boolean | string | integer | null"
        );
        assert_eq!(
            schema_type_label(&listed, &root, "property"),
            "integer | string"
        );
    }
}
