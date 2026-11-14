//! Walking a JSON Schema: types, defaults, branches, the schema at a
//! document path, and reading values to the shapes it expects.

use super::*;

pub(super) fn schema_type_label(schema: &Value, root: &Value, fallback: &str) -> String {
    let mut labels: Vec<String> = resolve_branches(schema, root)
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
        .any(|branch| branch.get("properties").is_some())
    {
        return Value::Object(serde_json::Map::new());
    }
    Value::Null
}

/// The `const` of a property schema, through `$ref` and the combinators.
pub fn schema_const<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root)
        .into_iter()
        .find_map(|branch| branch.get("const"))
}

/// The branch of a property schema that describes an object, if any.
pub fn object_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root).into_iter().find(|branch| {
        branch.get("properties").is_some()
            || branch.get("patternProperties").is_some()
            || branch.get("additionalProperties").is_some()
            || branch.get("unevaluatedProperties").is_some()
            || type_includes(branch, "object")
    })
}

/// The branch of a property schema that describes an array, if any.
pub fn array_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root).into_iter().find(|branch| {
        branch.get("items").is_some()
            || branch.get("prefixItems").is_some()
            || type_includes(branch, "array")
    })
}

/// The branches of `schema`: itself and every schema applying to the same
/// value, through `$ref`, `anyOf`/`oneOf`/`allOf`, `if`/`then`/`else` and
/// `dependentSchemas`, the value unknown (both of a conditional's
/// branches, every dependent schema; see [`branches_for`]).
pub(crate) fn resolve_branches<'a>(schema: &'a Value, root: &'a Value) -> Vec<&'a Value> {
    branches_for(schema, root, None)
}

/// [`resolve_branches`] for `instance`, the value the schema describes
/// when the document gives it: a conditional takes the branch its `if`
/// decides for it (both while the value does not decide), and a dependent
/// schema applies when the object has its key. Each schema is walked once,
/// so a reference cycle ends and a chain of references has no length
/// limit. Up to draft-07 a `$ref` stands alone: the keywords beside it are
/// ignored, as the validator ignores them.
pub(crate) fn branches_for<'a>(
    schema: &'a Value,
    root: &'a Value,
    instance: Option<&Value>,
) -> Vec<&'a Value> {
    walk_branches(schema, root, instance, true)
}

/// [`branches_for`] without the branches a conditional only may take: a
/// `then`/`else` only where its `if` decides for `instance`. What these
/// say holds for the value whatever is written next (a key one declares
/// `false` cannot be written).
pub(crate) fn certain_branches<'a>(
    schema: &'a Value,
    root: &'a Value,
    instance: Option<&Value>,
) -> Vec<&'a Value> {
    walk_branches(schema, root, instance, false)
}

fn walk_branches<'a>(
    schema: &'a Value,
    root: &'a Value,
    instance: Option<&Value>,
    undecided: bool,
) -> Vec<&'a Value> {
    enum Step<'a> {
        Visit(&'a Value),
        Emit(&'a Value),
    }
    crate::editor::work_count::note(crate::editor::work_count::Work::BranchWalk);
    let ref_alone = refs_stand_alone(root);
    let mut seen: HashSet<*const Value> = HashSet::new();
    let mut branches = Vec::new();
    let mut steps = vec![Step::Visit(schema)];
    while let Some(step) = steps.pop() {
        let schema = match step {
            Step::Emit(schema) => {
                branches.push(schema);
                continue;
            }
            Step::Visit(schema) => schema,
        };
        if !seen.insert(std::ptr::from_ref(schema)) {
            continue;
        }
        let target = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| ref_target(reference, root));
        // Pushed last first: the walk takes the target's branches, the
        // schema, then its applicators in order.
        let mut next: Vec<Step<'a>> = Vec::new();
        if let Some(target) = target {
            next.push(Step::Visit(target));
        }
        if !(ref_alone && schema.get("$ref").is_some()) {
            next.push(Step::Emit(schema));
            for keyword in ["anyOf", "oneOf", "allOf"] {
                for option in schema
                    .get(keyword)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    next.push(Step::Visit(option));
                }
            }
            next.extend(conditional_branches(schema, root, instance, undecided).map(Step::Visit));
            next.extend(dependent_schemas(schema, instance).map(Step::Visit));
        }
        steps.extend(next.into_iter().rev());
    }
    branches
}

