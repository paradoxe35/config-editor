use super::commands::indent_unit;
use super::diagnostics::SchemaBundle;
use super::keyboard::{replace_selection, utf16_to_byte_index};
use super::language::{CompletionContext, LanguageBehavior, SchemaPathSegment};
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
    pub(super) is_value: bool,
    /// Character-index ranges of `label` covered by the typed query, for
    /// highlighting the match in the suggestion row. Empty when the query is
    /// empty or when scoring fell through without tracking positions.
    pub(super) matched: Vec<(usize, usize)>,
}

fn schema_type_label(schema: &Value, root: &Value, fallback: &str) -> String {
    let mut labels: Vec<String> = resolve_branches(schema, root, 0)
        .into_iter()
        .filter_map(|branch| {
            if branch.get("enum").is_some() {
                return Some("enum".to_string());
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

fn type_includes(schema: &Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(schema_type)) => schema_type == expected,
        Some(Value::Array(types)) => types.iter().any(|schema_type| schema_type == expected),
        _ => false,
    }
}

pub(super) fn schema_default<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find_map(|branch| branch.get("default"))
}

pub(super) fn schema_const<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find_map(|branch| branch.get("const"))
}

pub(super) fn object_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
    resolve_branches(schema, root, 0)
        .into_iter()
        .find(|branch| {
            branch.get("properties").is_some()
                || branch.get("patternProperties").is_some()
                || branch.get("additionalProperties").is_some()
                || type_includes(branch, "object")
        })
}

pub(super) fn array_branch<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
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

fn schema_at_path<'a>(root: &'a Value, path: &[SchemaPathSegment]) -> Vec<&'a Value> {
    let mut schemas = vec![root];
    for segment in path {
        let mut next = Vec::new();
        for current in schemas {
            for branch in resolve_branches(current, root, 0) {
                if let SchemaPathSegment::ArrayItem(index) = segment {
                    if let Some(prefix_items) = branch.get("prefixItems").and_then(Value::as_array)
                        && let Some(item_schema) = prefix_items.get(*index)
                    {
                        next.push(item_schema);
                    } else if let Some(items) =
                        branch.get("items").filter(|items| items.is_object())
                    {
                        next.push(items);
                    }
                    continue;
                }
                let SchemaPathSegment::Key(key) = segment else {
                    continue;
                };
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
        schemas = next;
        if schemas.is_empty() {
            break;
        }
    }
    schemas
}

fn schema_value_candidates(branch: &Value) -> Vec<Value> {
    let mut values = branch
        .get("enum")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(value) = branch.get("const") {
        values.push(value.clone());
    }
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

/// Match-quality tiers for `match_score`, highest first: an exact-prefix hit
/// outranks a case-insensitive one, which outranks word initials (for
/// `max_connections`), which outranks a mid-label substring, which outranks a
/// greedy subsequence. Within a tier, shorter labels win via the
/// `- label_length` term; substrings additionally prefer earlier hits
/// (`- 2 * index`) while tolerating a quarter-point length penalty
/// (`- label_length / 4`) so a short late match does not beat a long early one.
pub(super) const MATCH_TIER_EXACT_PREFIX: i64 = 1000;
pub(super) const MATCH_TIER_CASE_INSENSITIVE_PREFIX: i64 = 900;
pub(super) const MATCH_TIER_INITIALS: i64 = 800;
pub(super) const MATCH_TIER_SUBSTRING: i64 = 700;
pub(super) const MATCH_TIER_SUBSEQUENCE: i64 = 500;

/// Penalty weights inside the substring tier: per-character position penalty
/// and the length divisor. Kept as named constants so the ranking trade-off
/// reads as intent rather than arithmetic.
pub(super) const MATCH_SUBSTRING_INDEX_PENALTY: i64 = 2;
pub(super) const MATCH_SUBSTRING_LENGTH_DIVISOR: i64 = 4;

/// Ranks schema names against the typed prefix. Exact prefixes win, followed by
/// case-insensitive prefixes, word initials (for `max_connections`), substrings,
/// and finally an ordered subsequence match.
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

/// Character-index ranges of `label` covered by `typed`, mirroring the
/// `match_score` tiers so the suggestion row can highlight what matched:
/// prefixes cover the leading run, initials cover each consumed word start,
/// substrings cover the found run, and subsequence matches cover the greedy
/// per-character hits. Returns an empty vector when there is no match.
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
    // Word separators are single ASCII bytes, so a running byte offset
    // tracks each part's start exactly.
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

/// Required keys sort above optional ones with the same match quality, without
/// overriding a stronger match tier.
const REQUIRED_BONUS: i64 = 40;

fn sorted_completions(mut scored: Vec<(i64, Completion)>) -> Vec<Completion> {
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.label.cmp(&right.1.label))
    });
    scored.dedup_by(|left, right| left.1.label == right.1.label);
    scored
        .into_iter()
        .map(|(_, completion)| completion)
        .collect()
}

