//! Reading a document the way the schema expects, for languages that write
//! arrays by repeating their key (a lone value where only a list is allowed
//! is a one-item list) and XML, whose values are all text: its text is read
//! as the type of an option it fits (`<workers>4</workers>` is the number
//! for "an integer or `auto`", `<v>1.3</v>` the string a `const` lists).

use super::*;

/// Reads `value` the way `schema` expects for a language that writes arrays
/// by repeating their key: each lone value (or, for blocks, lone object)
/// where only an array is allowed becomes a one-item array (a single XML
/// `<dns>` element is `dns: [...]`), and XML text is typed by the schema.
pub(crate) fn conform_arrays(value: &mut Value, schema: &Value, repetition: Repetition) {
    conform(value, &[schema], schema, repetition, 0);
}

/// [`conform_arrays`] where `schemas` all describe `value` (the property
/// as each option of its parent gives it): read as one of them allows.
pub(super) fn conform(
    value: &mut Value,
    schemas: &[&Value],
    root: &Value,
    repetition: Repetition,
    depth: usize,
) {
    if depth > 64 || schemas.is_empty() {
        return;
    }
    let mut branches: Vec<&Value> = Vec::new();
    for schema in schemas {
        // An object decides the conditionals it is under (`if kind is a`).
        let instance = value.is_object().then_some(&*value);
        for branch in branches_for(schema, root, instance) {
            if !branches.iter().any(|known| std::ptr::eq(*known, branch)) {
                branches.push(branch);
            }
        }
    }
    let wants_array = branches.iter().any(|branch| {
        branch.get("items").is_some()
            || branch.get("prefixItems").is_some()
            || type_includes(branch, "array")
    });
    if repetition == Repetition::Elements
        && matches!(value, Value::Number(_) | Value::Bool(_) | Value::Null)
        && read_as_text(value, schemas, &branches, root)
    {
        *value = Value::String(scalar_text(value));
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
    if let Value::Object(map) = &*value {
        branches = discriminated(branches, map, root);
    }
    // Each child read with every schema the branches above give it.
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                let schemas = children_of(&branches, &SchemaPathSegment::ArrayItem(index));
                conform(item, &schemas, root, repetition, depth + 1);
            }
        }
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let schemas = children_of(&branches, &SchemaPathSegment::Key(key.clone()));
                conform(child, &schemas, root, repetition, depth + 1);
            }
        }
        _ => {}
    }
}

/// Whether XML text read as a number, boolean or `null` (`value`) is the
/// string it is written as: where a string the schema lists is that text
/// (`const: "1.3"`), else where no option `value` fits but its text does
/// (`oneOf [integer ≥ 10, string]` for `4`), else where no option allows
/// its kind and text is allowed (`type: string`, or only strings listed).
fn read_as_text(value: &Value, schemas: &[&Value], branches: &[&Value], root: &Value) -> bool {
    let text = Value::String(scalar_text(value));
    let listed: Vec<&Value> = branches.iter().flat_map(|branch| listed(branch)).collect();
    if listed.contains(&&text) && !listed.contains(&value) {
        return true;
    }
    let fits = |candidate: &Value| {
        schemas
            .iter()
            .any(|schema| conditions::decides(schema, candidate, root) == Some(true))
    };
    if fits(value) {
        return false;
    }
    if fits(&text) {
        return true;
    }
    let allowed = listed.contains(&value) || branches.iter().any(|branch| accepts(branch, value));
    !allowed
        && (branches
            .iter()
            .any(|branch| type_includes(branch, "string"))
            || (!listed.is_empty() && listed.iter().all(|allowed| allowed.is_string())))
}

/// The values `branch` lists as the only ones allowed (`const`, `enum`).
fn listed(branch: &Value) -> impl Iterator<Item = &Value> {
    branch.get("const").into_iter().chain(
        branch
            .get("enum")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
    )
}

/// A scalar as XML text writes it.
fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `branches` without the `oneOf`/`anyOf` options `map` is not (one whose
/// property lists values its own value is not: `kind: const "b"` for
/// `kind: a`), so a property the options type differently (`v` a number
/// in one, text in the other) is read as the option the object is in
/// wants it. All of them when every such option would go (the object is
/// none of them yet).
fn discriminated<'a>(
    branches: Vec<&'a Value>,
    map: &serde_json::Map<String, Value>,
    root: &'a Value,
) -> Vec<&'a Value> {
    let options: Vec<&Value> = branches
        .iter()
        .flat_map(|branch| ["oneOf", "anyOf"].map(|keyword| branch.get(keyword)))
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
        .map(|option| target(option, root))
        .collect();
    let gone: Vec<&Value> = options
        .iter()
        .copied()
        .filter(|option| refuted(option, map, root))
        .collect();
    let is_gone = |branch: &Value| gone.iter().any(|option| std::ptr::eq(*option, branch));
    let all_gone = options
        .iter()
        .filter(|option| option.get("properties").is_some())
        .all(|option| is_gone(option));
    if gone.is_empty() || all_gone {
        return branches;
    }
    branches
        .into_iter()
        .filter(|branch| !is_gone(branch))
        .collect()
}

/// The schema `schema`'s `$ref` names, else `schema`.
fn target<'a>(schema: &'a Value, root: &'a Value) -> &'a Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| ref_target(reference, root))
        .unwrap_or(schema)
}

/// Whether one of `branch`'s properties lists the values allowed and the
/// object's scalar there is none of them, also as text (XML's `1` is the
/// `"1"` listed).
fn refuted(branch: &Value, map: &serde_json::Map<String, Value>, root: &Value) -> bool {
    let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
        return false;
    };
    properties.iter().any(|(key, property)| {
        let Some(present) = map
            .get(key)
            .filter(|present| !present.is_object() && !present.is_array())
        else {
            return false;
        };
        let mut allowed = listed(target(property, root)).peekable();
        allowed.peek().is_some()
            && !allowed
                .any(|allowed| allowed == present || scalar_text(allowed) == scalar_text(present))
    })
}
