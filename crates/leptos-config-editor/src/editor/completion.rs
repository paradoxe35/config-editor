use super::commands::indent_unit;
use super::diagnostics::SchemaBundle;
use super::keyboard::{replace_selection, utf16_to_byte_index};
use super::language::{
    CompletionContext, ConfigLanguage, EntryPosition, InsertContext, Repetition, SchemaPathSegment,
};
use super::metrics::read_editor_metrics;
use super::positioning::{FALLBACK_VIEWPORT_HEIGHT, FALLBACK_VIEWPORT_WIDTH, POPUP_MARGIN};
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use wasm_bindgen::JsCast;

#[derive(Clone, PartialEq)]
pub(super) struct Completion {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) type_label: String,
    pub(super) insert: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) cursor_offset: usize,
    /// UTF-16 length of the text before the caret that is left selected (an
    /// inserted default), so typing replaces it.
    pub(super) selected: usize,
    pub(super) is_value: bool,
    /// Character ranges of `label` matched by the query, for highlighting.
    pub(super) matched: Vec<(usize, usize)>,
}

fn schema_type_label(schema: &Value, root: &Value, fallback: &str) -> String {
    let mut labels: Vec<String> = resolve_branches(schema, root, 0)
        .into_iter()
        .filter_map(|branch| {
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

fn json_type_name(value: &Value) -> &'static str {
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

fn type_includes(schema: &Value, expected: &str) -> bool {
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

fn resolve_branches<'a>(schema: &'a Value, root: &'a Value, depth: usize) -> Vec<&'a Value> {
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
fn schema_at_path<'a>(
    root: &'a Value,
    path: &[SchemaPathSegment],
    repeats_arrays: bool,
) -> Vec<&'a Value> {
    let mut schemas = vec![root];
    for segment in path {
        let mut next: Vec<&Value> = schemas
            .into_iter()
            .flat_map(|schema| child_schemas(schema, root, segment))
            .collect();
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
fn child_schemas<'a>(
    schema: &'a Value,
    root: &'a Value,
    segment: &SchemaPathSegment,
) -> Vec<&'a Value> {
    let mut next = Vec::new();
    for branch in resolve_branches(schema, root, 0) {
        match segment {
            SchemaPathSegment::ArrayItem(index) => {
                if let Some(item_schema) = branch
                    .get("prefixItems")
                    .and_then(Value::as_array)
                    .and_then(|prefix_items| prefix_items.get(*index))
                {
                    next.push(item_schema);
                } else if let Some(items) = branch.get("items").filter(|items| items.is_object()) {
                    next.push(items);
                }
            }
            SchemaPathSegment::Key(key) => {
                let mut matched = false;
                if let Some(property) = branch
                    .get("properties")
                    .and_then(|properties| properties.get(key))
                {
                    next.push(property);
                    matched = true;
                }
                if let Some(patterns) = branch.get("patternProperties").and_then(Value::as_object) {
                    for (pattern, property) in patterns {
                        if Regex::new(pattern).is_ok_and(|regex| regex.is_match(key)) {
                            next.push(property);
                            matched = true;
                        }
                    }
                }
                if !matched
                    && let Some(additional) = branch.get("additionalProperties")
                    && additional.is_object()
                {
                    next.push(additional);
                }
            }
        }
    }
    next
}

/// Reads `value` the way the schema expects for a language that writes arrays
/// by repeating their key: each lone value (or, for blocks, lone object)
/// where only an array is allowed becomes a one-item array (a single XML
/// `<dns>` element is `dns: [...]`).
pub(super) fn conform_arrays(value: &mut Value, schema: &Value, repetition: Repetition) {
    conform(value, schema, schema, repetition, 0);
}

fn conform(value: &mut Value, schema: &Value, root: &Value, repetition: Repetition, depth: usize) {
    if depth > 64 {
        return;
    }
    let branches = resolve_branches(schema, root, 0);
    let wants_array = branches.iter().any(|branch| {
        branch.get("items").is_some()
            || branch.get("prefixItems").is_some()
            || type_includes(branch, "array")
    });
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
fn accepts(branch: &Value, value: &Value) -> bool {
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
fn schema_value_candidates(branch: &Value, open: bool) -> Vec<Value> {
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

fn path_with_key(path: &[SchemaPathSegment], key: &str) -> Vec<SchemaPathSegment> {
    let mut path = path.to_vec();
    path.push(SchemaPathSegment::Key(key.to_string()));
    path
}

fn value_label(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn unquoted(typed: &str) -> &str {
    let typed = typed.trim_start();
    if let Some(quote) = typed
        .chars()
        .next()
        .filter(|quote| *quote == '"' || *quote == '\'')
    {
        let inner = &typed[quote.len_utf8()..];
        return inner.strip_suffix(quote).unwrap_or(inner);
    }
    typed
}

/// Base scores of the `match_score` tiers, highest first. Within a tier,
/// shorter labels win.
pub(super) const MATCH_TIER_EXACT_PREFIX: i64 = 1000;
pub(super) const MATCH_TIER_CASE_INSENSITIVE_PREFIX: i64 = 900;
pub(super) const MATCH_TIER_INITIALS: i64 = 800;
pub(super) const MATCH_TIER_SUBSTRING: i64 = 700;
pub(super) const MATCH_TIER_SUBSEQUENCE: i64 = 500;

/// Substring tier: earlier hits win (`- 2 * index`) and length counts only a
/// quarter, so a short late match does not beat a long early one.
pub(super) const MATCH_SUBSTRING_INDEX_PENALTY: i64 = 2;
pub(super) const MATCH_SUBSTRING_LENGTH_DIVISOR: i64 = 4;

/// Scores `label` against `typed`: exact prefix, case-insensitive prefix, word
/// initials (`mc` for `max_connections`), substring, then subsequence.
fn match_score(label: &str, typed: &str) -> Option<i64> {
    if typed.is_empty() {
        return Some(0);
    }
    let label_length = label.chars().count() as i64;
    if label.starts_with(typed) {
        return Some(MATCH_TIER_EXACT_PREFIX - label_length);
    }
    let label_lower = label.to_lowercase();
    let typed_lower = typed.to_lowercase();
    if label_lower.starts_with(&typed_lower) {
        return Some(MATCH_TIER_CASE_INSENSITIVE_PREFIX - label_length);
    }
    let initials: String = label
        .split(['_', '-', '.', ' '])
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.chars().next())
        .flat_map(char::to_lowercase)
        .collect();
    if !initials.is_empty() && initials.starts_with(&typed_lower) {
        return Some(MATCH_TIER_INITIALS - label_length);
    }
    if let Some(index) = label_lower.find(&typed_lower) {
        return Some(
            MATCH_TIER_SUBSTRING
                - index as i64 * MATCH_SUBSTRING_INDEX_PENALTY
                - label_length / MATCH_SUBSTRING_LENGTH_DIVISOR,
        );
    }
    let mut remaining = label_lower.chars();
    let mut matched = 0usize;
    for wanted in typed_lower.chars() {
        if remaining.any(|candidate| candidate == wanted) {
            matched += 1;
        } else {
            return None;
        }
    }
    (matched == typed_lower.chars().count()).then_some(MATCH_TIER_SUBSEQUENCE - label_length)
}

/// Character ranges of `label` matched by `typed`, following the
/// `match_score` tiers; empty when nothing matches.
pub(super) fn match_ranges(label: &str, typed: &str) -> Vec<(usize, usize)> {
    if typed.is_empty() {
        return Vec::new();
    }
    let typed_len = typed.chars().count();
    if label.starts_with(typed) {
        return vec![(0, typed_len)];
    }
    let label_lower = label.to_lowercase();
    let typed_lower = typed.to_lowercase();
    if label_lower.starts_with(&typed_lower) {
        return vec![(0, typed_len)];
    }
    let mut initials: Vec<(usize, char)> = Vec::new();
    // Separators are one ASCII byte, so this tracks each part's start exactly.
    let mut byte_offset = 0;
    for part in label.split(['_', '-', '.', ' ']) {
        if let Some(first) = part.chars().next() {
            let char_offset = label[..byte_offset].chars().count();
            initials.extend(first.to_lowercase().map(|lower| (char_offset, lower)));
        }
        byte_offset += part.len() + 1;
    }
    if !initials.is_empty()
        && initials
            .iter()
            .map(|(_, initial)| *initial)
            .collect::<String>()
            .starts_with(&typed_lower)
    {
        return initials
            .into_iter()
            .take(typed_len)
            .map(|(offset, _)| (offset, offset + 1))
            .collect();
    }
    if let Some(byte_index) = label_lower.find(&typed_lower) {
        let start = label_lower[..byte_index].chars().count();
        return vec![(start, start + typed_len)];
    }
    let mut ranges = Vec::new();
    let mut remaining = label_lower.chars().enumerate();
    for wanted in typed_lower.chars() {
        let mut hit = None;
        for (index, candidate) in remaining.by_ref() {
            if candidate == wanted {
                hit = Some(index);
                break;
            }
        }
        match hit {
            Some(index) => ranges.push((index, index + 1)),
            None => return Vec::new(),
        }
    }
    ranges
}

/// Lifts required keys within a match tier, never above the next tier.
const REQUIRED_BONUS: i64 = 40;

/// Best match first. Ties keep schema order for values (enum order, so a
/// default listed first stays first) and sort alphabetically for properties.
fn sorted_completions(mut scored: Vec<(i64, Completion)>) -> Vec<Completion> {
    scored.sort_by(|left, right| {
        right.0.cmp(&left.0).then_with(|| {
            if left.1.is_value && right.1.is_value {
                std::cmp::Ordering::Equal
            } else {
                left.1.label.cmp(&right.1.label)
            }
        })
    });
    scored.dedup_by(|left, right| left.1.label == right.1.label);
    scored
        .into_iter()
        .map(|(_, completion)| completion)
        .collect()
}

/// Schemas often give a port a string example (`"7835"`) next to an integer
/// branch; when numbers are allowed, insert the bare number.
fn numeric_example_as_number(value: Value, numeric_allowed: bool) -> Value {
    if numeric_allowed
        && let Value::String(text) = &value
        && let Ok(number) = text.parse::<i64>()
    {
        return Value::from(number);
    }
    value
}

#[allow(clippy::too_many_arguments)]
fn value_completions(
    schemas: &[&Value],
    typed: &str,
    start: usize,
    end: usize,
    root: &Value,
    leading: &str,
    open: bool,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    let query = unquoted(typed);
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    // A language without null (TOML) is not offered one.
    let null_writable = behavior
        .serialize(&serde_json::json!({ "value": null }), None)
        .is_ok();
    let numeric_allowed = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .any(|branch| type_includes(branch, "integer") || type_includes(branch, "number"));
    for schema in schemas {
        for branch in resolve_branches(schema, root, 0) {
            let description = branch
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let type_label = schema_type_label(branch, root, "value");
            for value in schema_value_candidates(branch, open) {
                let value = numeric_example_as_number(value, numeric_allowed);
                let label = value_label(&value);
                if !seen.insert(label.clone()) || (value.is_null() && !null_writable) {
                    continue;
                }
                if let Some(score) = match_score(&label, query) {
                    let insert = format!("{leading}{}", behavior.format_value(&value));
                    candidates.push((
                        score,
                        Completion {
                            label: label.clone(),
                            detail: description.to_string(),
                            type_label: type_label.clone(),
                            cursor_offset: insert.encode_utf16().count(),
                            insert,
                            start,
                            end,
                            selected: 0,
                            is_value: true,
                            matched: match_ranges(&label, query),
                        },
                    ));
                }
            }
        }
    }
    sorted_completions(candidates)
}

/// `(cursor_offset, selected)` for an inserted key: a scalar default the
/// insert wrote just before the caret (`max_pending: 512|`, `mode = "fail|"`)
/// is selected, so typing replaces it instead of appending to it.
fn default_selection(
    insert: &str,
    cursor_offset: usize,
    property: &Value,
    root: &Value,
) -> (usize, usize) {
    let unselected = (cursor_offset, 0);
    let Some(default) = schema_default(property, root)
        .or_else(|| schema_const(property, root))
        .filter(|value| !value.is_object() && !value.is_array() && !value.is_null())
    else {
        return unselected;
    };
    let text = match default {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    if text.is_empty() {
        return unselected;
    }
    let before = &insert[..utf16_to_byte_index(insert, cursor_offset)];
    let length = text.encode_utf16().count();
    if before.ends_with(&text) {
        return (cursor_offset, length);
    }
    // The caret is after the closing quote: select inside the quotes.
    if let Some(quoted) = before.strip_suffix(['"', '\''])
        && quoted.ends_with(&text)
    {
        return (cursor_offset - 1, length);
    }
    unselected
}

#[allow(clippy::too_many_arguments)]
fn property_completions(
    schemas: &[&Value],
    typed: &str,
    start: usize,
    end: usize,
    root: &Value,
    existing: Option<&HashSet<String>>,
    repetition: Repetition,
    insert_context: &InsertContext<'_>,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    let mut candidates = Vec::new();
    for branch in schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
    {
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let required: HashSet<&str> = branch
            .get("required")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for (key, property) in properties {
            // A list key written by repetition can appear again: any list in
            // XML, a list of objects as HCL blocks (never inside `{ … }`).
            let repeatable = match repetition {
                Repetition::Elements => array_branch(property, root).is_some(),
                Repetition::Blocks => {
                    insert_context.position == EntryPosition::Block
                        && array_branch(property, root)
                            .and_then(|array| array.get("items"))
                            .is_some_and(|items| object_branch(items, root).is_some())
                }
                _ => false,
            };
            if !repeatable && existing.is_some_and(|existing| existing.contains(key)) {
                continue;
            }
            let Some(score) = match_score(key, typed) else {
                continue;
            };
            let score = score
                + if required.contains(key.as_str()) {
                    REQUIRED_BONUS
                } else {
                    0
                };
            let Some((insert, cursor_offset)) =
                behavior.property_insert(key, property, root, insert_context)
            else {
                continue;
            };
            let (cursor_offset, selected) =
                default_selection(&insert, cursor_offset, property, root);
            candidates.push((
                score,
                Completion {
                    label: key.clone(),
                    detail: property
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    type_label: schema_type_label(property, root, "property"),
                    insert,
                    start,
                    end,
                    cursor_offset,
                    selected,
                    is_value: false,
                    matched: match_ranges(key, typed),
                },
            ));
        }
    }
    sorted_completions(candidates)
}

pub(super) fn completions(
    source: &str,
    cursor: usize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    if !enabled {
        return Vec::new();
    }
    let Some(bundle) = schema else {
        return Vec::new();
    };
    // Nothing is suggested inside a comment.
    if behavior.lexical_state(source, cursor).in_comment {
        return Vec::new();
    }
    let cursor_byte = utf16_to_byte_index(source, cursor);
    let context = behavior.completion_context(source, cursor);
    let mut options =
        completions_for_context(source, cursor_byte, &context, force, bundle, behavior);
    for option in &mut options {
        frame_insert(option, &context);
        rename_key_with_value(option, source, cursor);
        // An insert ending in a line break before an existing one would leave
        // a blank line, unless the caret goes on that line (`[pool]\n|`).
        let end = utf16_to_byte_index(source, option.end);
        let caret_on_new_line = option.cursor_offset == option.insert.encode_utf16().count();
        if option.insert.ends_with('\n') && source[end..].starts_with('\n') && !caret_on_new_line {
            option.insert.pop();
            let units = option.insert.encode_utf16().count();
            option.cursor_offset = option.cursor_offset.min(units);
        }
    }
    options
}

/// Adds the context's entry prefix and suffix to an accepted insert, and lays
/// out a multi-line insert per `context.block`.
pub(super) fn frame_insert(option: &mut Completion, context: &CompletionContext) {
    let expand = !option.is_value && option.insert.contains('\n');
    match context.block.as_ref().filter(|_| expand) {
        Some(block) => {
            let prefix = format!(
                "{}\n{}",
                context.entry_prefix.trim_end_matches([' ', '\t']),
                block.indent
            );
            option.insert.insert_str(0, &prefix);
            option.insert.push_str(&block.after);
            option.cursor_offset += prefix.encode_utf16().count();
            option.start = block.start;
            option.end = block.end;
        }
        None if context
            .entry_after
            .as_deref()
            .is_some_and(|word| !word.is_empty() && option.label.ends_with(word)) =>
        {
            // The suggestion completes the key the caret is inside: rename it,
            // keeping the entry's existing separator and value.
            option.end += context
                .entry_after
                .as_deref()
                .map_or(0, |word| word.encode_utf16().count());
            option.insert = option.label.clone();
            option.cursor_offset = option.insert.encode_utf16().count();
        }
        None => {
            option.insert.insert_str(0, &context.entry_prefix);
            // A suffix starting a new line would double an insert's own break.
            if context.entry_suffix.starts_with('\n') && option.insert.ends_with('\n') {
                option.insert.pop();
            }
            option.insert.push_str(&context.entry_suffix);
            option.cursor_offset += context.entry_prefix.encode_utf16().count();
        }
    }
}

fn completions_for_context(
    source: &str,
    cursor_byte: usize,
    context: &CompletionContext,
    force: bool,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    let schemas = schema_at_path(
        &bundle.schema,
        &context.path,
        behavior.repetition() != Repetition::None,
    );

    if let Some(key) = context.value_key.as_deref() {
        // A number being typed is the user's own value: suggesting a longer
        // one (`1` → `10`) would make Enter replace it. Ctrl+Space still lists them.
        let typed_value = unquoted(&context.typed);
        if !force && !typed_value.is_empty() && typed_value.parse::<f64>().is_ok() {
            return Vec::new();
        }
        let value_schemas = schema_at_path(
            &bundle.schema,
            &path_with_key(&context.path, key),
            behavior.repetition() != Repetition::None,
        );
        let leading = if context.value_needs_space { " " } else { "" };
        let mut options = value_completions(
            &value_schemas,
            &context.typed,
            context.replace_start,
            context.replace_end,
            &bundle.schema,
            leading,
            force || typed_value.is_empty(),
            behavior,
        );
        if !force && unquoted(&context.typed).is_empty() {
            options.retain(|option| option.label != "null");
        }
        let object_value = value_schemas
            .iter()
            .any(|schema| object_branch(schema, &bundle.schema).is_some());
        if context.value_or_children && (options.is_empty() || object_value) {
            let children = CompletionContext {
                path: path_with_key(&context.path, key),
                value_key: None,
                value_or_children: false,
                explicit_entry_start: true,
                ..context.clone()
            };
            return completions_for_context(
                source,
                cursor_byte,
                &children,
                force,
                bundle,
                behavior,
            );
        }
        return options;
    }

    if context.is_array_item {
        let has_object_properties = schemas
            .iter()
            .flat_map(|schema| resolve_branches(schema, &bundle.schema, 0))
            .any(|branch| {
                branch.get("properties").is_some()
                    || branch.get("patternProperties").is_some()
                    || type_includes(branch, "object")
            });
        if !has_object_properties {
            return value_completions(
                &schemas,
                &context.typed,
                context.replace_start,
                context.replace_end,
                &bundle.schema,
                "",
                force || unquoted(&context.typed).is_empty(),
                behavior,
            );
        }
    }

    let query = unquoted(&context.typed);
    if query.is_empty() && !force && !context.explicit_entry_start {
        return Vec::new();
    }
    if query.contains([':', '#', '\n']) {
        return Vec::new();
    }

    if context.no_properties {
        return Vec::new();
    }
    let existing_keys = behavior.existing_keys(source, cursor_byte, &context.path);
    let unit = indent_unit(source);
    let base_indent = context
        .block
        .as_ref()
        .map_or(context.base_indent.as_str(), |block| block.indent.as_str());
    let mut options = property_completions(
        &schemas,
        query,
        context.replace_start,
        context.replace_end,
        &bundle.schema,
        existing_keys.as_ref(),
        behavior.repetition(),
        &InsertContext {
            base_indent,
            unit: &unit,
            position: context.position,
            path: &context.path,
        },
        behavior,
    );
    // A key typed directly in an array starts a new object item, which needs
    // its delimiters: `[|]` becomes `[{ hosts = [] }]`, not `[hosts = []]`.
    if let Some((open, close)) = context.entry_wrap {
        for option in &mut options {
            option.insert = format!("{open}{}{close}", option.insert);
            option.cursor_offset += open.encode_utf16().count();
        }
    }
    options
}

/// Accepting a key keeps the list open so its child keys can be suggested. Accepting
/// a value ends the entry, so the list closes instead of offering the same value again.
/// A key whose default is left selected is a placeholder: typing replaces it,
/// so there is nothing to suggest.
pub(super) fn accept_reopens_suggestions(completion: &Completion, auto_trigger: bool) -> bool {
    auto_trigger && !completion.is_value && completion.selected == 0
}

/// Rows PageUp/PageDown move: one popup page
/// (`COMPLETION_POPUP_MAX_HEIGHT / COMPLETION_ROW_HEIGHT`).
pub(super) const COMPLETION_PAGE_ROWS: usize = 7;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct KeyHover {
    pub(super) key: String,
    pub(super) start_column: usize,
    pub(super) type_label: String,
    pub(super) description: Option<String>,
    pub(super) default: Option<String>,
    pub(super) examples: Vec<String>,
    pub(super) required: bool,
}

/// The key under character `column` of zero-based `line_index`, with its
/// schema documentation.
pub(super) fn key_hover(
    source: &str,
    line_index: usize,
    column: usize,
    schema: Option<&SchemaBundle>,
    behavior: &dyn ConfigLanguage,
) -> Option<KeyHover> {
    let bundle = schema?;
    let line_start: usize = source
        .split('\n')
        .take(line_index)
        .map(|line| line.len() + 1)
        .sum();
    let line = source.get(line_start..)?.split('\n').next()?;
    let resolved = behavior.hover_path(source, line_index, column);
    let (key, start_column) = match &resolved {
        Some((_, key, start)) => (key.clone(), *start),
        None => behavior.hover_key(line, column)?,
    };
    let cursor = source[..line_start + behavior.hover_cursor(line, column)]
        .encode_utf16()
        .count();
    let context = behavior.completion_context(source, cursor);
    // The hovered key is either the key whose value the context is in, or the
    // container the context is inside (a TOML `[table]` header, an HCL block
    // name, an XML element with attributes).
    let path = if let Some((path, _, _)) = resolved {
        path
    } else if context.value_key.as_deref() == Some(key.as_str()) {
        path_with_key(&context.path, &key)
    } else {
        let mut container = context.path.clone();
        while matches!(container.last(), Some(SchemaPathSegment::ArrayItem(_))) {
            container.pop();
        }
        if context.value_key.is_some()
            || container.last() != Some(&SchemaPathSegment::Key(key.clone()))
        {
            return None;
        }
        container
    };
    let parent = &path[..path.len() - 1];
    let root = &bundle.schema;
    let schemas = schema_at_path(root, &path, behavior.repetition() != Repetition::None);
    if schemas.is_empty() {
        return None;
    }
    let branches: Vec<&Value> = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .collect();
    let description = branches.iter().find_map(|branch| {
        branch
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let default = schemas
        .iter()
        .find_map(|schema| schema_default(schema, root))
        .map(value_label);
    let mut examples: Vec<String> = Vec::new();
    for branch in &branches {
        let values = match branch.get("examples") {
            Some(Value::Array(examples)) => examples.clone(),
            Some(example) => vec![example.clone()],
            None => Vec::new(),
        };
        for value in values.iter().chain(branch.get("example")) {
            let label = value_label(value);
            if !examples.contains(&label) {
                examples.push(label);
            }
        }
    }
    examples.truncate(5);
    let required = schema_at_path(root, parent, behavior.repetition() != Repetition::None)
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .any(|branch| {
            branch
                .get("required")
                .and_then(Value::as_array)
                .is_some_and(|required| required.iter().any(|name| name.as_str() == Some(&key)))
        });
    Some(KeyHover {
        key,
        start_column,
        type_label: schema_type_label(schemas[0], root, "property"),
        description,
        default,
        examples,
        required,
    })
}

pub(super) fn should_auto_trigger(input_type: &str, previous: &str, current: &str) -> bool {
    if current == previous || current.matches('\n').count() > previous.matches('\n').count() {
        return false;
    }
    let grew = current.len() > previous.len();
    let typed_over_selection = matches!(input_type, "insertText" | "insertCompositionText");
    if !grew && !typed_over_selection {
        return false;
    }
    !matches!(
        input_type,
        "insertLineBreak"
            | "insertParagraph"
            | "insertFromPaste"
            | "insertFromDrop"
            | "deleteContentBackward"
            | "deleteContentForward"
            | "deleteByCut"
            | "deleteByDrag"
            | "historyUndo"
            | "historyRedo"
    )
}

/// Completing a key that already has its separator and value after the caret
/// (`keep|: 99`, `"publ|": "x"`) renames just the key and keeps the value.
fn rename_key_with_value(option: &mut Completion, source: &str, cursor: usize) {
    if option.is_value || option.start > cursor {
        return;
    }
    let caret = utf16_to_byte_index(source, cursor);
    let line_rest = source[caret..].split('\n').next().unwrap_or("");
    let after_quote = line_rest.strip_prefix('"').unwrap_or(line_rest);
    let separator = after_quote.trim_start();
    if !(separator.starts_with(':') || separator.starts_with('=') && !separator.starts_with("==")) {
        return;
    }
    let quoted = option.insert.trim_start().starts_with('"');
    option.insert = if quoted {
        format!("\"{}\"", option.label)
    } else {
        option.label.clone()
    };
    // Up to the caret, plus the key's own closing quote.
    option.end = cursor
        + if quoted {
            line_rest.len() - after_quote.len()
        } else {
            0
        };
    option.cursor_offset = option.insert.encode_utf16().count();
    option.selected = 0;
}

/// The next empty value slot after UTF-16 `caret` and before `end` in
/// `source`: inside `""`, between `<k></k>`, or at the end of a line that
/// ends with a separator (`key: `, `key = `). Tab walks the slots of an
/// accepted block.
pub(super) fn next_slot(source: &str, caret: usize, end: usize) -> Option<usize> {
    let from = utf16_to_byte_index(source, caret);
    let until = utf16_to_byte_index(source, end).min(source.len());
    let region = source.get(from..until)?;
    let mut candidates = Vec::new();
    for (at, _) in region.match_indices("\"\"") {
        candidates.push(at + 1);
    }
    for (at, _) in region.match_indices("></") {
        candidates.push(at + 1);
    }
    let mut offset = 0;
    for line in region.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        if body.ends_with(": ") || body.ends_with(" = ") || body.ends_with(':') {
            candidates.push(offset + body.len());
        }
        offset += line.len();
    }
    candidates
        .into_iter()
        .filter(|at| *at > 0)
        .min()
        .map(|at| source[..from + at].encode_utf16().count())
}

/// The closers `completion` wrote after the caret (`[|]`, `"|"`), as UTF-16
/// distances from the end of `updated`, so typing them steps over them.
pub(super) fn inserted_closers(completion: &Completion, updated: &str, caret: usize) -> Vec<usize> {
    let length = updated.encode_utf16().count();
    let insert_start = caret.saturating_sub(completion.cursor_offset);
    let mut offset = insert_start;
    let mut closers = Vec::new();
    for character in completion.insert.chars() {
        if offset >= caret && matches!(character, ')' | ']' | '}' | '"' | '\'') {
            closers.push(length - offset);
        }
        offset += character.len_utf16();
    }
    closers
}

/// The text with `completion` applied, and the selection after it as
/// `(anchor, caret)`; they are equal unless a default is left selected.
pub(super) fn apply_completion(source: &str, completion: &Completion) -> (String, usize, usize) {
    let (updated, end_cursor) =
        replace_selection(source, completion.start, completion.end, &completion.insert);
    let end_offset = completion.insert.encode_utf16().count();
    let caret = end_cursor
        .saturating_sub(end_offset)
        .saturating_add(completion.cursor_offset);
    (updated, caret.saturating_sub(completion.selected), caret)
}

pub(super) fn scroll_completion_into_view(list: &web_sys::HtmlDivElement, index: usize) {
    let Some(option) = list
        .children()
        .item(index as u32)
        .and_then(|option| option.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return;
    };

    let top = option.offset_top();
    let bottom = top + option.offset_height();
    let scroll_top = list.scroll_top();
    let visible_bottom = scroll_top + list.client_height();
    if top < scroll_top {
        list.set_scroll_top(top);
    } else if bottom > visible_bottom {
        list.set_scroll_top(bottom - list.client_height());
    }
}

/// Tab width, in columns, for estimating the caret's x position without
/// measuring text; close enough for the short keys suggested.
pub(super) const COMPLETION_TAB_STOP_WIDTH: f64 = 2.0;

/// The rendered column after `text`, with tabs at their tab stops.
pub(super) fn visual_column(text: &str) -> f64 {
    let mut column = 0.0;
    for character in text.chars() {
        if character == '\t' {
            column += COMPLETION_TAB_STOP_WIDTH - (column % COMPLETION_TAB_STOP_WIDTH);
        } else {
            column += 1.0;
        }
    }
    column
}

/// Completion popup width, in pixels: room for key, type and detail.
pub(super) const COMPLETION_POPUP_WIDTH: f64 = 420.0;

/// Total horizontal gutter, in pixels, kept between the popup and the
/// viewport edges.
pub(super) const COMPLETION_POPUP_VIEWPORT_GUTTER: f64 = 16.0;

/// Height of one suggestion row, in pixels; mirrors the
/// `.config-editor__completion` rule in `preset.css`.
pub(super) const COMPLETION_ROW_HEIGHT: f64 = 44.0;

/// Vertical padding of the completion list, in pixels.
pub(super) const COMPLETION_LIST_PADDING: f64 = 2.0;

/// Completion popup height cap, in pixels (about seven rows).
pub(super) const COMPLETION_POPUP_MAX_HEIGHT: f64 = 320.0;

/// Popup style for a caret at viewport coordinates: below the caret when it
/// fits, otherwise above, clamped to the viewport.
pub(super) fn completion_popup_geometry(
    viewport_width: f64,
    viewport_height: f64,
    caret_x: f64,
    caret_y: f64,
    line_height: f64,
    option_count: usize,
) -> String {
    let width =
        COMPLETION_POPUP_WIDTH.min((viewport_width - COMPLETION_POPUP_VIEWPORT_GUTTER).max(0.0));
    let left = caret_x.clamp(
        POPUP_MARGIN,
        (viewport_width - width - POPUP_MARGIN).max(POPUP_MARGIN),
    );
    let ideal_height = (option_count as f64 * COMPLETION_ROW_HEIGHT + COMPLETION_LIST_PADDING)
        .min(COMPLETION_POPUP_MAX_HEIGHT);
    let space_below = (viewport_height - caret_y - line_height - POPUP_MARGIN).max(0.0);
    let space_above = (caret_y - POPUP_MARGIN).max(0.0);
    let (top, available_height) = if space_below >= ideal_height || space_below >= space_above {
        (caret_y + line_height, space_below)
    } else {
        ((caret_y - ideal_height).max(POPUP_MARGIN), space_above)
    };
    let max_height = ideal_height.min(available_height).max(0.0);
    format!(
        "position:fixed;left:{left:.1}px;top:{top:.1}px;width:{width:.1}px;max-height:{max_height:.1}px"
    )
}

pub(super) fn completion_popup_style(
    editor: &web_sys::HtmlTextAreaElement,
    source: &str,
    cursor: usize,
    option_count: usize,
) -> String {
    let Some(window) = web_sys::window() else {
        return "position:fixed;visibility:hidden".to_string();
    };
    let element: &web_sys::Element = editor.unchecked_ref();
    let rect = element.get_bounding_client_rect();
    let metrics = read_editor_metrics(element);
    let prefix = &source[..utf16_to_byte_index(source, cursor)];
    let (line, column) = prefix
        .rsplit_once('\n')
        .map_or((0, visual_column(prefix)), |(before, after)| {
            (before.matches('\n').count() + 1, visual_column(after))
        });
    let caret_x = rect.left() + metrics.padding_left + column * metrics.char_width
        - editor.scroll_left() as f64;
    let caret_y = rect.top() + metrics.padding_top + line as f64 * metrics.line_height
        - editor.scroll_top() as f64;
    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|width| width.as_f64())
        .unwrap_or(FALLBACK_VIEWPORT_WIDTH);
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|height| height.as_f64())
        .unwrap_or(FALLBACK_VIEWPORT_HEIGHT);
    completion_popup_geometry(
        viewport_width,
        viewport_height,
        caret_x,
        caret_y,
        metrics.line_height,
        option_count,
    )
}

/// Accepts `label` at the `|` in `marked` (as after Ctrl+Space) and returns
/// the document with `|` at the new caret.
#[cfg(test)]
#[allow(dead_code, reason = "used by the language test modules")]
pub(super) fn accept_marked(
    marked: &str,
    label: &str,
    schema: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> String {
    let at = marked.find('|').expect("a | caret marker");
    let source = marked.replacen('|', "", 1);
    let options = completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(schema),
        true,
        behavior,
    );
    let Some(option) = options.iter().find(|option| option.label == label) else {
        let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
        panic!("{label} is not offered at the caret; offered {labels:?}");
    };
    let (updated, _, caret) = apply_completion(&source, option);
    let byte = updated
        .char_indices()
        .map(|(index, _)| index)
        .chain([updated.len()])
        .find(|index| updated[..*index].encode_utf16().count() == caret)
        .expect("caret on a character boundary");
    format!("{}|{}", &updated[..byte], &updated[byte..])
}

#[cfg(test)]
mod tests {
    use super::{
        COMPLETION_LIST_PADDING, COMPLETION_POPUP_MAX_HEIGHT, COMPLETION_POPUP_WIDTH,
        COMPLETION_ROW_HEIGHT, MATCH_SUBSTRING_INDEX_PENALTY, MATCH_SUBSTRING_LENGTH_DIVISOR,
        MATCH_TIER_CASE_INSENSITIVE_PREFIX, MATCH_TIER_EXACT_PREFIX, MATCH_TIER_INITIALS,
        MATCH_TIER_SUBSEQUENCE, MATCH_TIER_SUBSTRING, completion_popup_geometry,
    };

    /// Changing these must be deliberate: the sizes mirror `preset.css`.
    #[test]
    fn ranking_tiers_and_popup_sizing_match_their_documentation() {
        assert_eq!(MATCH_TIER_EXACT_PREFIX, 1000);
        assert_eq!(MATCH_TIER_CASE_INSENSITIVE_PREFIX, 900);
        assert_eq!(MATCH_TIER_INITIALS, 800);
        assert_eq!(MATCH_TIER_SUBSTRING, 700);
        assert_eq!(MATCH_TIER_SUBSEQUENCE, 500);
        assert_eq!(MATCH_SUBSTRING_INDEX_PENALTY, 2);
        assert_eq!(MATCH_SUBSTRING_LENGTH_DIVISOR, 4);
        assert_eq!(COMPLETION_POPUP_WIDTH, 420.0);
        assert_eq!(COMPLETION_ROW_HEIGHT, 44.0);
        assert_eq!(COMPLETION_LIST_PADDING, 2.0);
        assert_eq!(COMPLETION_POPUP_MAX_HEIGHT, 320.0);
    }

    #[test]
    fn completion_popup_opens_below_caret_when_space_allows() {
        let style = completion_popup_geometry(1024.0, 768.0, 100.0, 200.0, 21.6, 3);
        // 3 options: ideal height = 3*44+2 = 134. Below: 768-200-21.6-8 = 538.4.
        assert!(
            style.contains("top:221.6px"),
            "popup should sit just below the caret line, got: {style}"
        );
        assert!(
            style.contains("left:100.0px"),
            "popup should anchor at the caret x, got: {style}"
        );
        assert!(
            style.contains("max-height:134.0px"),
            "popup should fit all options, got: {style}"
        );
    }

    #[test]
    fn completion_popup_flips_above_caret_near_viewport_bottom() {
        let style = completion_popup_geometry(1024.0, 768.0, 100.0, 700.0, 21.6, 3);
        // Below: 768-700-21.6-8 = 38.4 < 134; above: 700-8 = 692. So flip.
        assert!(
            style.contains("top:566.0px"),
            "popup should flip above the caret (700-134), got: {style}"
        );
        assert!(
            style.contains("max-height:134.0px"),
            "popup keeps full height when there is room above, got: {style}"
        );
    }

    #[test]
    fn completion_popup_clamps_to_viewport_right_edge() {
        let style = completion_popup_geometry(1024.0, 768.0, 1000.0, 200.0, 21.6, 3);
        // width = min(420, 1024-16) = 420; left clamped to 1024-420-8 = 596.
        assert!(
            style.contains("left:596.0px"),
            "popup should clamp to the right edge, got: {style}"
        );
        assert!(
            style.contains("width:420.0px"),
            "popup keeps full width when clamped, got: {style}"
        );
    }
}

#[cfg(test)]
mod slot_tests {
    use super::next_slot;

    #[test]
    fn slots_are_empty_values_after_the_caret() {
        let yaml = "rfc2136:\n  server: \n  key_name: \n  secret: \n";
        let first = yaml.find("server: ").unwrap() + "server: ".len();
        let second = next_slot(yaml, first, yaml.len()).unwrap();
        assert_eq!(&yaml[..second], "rfc2136:\n  server: \n  key_name: ");
        let json = "{\"a\": \"\", \"b\": \"\"}";
        assert_eq!(next_slot(json, 7, json.len()), Some(16));
        let xml = "<x>\n  <a></a>\n  <b></b>\n</x>";
        let after_a = xml.find("<a>").unwrap() + 3;
        assert_eq!(
            next_slot(xml, after_a, xml.len()),
            Some(xml.find("<b>").unwrap() + 3)
        );
        assert_eq!(next_slot(yaml, yaml.len() - 1, yaml.len()), None);
    }
}