/// The schema a `$ref` names in `root`: a `#` JSON pointer, its tokens
/// percent-decoded. (References by `$id` or `$anchor` are rewritten to
/// pointers when the schema is built, see `ui_refs`.)
pub(crate) fn ref_target<'a>(reference: &str, root: &'a Value) -> Option<&'a Value> {
    let pointer = reference.strip_prefix('#')?;
    root.pointer(pointer)
        .or_else(|| root.pointer(&super::super::ui_refs::percent_decoded(pointer)))
}

/// Whether the keywords beside a `$ref` are ignored: up to draft-07, which
/// the root's `$schema` names.
fn refs_stand_alone(root: &Value) -> bool {
    root.get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|dialect| {
            ["draft-03", "draft-04", "draft-06", "draft-07"]
                .iter()
                .any(|draft| dialect.contains(draft))
        })
}

/// The `then`/`else` of `schema`'s `if` that apply to `instance`: the one
/// the `if` decides, or (`undecided`) both while it does not.
fn conditional_branches<'a>(
    schema: &'a Value,
    root: &'a Value,
    instance: Option<&Value>,
    undecided: bool,
) -> impl Iterator<Item = &'a Value> {
    let condition = schema.get("if");
    let decided = condition.and_then(|condition| {
        instance.and_then(|instance| super::conditions::decides(condition, instance, root))
    });
    let takes = |branch: bool| match decided {
        Some(decided) => decided == branch,
        None => undecided,
    };
    let then = schema
        .get("then")
        .filter(|_| condition.is_some() && takes(true));
    let otherwise = schema
        .get("else")
        .filter(|_| condition.is_some() && takes(false));
    then.into_iter().chain(otherwise)
}

/// The dependent schemas of `schema` (`dependentSchemas`, and draft-07's
/// `dependencies` given as schemas) that apply to `instance`: those whose
/// key the object has, or all of them while the value is unknown.
fn dependent_schemas<'a>(
    schema: &'a Value,
    instance: Option<&Value>,
) -> impl Iterator<Item = &'a Value> {
    ["dependentSchemas", "dependencies"]
        .into_iter()
        .filter_map(|keyword| schema.get(keyword).and_then(Value::as_object))
        .flatten()
        .filter(|(_, dependent)| dependent.is_object() || dependent.is_boolean())
        .filter(move |(key, _)| match instance {
            None => true,
            Some(Value::Object(members)) => members.contains_key(key.as_str()),
            Some(_) => false,
        })
        .map(|(_, dependent)| dependent)
}

/// The schemas `path` leads to from the root. With `repeats_arrays` a key
/// naming an array is one of its items as well (each XML `<dns>` element is
/// one entry of `dns`).
pub(crate) fn schema_at_path<'a>(
    root: &'a Value,
    path: &[SchemaPathSegment],
    repeats_arrays: bool,
) -> Vec<&'a Value> {
    schema_at_path_in(root, path, repeats_arrays, None)
}

/// [`schema_at_path`] through `document`, the parsed document when there is
/// one: each step's conditionals are decided by the value there (see
/// [`branches_for`]).
pub(crate) fn schema_at_path_in<'a>(
    root: &'a Value,
    path: &[SchemaPathSegment],
    repeats_arrays: bool,
    document: Option<&Value>,
) -> Vec<&'a Value> {
    let mut schemas = vec![root];
    for (depth, segment) in path.iter().enumerate() {
        let instance = document.and_then(|document| value_at(document, &path[..depth]));
        let mut next: Vec<&Value> = schemas
            .iter()
            .flat_map(|schema| child_schemas_in(schema, root, segment, instance))
            .collect();
        // An item of something the schema has as one value (a block or an
        // element written twice): each copy is that value.
        if next.is_empty() && matches!(segment, SchemaPathSegment::ArrayItem(_)) {
            next = schemas.clone();
        }
        if repeats_arrays && matches!(segment, SchemaPathSegment::Key(_)) {
            let instance = document.and_then(|document| value_at(document, &path[..=depth]));
            let items: Vec<&Value> = next
                .iter()
                .flat_map(|schema| {
                    child_schemas_in(schema, root, &SchemaPathSegment::ArrayItem(0), instance)
                })
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
    child_schemas_in(schema, root, segment, None)
}

/// [`child_schemas`] of `instance`, the value of `schema` when known.
pub(crate) fn child_schemas_in<'a>(
    schema: &'a Value,
    root: &'a Value,
    segment: &SchemaPathSegment,
    instance: Option<&Value>,
) -> Vec<&'a Value> {
    children_of(&branches_for(schema, root, instance), segment)
}

