//! Walking a JSON Schema: types, defaults, branches, the schema at a
//! document path, and reading values to the shapes it expects.

use super::*;

pub(super) fn schema_type_label(schema: &Value, root: &Value, fallback: &str) -> String {
    let mut labels: Vec<String> = resolve_branches(schema, root, 0)
        .into_iter()
        .filter_map(|branch| {
            // A const is its one value (`"static"`), unless that is long.
            if branch.get("type").is_none()
                && let Some(value) = branch.get("const")
            {
                let written = value.to_string();
                return Some(if written.chars().count() <= CONST_LABEL_LIMIT {
                    written
                } else {
                    json_type_name(value).to_string()
                });
            }
            // An enum is a constraint, not a type: show its values' type.
            if branch.get("type").is_none()
                && let Some(values) = branch.get("enum").and_then(Value::as_array)
            {
                let mut kinds: Vec<&str> = values.iter().map(json_type_name).collect();
                kinds.sort_unstable();
                kinds.dedup();
                return Some(kinds.join(" | "));
            }
            match branch.get("type") {
                Some(Value::String(value)) => Some(value.clone()),
                Some(Value::Array(values)) => {
                    let value = values
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" | ");
                    (!value.is_empty()).then_some(value)
                }
                _ => None,
            }
        })
        .collect();
    labels.sort();
    labels.dedup();
    if labels.is_empty() {
        fallback.to_string()
    } else {
        labels.join(" | ")
    }
}

/// The longest `const` value a type label shows as written; a longer one
/// shows its type.
const CONST_LABEL_LIMIT: usize = 24;

/// The type labels of `schemas` (the same key in several `oneOf` options)
/// in one: `"manual" | "static"`.
pub(super) fn type_label_of_all(schemas: &[&Value], root: &Value, fallback: &str) -> String {
    let mut labels: Vec<String> = Vec::new();
    for schema in schemas {
        let label = schema_type_label(schema, root, "");
        if !label.is_empty() && !labels.contains(&label) {
            labels.push(label);
        }
    }
    if labels.is_empty() {
        fallback.to_string()
    } else {
        labels.join(" | ")
    }
}

pub(super) fn json_type_name(value: &Value) -> &'static str {
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

pub(super) fn type_includes(schema: &Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(schema_type)) => schema_type == expected,
        Some(Value::Array(types)) => types.iter().any(|schema_type| schema_type == expected),
        _ => false,
    }
}

/// The `default` of a property schema, through `$ref` and `anyOf`/`oneOf`/`allOf`.
pub fn schema_default<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find_map(|branch| branch.get("default"))
}

/// A valid value for a property the user has not filled yet: its default,
/// const, or first enum value, else an empty value of its first type (`""`,
/// `0`, `false`, `[]`, `{}`, `null`). Inserting it keeps the document valid.
pub fn placeholder(schema: &Value, root: &Value) -> Value {
    let branches = resolve_branches(schema, root, 0);
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
        .any(|branch| branch.get("properties").is_some())
    {
        return Value::Object(serde_json::Map::new());
    }
    Value::Null
}

/// The `const` of a property schema, through `$ref` and the combinators.
pub fn schema_const<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find_map(|branch| branch.get("const"))
}

/// The branch of a property schema that describes an object, if any.
pub fn object_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find(|branch| {
            branch.get("properties").is_some()
                || branch.get("patternProperties").is_some()
                || branch.get("additionalProperties").is_some()
                || type_includes(branch, "object")
        })
}

/// The branch of a property schema that describes an array, if any.
pub fn array_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find(|branch| {
            branch.get("items").is_some()
                || branch.get("prefixItems").is_some()
                || type_includes(branch, "array")
        })
}

pub(crate) fn resolve_branches<'a>(
    schema: &'a Value,
    root: &'a Value,
    depth: usize,
) -> Vec<&'a Value> {
    if depth > 16 {
        return Vec::new();
    }
    let mut branches = Vec::new();
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        && let Some(target) = reference
            .strip_prefix('#')
            .and_then(|pointer| root.pointer(pointer))
    {
        branches.extend(resolve_branches(target, root, depth + 1));
    }
    branches.push(schema);
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(options) = schema.get(keyword).and_then(Value::as_array) {
            for option in options {
                branches.extend(resolve_branches(option, root, depth + 1));
            }
        }
    }
    branches
}

/// The schemas `path` leads to from the root. With `repeats_arrays` a key
/// naming an array is one of its items as well (each XML `<dns>` element is
/// one entry of `dns`).
pub(crate) fn schema_at_path<'a>(
    root: &'a Value,
    path: &[SchemaPathSegment],
    repeats_arrays: bool,
) -> Vec<&'a Value> {
    let mut schemas = vec![root];
    for segment in path {
        let mut next: Vec<&Value> = schemas
            .iter()
            .flat_map(|schema| child_schemas(schema, root, segment))
            .collect();
        // An item of something the schema has as one value (a block or an
        // element written twice): each copy is that value.
        if next.is_empty() && matches!(segment, SchemaPathSegment::ArrayItem(_)) {
            next = schemas.clone();
        }
        if repeats_arrays && matches!(segment, SchemaPathSegment::Key(_)) {
            let items: Vec<&Value> = next
                .iter()
                .flat_map(|schema| child_schemas(schema, root, &SchemaPathSegment::ArrayItem(0)))
                .collect();
            next.extend(items);
        }
        schemas = next;
        if schemas.is_empty() {
            break;
        }
    }
    schemas
}