/// Schemas often document a port as a string example (`"7835"`) next to an integer
/// branch. When the schema accepts numbers, insert the bare number.
fn numeric_example_as_number(value: Value, numeric_allowed: bool) -> Value {
    if numeric_allowed
        && let Value::String(text) = &value
        && let Ok(number) = text.parse::<i64>()
    {
        return Value::from(number);
    }
    value
}

fn value_completions(
    schemas: &[&Value],
    typed: &str,
    start: usize,
    end: usize,
    root: &Value,
    leading: &str,
    behavior: &dyn LanguageBehavior,
) -> Vec<Completion> {
    let query = unquoted(typed);
    let mut candidates = Vec::new();
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
            for value in schema_value_candidates(branch) {
                let value = numeric_example_as_number(value, numeric_allowed);
                let label = value_label(&value);
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

#[allow(clippy::too_many_arguments)]
fn property_completions(
    schemas: &[&Value],
    typed: &str,
    start: usize,
    end: usize,
    root: &Value,
    existing: Option<&HashSet<String>>,
    base_indent: &str,
    unit: &str,
    behavior: &dyn LanguageBehavior,
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
            if existing.is_some_and(|existing| existing.contains(key)) {
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
                behavior.property_insert(key, property, root, base_indent, unit)
            else {
                continue;
            };
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
                    is_value: false,
                    matched: match_ranges(key, typed),
                },
            ));
        }
    }
    sorted_completions(candidates)
}

/// Shared completion orchestration: language behavior supplies the context,
/// existing-key filter, and inserts; ranking and schema navigation stay shared.
pub(super) fn completions(
    source: &str,
    cursor: usize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
    behavior: &dyn LanguageBehavior,
) -> Vec<Completion> {
    if !enabled {
        return Vec::new();
    }
    let Some(bundle) = schema else {
        return Vec::new();
    };
    let cursor_byte = utf16_to_byte_index(source, cursor);
    let context = behavior.completion_context(source, cursor);
    let mut options = completions_for_context(source, cursor_byte, &context, force, bundle, behavior);
    if !context.entry_prefix.is_empty() || !context.entry_suffix.is_empty() {
        let prefix_units = context.entry_prefix.encode_utf16().count();
        for option in &mut options {
            option.insert.insert_str(0, context.entry_prefix);
            option.insert.push_str(context.entry_suffix);
            option.cursor_offset += prefix_units;
        }
    }
    options
}

fn completions_for_context(
    source: &str,
    cursor_byte: usize,
    context: &CompletionContext,
    force: bool,
    bundle: &SchemaBundle,
    behavior: &dyn LanguageBehavior,
) -> Vec<Completion> {
    let schemas = schema_at_path(&bundle.schema, &context.path);

    if let Some(key) = context.value_key.as_deref() {
        let value_schemas = schema_at_path(&bundle.schema, &path_with_key(&context.path, key));
        let leading = if context.value_needs_space { " " } else { "" };
        let mut options = value_completions(
            &value_schemas,
            &context.typed,
            context.replace_start,
            context.replace_end,
            &bundle.schema,
            leading,
            behavior,
        );
        if !force && unquoted(&context.typed).is_empty() {
            options.retain(|option| option.label != "null");
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
                behavior,
            );
        }
    }

    let query = unquoted(&context.typed);
    // An explicit entry start (the language saw the user begin a key) opens the
    // list even with nothing typed yet. Other empty queries stay silent.
    if query.is_empty() && !force && !context.explicit_entry_start {
        return Vec::new();
    }
    if query.contains([':', '#', '\n']) {
        return Vec::new();
    }

    let existing_keys = behavior.existing_keys(source, cursor_byte, &context.path);
    let unit = indent_unit(source);
    property_completions(
        &schemas,
        query,
        context.replace_start,
        context.replace_end,
        &bundle.schema,
        existing_keys.as_ref(),
        &context.base_indent,
        &unit,
        behavior,
    )
}

/// Accepting a key keeps the list open so its child keys can be suggested. Accepting
/// a value ends the entry, so the list closes instead of offering the same value again.
pub(super) fn accept_reopens_suggestions(completion: &Completion, auto_trigger: bool) -> bool {
    auto_trigger && !completion.is_value
}

/// Whether Tab accepts the highlighted completion instead of indenting. Accept
/// only after explicit engagement: a `Ctrl+Space` (or accept-chained) request,
/// or navigation via arrow keys/hover. An auto-triggered list must not hijack
/// Tab, which indents as in VS Code. Enter accepts whenever a list is open.
pub(super) fn should_accept_with_keyboard(explicit_request: bool, navigated: bool) -> bool {
    explicit_request || navigated
}

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

