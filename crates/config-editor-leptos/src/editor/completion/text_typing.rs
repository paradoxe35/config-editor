//! Text scalars typed by the schema: a language whose scalars are all
//! text in its syntax (`Scalars::Text`, XML) reads each one as the type
//! the schema gives its place, instead of guessing it from the text and
//! correcting the guess afterwards (a guess cannot be undone exactly:
//! `1.50` was kept as text so that it would write back the same, and
//! `<port> 80 </port>` lost its spaces).
//!
//! Where the schema says nothing about a place (or allows several types
//! there), the language's own guess applies, as without a schema.

use serde_json::{Number, Value};

use super::super::language::{ConfigLanguage, SchemaPathSegment};
use super::schema::{accepts, child_schemas, resolve_branches, type_includes};

/// Deepest nesting typed, as `conform` limits it.
const MAX_DEPTH: usize = 64;

/// Types each text scalar of `value` (as `language.parse_text` reads it)
/// by `schema`; places the schema says nothing about take the language's
/// guess. Arrays are not wrapped here (see `conform`): a lone value where
/// only a list is allowed is typed as the list's first item.
pub(crate) fn type_texts(
    value: &mut Value,
    schema: &Value,
    root: &Value,
    language: &dyn ConfigLanguage,
) {
    walk(value, &[schema], root, language, false, 0);
}

fn walk(
    value: &mut Value,
    schemas: &[&Value],
    root: &Value,
    language: &dyn ConfigLanguage,
    beside_children: bool,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        guess_all(value, language);
        return;
    }
    let branches: Vec<&Value> = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root))
        .collect();
    match value {
        Value::String(text) => {
            let scalar: Vec<&Value> = branches
                .iter()
                .copied()
                .filter(|branch| !wants_array(branch))
                .collect();
            if scalar.is_empty() && !branches.is_empty() {
                // Only a list is allowed: the text is its first item.
                let items: Vec<&Value> = schemas
                    .iter()
                    .flat_map(|schema| {
                        child_schemas(schema, root, &SchemaPathSegment::ArrayItem(0))
                    })
                    .collect();
                walk(value, &items, root, language, beside_children, depth + 1);
                return;
            }
            let text = std::mem::take(text);
            *value = typed(text, &scalar, language, beside_children);
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                let segment = SchemaPathSegment::ArrayItem(index);
                let item_schemas: Vec<&Value> = schemas
                    .iter()
                    .flat_map(|schema| child_schemas(schema, root, &segment))
                    .collect();
                walk(item, &item_schemas, root, language, false, depth + 1);
            }
        }
        Value::Object(map) => {
            let text_key = language.text_key();
            for (key, child) in map.iter_mut() {
                let segment = SchemaPathSegment::Key(key.clone());
                let child_schemas: Vec<&Value> = schemas
                    .iter()
                    .flat_map(|schema| child_schemas(schema, root, &segment))
                    .collect();
                let beside = text_key == Some(key.as_str());
                walk(child, &child_schemas, root, language, beside, depth + 1);
            }
        }
        _ => {}
    }
}

/// Every text scalar of `value` as the language guesses it (past the
/// depth typed).
fn guess_all(value: &mut Value, language: &dyn ConfigLanguage) {
    let text_key = language.text_key();
    match value {
        Value::String(text) => *value = language.guess_scalar(text),
        Value::Array(items) => items.iter_mut().for_each(|item| guess_all(item, language)),
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if text_key != Some(key.as_str()) {
                    guess_all(child, language);
                }
            }
        }
        _ => {}
    }
}

/// Whether `branch` describes a list.
fn wants_array(branch: &Value) -> bool {
    branch.get("items").is_some()
        || branch.get("prefixItems").is_some()
        || type_includes(branch, "array")
}

/// The JSON types `branch` allows a scalar to have: its `type`, else the
/// types of its `enum` values or `const`; `None` when it does not say.
fn scalar_types(branch: &Value) -> Option<Vec<&'static str>> {
    const NAMES: [&str; 6] = ["string", "integer", "number", "boolean", "null", "object"];
    let named = |name: &str| NAMES.iter().copied().find(|known| *known == name);
    match branch.get("type") {
        Some(Value::String(name)) => return Some(named(name).into_iter().collect()),
        Some(Value::Array(names)) => {
            return Some(
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(named)
                    .collect(),
            );
        }
        _ => {}
    }
    let values: Vec<&Value> = match (branch.get("enum"), branch.get("const")) {
        (Some(Value::Array(values)), _) => values.iter().collect(),
        (_, Some(value)) => vec![value],
        _ => return None,
    };
    let mut types = Vec::new();
    for value in values {
        let name = match value {
            Value::String(_) => "string",
            Value::Number(number) if number.is_f64() => "number",
            Value::Number(_) => "integer",
            Value::Bool(_) => "boolean",
            Value::Null => "null",
            _ => return None,
        };
        if !types.contains(&name) {
            types.push(name);
        }
    }
    Some(types)
}