/// The schemas of one child (a key or an array item) of a value of `schema`.
pub(crate) fn child_schemas<'a>(
    schema: &'a Value,
    root: &'a Value,
    segment: &SchemaPathSegment,
) -> Vec<&'a Value> {
    let mut next = Vec::new();
    for branch in resolve_branches(schema, root, 0) {
        let named = named_properties(branch, &segment.as_key());
        let item = segment
            .as_index()
            .and_then(|index| item_schema(branch, index));
        match segment {
            // An index names an item of an array, and in an object the key
            // of its digits when the object names that key.
            SchemaPathSegment::ArrayItem(_) => match item {
                Some(item) => next.push(item),
                None => next.extend(named),
            },
            SchemaPathSegment::Key(_) if !named.is_empty() => next.extend(named),
            // A key written as an index (`FieldPath::key("1")`) reads as one
            // in an array.
            SchemaPathSegment::Key(_) => match item {
                Some(item) => next.push(item),
                None => next.extend(
                    branch
                        .get("additionalProperties")
                        .filter(|additional| additional.is_object()),
                ),
            },
        }
    }
    next
}

/// The schema of item `index` of an array of `branch`.
fn item_schema(branch: &Value, index: usize) -> Option<&Value> {
    branch
        .get("prefixItems")
        .and_then(Value::as_array)
        .and_then(|prefix_items| prefix_items.get(index))
        .or_else(|| branch.get("items").filter(|items| items.is_object()))
}

/// The schemas `branch` gives the key `key` by name: its property and the
/// patterns matching it.
fn named_properties<'a>(branch: &'a Value, key: &str) -> Vec<&'a Value> {
    let property = branch
        .get("properties")
        .and_then(|properties| properties.get(key));
    let patterns = branch
        .get("patternProperties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(pattern, _)| patterns::pattern_matches(pattern, key))
        .map(|(_, property)| property);
    property.into_iter().chain(patterns).collect()
}

/// Reads `value` the way the schema expects for a language that writes arrays
/// by repeating their key: each lone value (or, for blocks, lone object)
/// where only an array is allowed becomes a one-item array (a single XML
/// `<dns>` element is `dns: [...]`).
pub(crate) fn conform_arrays(value: &mut Value, schema: &Value, repetition: Repetition) {
    conform(value, schema, schema, repetition, 0);
}

pub(super) fn conform(
    value: &mut Value,
    schema: &Value,
    root: &Value,
    repetition: Repetition,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    let branches = resolve_branches(schema, root, 0);
    let wants_array = branches.iter().any(|branch| {
        branch.get("items").is_some()
            || branch.get("prefixItems").is_some()
            || type_includes(branch, "array")
    });
    // XML text that looks like a number, boolean, or `null` is a string
    // where only a string is allowed (`<public_address>123</…>`).
    if repetition == Repetition::Elements
        && matches!(value, Value::Number(_) | Value::Bool(_) | Value::Null)
        && !branches.iter().any(|branch| accepts(branch, value))
        && branches
            .iter()
            .any(|branch| type_includes(branch, "string"))
    {
        let text = match &*value {
            Value::Null => "null".to_string(),
            other => other.to_string(),
        };
        *value = Value::String(text);
    }
    // An empty XML element reads as `{}`; where the schema wants text it is
    // the empty string.
    if repetition == Repetition::Elements
        && value.as_object().is_some_and(serde_json::Map::is_empty)
        && !branches.iter().any(|branch| accepts(branch, value))
        && branches
            .iter()
            .any(|branch| type_includes(branch, "string"))
    {
        *value = Value::String(String::new());
    }
    let repeatable = match repetition {
        Repetition::Elements => !value.is_array(),
        Repetition::Blocks => value.is_object(),
        _ => false,
    };
    if wants_array && repeatable && !branches.iter().any(|branch| accepts(branch, value)) {
        *value = Value::Array(vec![value.take()]);
    }
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                for item_schema in child_schemas(schema, root, &SchemaPathSegment::ArrayItem(index))
                {
                    conform(item, item_schema, root, repetition, depth + 1);
                }
            }
        }
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let segment = SchemaPathSegment::Key(key.clone());
                for child_schema in child_schemas(schema, root, &segment) {
                    conform(child, child_schema, root, repetition, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// Whether one resolved branch allows `value`'s kind as it is.
pub(super) fn accepts(branch: &Value, value: &Value) -> bool {
    match value {
        Value::Object(_) => {
            type_includes(branch, "object")
                || branch.get("properties").is_some()
                || branch.get("additionalProperties").is_some()
                || branch.get("patternProperties").is_some()
        }
        Value::String(_) => type_includes(branch, "string"),
        Value::Bool(_) => type_includes(branch, "boolean"),
        Value::Null => type_includes(branch, "null"),
        Value::Number(number) => {
            type_includes(branch, "number")
                || (number.is_i64() || number.is_u64()) && type_includes(branch, "integer")
        }
        Value::Array(_) => true,
    }
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
mod pattern_timing {
    use super::*;

    #[test]
    #[ignore = "timing, run by hand"]
    fn pattern_lookup_timing() {
        let schema = serde_json::json!({
            "patternProperties": {
                "^[a-z][a-z0-9_]*$": {"type": "string"},
                "^x-[A-Za-z0-9-]+$": {"type": "string"},
                "^[A-Z_]{2,}$": {"type": "integer"}
            }
        });
        let segment = SchemaPathSegment::Key("listen_port".to_string());
        let start = std::time::Instant::now();
        let mut found = 0;
        for _ in 0..10_000 {
            found += child_schemas(&schema, &schema, &segment).len();
        }
        eprintln!("10000 lookups: {:?} ({found})", start.elapsed());
    }
}