/// Returns the mapping key under `column` (a character index) on the 0-based
/// `line_index`, together with its schema documentation.
/// Shared hover orchestration: language behavior supplies the key lookup and
/// completion context; schema documentation assembly stays shared. The
pub(super) fn key_hover(
    source: &str,
    line_index: usize,
    column: usize,
    schema: Option<&SchemaBundle>,
    behavior: &dyn LanguageBehavior,
) -> Option<KeyHover> {
    let bundle = schema?;
    let line_start: usize = source
        .split('\n')
        .take(line_index)
        .map(|line| line.len() + 1)
        .sum();
    let line = source.get(line_start..)?.split('\n').next()?;
    let (key, start_column) = behavior.hover_key(line, column)?;
    let cursor = source[..line_start + behavior.hover_cursor(line, column)]
        .encode_utf16()
        .count();
    let context = behavior.completion_context(source, cursor);
    if context.value_key.as_deref() != Some(key.as_str()) {
        return None;
    }
    let root = &bundle.schema;
    let path = path_with_key(&context.path, &key);
    let schemas = schema_at_path(root, &path);
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
    let required = schema_at_path(root, &context.path)
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

pub(super) fn apply_completion(source: &str, completion: &Completion) -> (String, usize) {
    let (updated, end_cursor) =
        replace_selection(source, completion.start, completion.end, &completion.insert);
    let end_offset = completion.insert.encode_utf16().count();
    (
        updated,
        end_cursor
            .saturating_sub(end_offset)
            .saturating_add(completion.cursor_offset),
    )
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

/// Tab-stop width, in columns, for caret-x estimation. Textareas render tab
/// stops at 8 columns by default, but completion runs before the browser lays
/// out the row; using 2 keeps the popup near the visible caret for the
/// short keys this editor suggests without a canvas measurement pass.
pub(super) const COMPLETION_TAB_STOP_WIDTH: f64 = 2.0;

fn visual_column(text: &str) -> f64 {
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

/// Completion popup width, in pixels. Wide enough for the key + type +
/// detail columns (see the `padding: 5px 9px` row rule in `preset.css`)
/// without covering the whole editor.
pub(super) const COMPLETION_POPUP_WIDTH: f64 = 420.0;

/// Completion popup border thickness, in pixels: half comes from each side of
/// the two-sided clamp (`viewport_width - 16.0`), keeping the popup inside the
/// viewport with a [`POPUP_MARGIN`]-scale gutter.
pub(super) const COMPLETION_POPUP_VIEWPORT_GUTTER: f64 = 16.0;

/// Estimated height of one suggestion row, in pixels. Mirrors the
/// `.config-editor__completion` row (`12px` key + `11px` detail lines plus
/// `5px` vertical padding): a canvas-free estimate good enough to size the
/// popup before it renders.
pub(super) const COMPLETION_ROW_HEIGHT: f64 = 44.0;

/// Vertical padding inside the completion popup, in pixels: `2.0` reserves
/// the popup's own top/bottom breathing room on top of per-row padding.
pub(super) const COMPLETION_LIST_PADDING: f64 = 2.0;

/// Completion popup height cap, in pixels: shows roughly seven rows before
/// the list scrolls internally instead of growing off-viewport.
pub(super) const COMPLETION_POPUP_MAX_HEIGHT: f64 = 320.0;

/// Pure completion-popup placement: given the caret position in viewport
/// coordinates and the viewport size, decide above/below placement and clamp
/// to the viewport. No DOM access, so this is directly unit-testable.
pub(super) fn completion_popup_geometry(
    viewport_width: f64,
    viewport_height: f64,
    caret_x: f64,
    caret_y: f64,
    line_height: f64,
    option_count: usize,
) -> String {
    let width = COMPLETION_POPUP_WIDTH
        .min((viewport_width - COMPLETION_POPUP_VIEWPORT_GUTTER).max(0.0));
    let left = caret_x.clamp(
        POPUP_MARGIN,
        (viewport_width - width - POPUP_MARGIN).max(POPUP_MARGIN),
    );
    let ideal_height =
        (option_count as f64 * COMPLETION_ROW_HEIGHT + COMPLETION_LIST_PADDING)
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

#[cfg(test)]
mod tests {
    use super::{
        COMPLETION_LIST_PADDING, COMPLETION_POPUP_MAX_HEIGHT, COMPLETION_POPUP_WIDTH,
        COMPLETION_ROW_HEIGHT, MATCH_SUBSTRING_INDEX_PENALTY, MATCH_SUBSTRING_LENGTH_DIVISOR,
        MATCH_TIER_CASE_INSENSITIVE_PREFIX, MATCH_TIER_EXACT_PREFIX, MATCH_TIER_INITIALS,
        MATCH_TIER_SUBSEQUENCE, MATCH_TIER_SUBSTRING, completion_popup_geometry,
    };

    /// Pins the ranking tiers and popup sizing to their documented values so a
    /// future tweak has to update the doc comments (and CSS) deliberately.
    /// Tier order is exact > case-insensitive > initials > substring >
    /// subsequence (1000 > 900 > 800 > 700 > 500).
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

    /// Regression test: the popup opens below the caret when there is room.
    /// Behavior mirror of `peek_geometry_places_popup_below` in `diff.rs`.
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

    /// Regression test: near the bottom edge the popup flips above the caret
    /// instead of clipping off-viewport.
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

    /// Regression test: caret near the right edge clamps the popup so it
    /// stays inside the viewport (no horizontal overflow).
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