/// `text` read as the type `branches` (none of them a list) give it.
fn typed(
    text: String,
    branches: &[&Value],
    language: &dyn ConfigLanguage,
    beside_children: bool,
) -> Value {
    if !branches.iter().any(|branch| scalar_types(branch).is_some()) {
        // The schema says nothing of the type here: the language's guess,
        // as without a schema (text beside child entries stays text).
        return if beside_children {
            Value::String(text)
        } else {
            language.guess_scalar(&text)
        };
    }
    // The types the branches that say any allow (a branch saying none, as
    // an `allOf`'s `{"minimum": 0}` beside `{"type": "number"}`, adds none).
    let mut types: Vec<&'static str> = Vec::new();
    for names in branches.iter().filter_map(|branch| scalar_types(branch)) {
        for name in names {
            if !types.contains(&name) {
                types.push(name);
            }
        }
    }
    // The texts read as `true` and `false`: those the schema lists
    // (`"x-true": ["true", "1"]`), else `true` and `false`.
    let spellings = |key: &str, plain: &'static str| -> Vec<&str> {
        let listed: Vec<&str> = branches
            .iter()
            .filter_map(|branch| branch.get(key).and_then(Value::as_array))
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if listed.is_empty() {
            vec![plain]
        } else {
            listed
        }
    };
    let (trues, falses) = (spellings("x-true", "true"), spellings("x-false", "false"));
    let space = whitespace(branches);
    let has = |name: &str| types.contains(&name);
    if types.iter().all(|name| *name == "string") && has("string") {
        return Value::String(space.apply(text));
    }
    if !has("string") {
        // Each reading the schema allows, in a fixed order; the text when
        // none reads it (validation then says so).
        let trimmed = text.trim();
        if has("null") && trimmed == "null" {
            return Value::Null;
        }
        if has("boolean") {
            if trues.contains(&trimmed) {
                return Value::Bool(true);
            }
            if falses.contains(&trimmed) {
                return Value::Bool(false);
            }
        }
        if (has("integer") || has("number"))
            && let Some(number) = number_of(trimmed, has("number"))
        {
            return Value::Number(number);
        }
        return Value::String(text);
    }
    // Several types, a string among them:
    // the language's guess, kept only where the schema allows it.
    let guessed = language.guess_scalar(&text);
    if branches.iter().any(|branch| accepts(branch, &guessed)) || !has("string") {
        guessed
    } else {
        Value::String(space.apply(text))
    }
}

/// What a schema's `x-whitespace` asks of a string's whitespace.
#[derive(Clone, Copy)]
enum Space {
    Preserve,
    /// Each tab, line feed and carriage return is a space.
    Replace,
    /// Replaced, then runs of spaces made one and the ends trimmed.
    Collapse,
}

impl Space {
    fn apply(self, text: String) -> String {
        match self {
            Space::Preserve => text,
            Space::Replace => text.replace(['\t', '\n', '\r'], " "),
            Space::Collapse => text.split_whitespace().collect::<Vec<_>>().join(" "),
        }
    }
}

/// The whitespace rule of `branches`: the strictest one any gives.
fn whitespace(branches: &[&Value]) -> Space {
    let rules = branches
        .iter()
        .filter_map(|branch| branch.get("x-whitespace").and_then(Value::as_str));
    let mut space = Space::Preserve;
    for rule in rules {
        match rule {
            "collapse" => return Space::Collapse,
            "replace" => space = Space::Replace,
            _ => {}
        }
    }
    space
}

/// `text` as a number in decimal notation: an optional sign, digits with
/// an optional fraction (or a fraction alone, `.5`), and an optional
/// exponent. Without `fractions` (an integer only), a fraction or exponent
/// must make a whole number. `None` for anything else, or an integer too
/// large to hold exactly.
pub(crate) fn number_of(text: &str, fractions: bool) -> Option<Number> {
    let bytes = text.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let digits = |at: &mut usize| {
        let start = *at;
        while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
            *at += 1;
        }
        *at - start
    };
    let whole = digits(&mut at);
    let mut fraction = 0;
    let mut dotted = false;
    if bytes.get(at) == Some(&b'.') {
        dotted = true;
        at += 1;
        fraction = digits(&mut at);
    }
    if whole + fraction == 0 {
        return None;
    }
    let mut exponent = false;
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        exponent = true;
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        if digits(&mut at) == 0 {
            return None;
        }
    }
    if at != bytes.len() {
        return None;
    }
    let unsigned = text.strip_prefix('+').unwrap_or(text);
    if !dotted && !exponent {
        if let Ok(integer) = unsigned.parse::<i64>() {
            return Some(Number::from(integer));
        }
        if let Ok(integer) = unsigned.parse::<u64>() {
            return Some(Number::from(integer));
        }
        if !fractions {
            return None;
        }
    }
    let float: f64 = unsigned.parse().ok()?;
    if !float.is_finite() || !fractions && float.fract() != 0.0 {
        return None;
    }
    Number::from_f64(float)
}