/// The schemas of one child (a key or an array item) of a value whose
/// schema's branches are `branches` (see [`branches_for`]): a walk taking
/// every child of one value resolves its branches once.
fn children_of<'a>(branches: &[&'a Value], segment: &SchemaPathSegment) -> Vec<&'a Value> {
    let mut next = Vec::new();
    for branch in branches {
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
    // What no branch evaluates is the `unevaluatedProperties` (or
    // `unevaluatedItems`) schema's.
    if next.is_empty() {
        let keyword = match segment {
            SchemaPathSegment::Key(_) => "unevaluatedProperties",
            SchemaPathSegment::ArrayItem(_) => "unevaluatedItems",
        };
        next.extend(
            branches
                .iter()
                .filter_map(|branch| branch.get(keyword))
                .filter(|unevaluated| unevaluated.is_object()),
        );
    }
    next
}

/// The schema of item `index` of an array of `branch`: its tuple position
/// (`prefixItems`, or up to draft-07 an `items` list), else the schema of
/// the other items (`items`, or after a draft-07 tuple `additionalItems`).
pub(crate) fn item_schema(branch: &Value, index: usize) -> Option<&Value> {
    let tuple = |keyword: &str| branch.get(keyword).and_then(Value::as_array);
    if let Some(positions) = tuple("prefixItems") {
        return positions
            .get(index)
            .or_else(|| branch.get("items").filter(|items| items.is_object()));
    }
    if let Some(positions) = tuple("items") {
        return positions.get(index).or_else(|| {
            branch
                .get("additionalItems")
                .filter(|items| items.is_object())
        });
    }
    branch.get("items").filter(|items| items.is_object())
}

/// The schemas `branch` gives the key `key` by name: its property and the
/// patterns matching it.
pub(crate) fn named_properties<'a>(branch: &'a Value, key: &str) -> Vec<&'a Value> {
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
    conform(value, schema, schema, repetition, 0, false);
}

/// [`conform_arrays`] of a value whose text scalars the schema typed
/// already (see `text_typing`): they are not guessed again.
pub(crate) fn conform_typed_arrays(value: &mut Value, schema: &Value, repetition: Repetition) {
    conform(value, schema, schema, repetition, 0, true);
}

pub(super) fn conform(
    value: &mut Value,
    schema: &Value,
    root: &Value,
    repetition: Repetition,
    depth: usize,
    typed: bool,
) {
    if depth > 64 {
        return;
    }
    let branches = resolve_branches(schema, root);
    let wants_array = branches.iter().any(|branch| {
        branch.get("items").is_some()
            || branch.get("prefixItems").is_some()
            || type_includes(branch, "array")
    });
    // XML text that looks like a number, boolean, or `null` is a string
    // where only a string is allowed (`<public_address>123</…>`).
    if repetition == Repetition::Elements
        && !typed
        && matches!(value, Value::Number(_) | Value::Bool(_) | Value::Null)
        && !branches.iter().any(|branch| accepts(branch, value))
        && branches
            .iter()
            .any(|branch| type_includes(branch, "string"))
    {
        *value = Value::String(scalar_text(value));
    }
    // Likewise where the values allowed are listed (`const`, `enum`) as
    // strings, untyped: `<v>1.3</v>` is the `"1.3"` listed. (XML reads a
    // number only from text it writes back unchanged: `1.3`, not `1.30`.)
    if repetition == Repetition::Elements
        && !typed
        && matches!(value, Value::Number(_) | Value::Bool(_) | Value::Null)
        && listed_as_strings(&branches, value)
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
    // Each child's schemas from the branches resolved above.
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                for item_schema in children_of(&branches, &SchemaPathSegment::ArrayItem(index)) {
                    conform(item, item_schema, root, repetition, depth + 1, typed);
                }
            }
        }
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let segment = SchemaPathSegment::Key(key.clone());
                for child_schema in children_of(&branches, &segment) {
                    conform(child, child_schema, root, repetition, depth + 1, typed);
                }
            }
        }
        _ => {}
    }
}

/// A scalar as XML text writes it.
fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// Whether `branches` list the values allowed (`const`, `enum`), `value`
/// is not one of them, and they are strings: its text, or all of them.
fn listed_as_strings(branches: &[&Value], value: &Value) -> bool {
    let listed: Vec<&Value> = branches
        .iter()
        .flat_map(|branch| {
            branch.get("const").into_iter().chain(
                branch
                    .get("enum")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            )
        })
        .collect();
    let text = scalar_text(value);
    !listed.is_empty()
        && !listed.contains(&value)
        && (listed.iter().any(|allowed| allowed.as_str() == Some(&text))
            || listed.iter().all(|allowed| allowed.is_string()))
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
