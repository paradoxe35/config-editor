//! The validator that says why a document is invalid, at a bounded cost.
//!
//! When a value matches none of a `oneOf`'s or `anyOf`'s options, the
//! validator keeps every option's errors, and an option's errors hold the
//! errors of the options nested in it. With options that do not exclude
//! each other (each holding the same recursive `$ref`), a value deep in a
//! document is explained once through every chain of options above it:
//! three per level, so a wrong value 15 levels down was explained 3^15
//! times, ran out of memory and stopped the page.
//!
//! This validator is built from a copy of the schema where each option
//! asks first whether its value may still be explained (the [`KEYWORD`]
//! keyword: at most [`TRIES_PER_VALUE`] times for each value of the
//! document): while it may, the option is the option, with every error;
//! past that, the option is the same schema twice negated, which holds the
//! same values but says only that it does not match (an [`unexplained`]
//! error). Which values an option holds never changes, only how much is
//! said about the ones it refuses, so the work is linear in the
//! document's size.

use std::cell::RefCell;
use std::collections::HashMap;

use jsonschema::error::{ValidationError, ValidationErrorKind};
use jsonschema::{Keyword, Validator};
use serde_json::{Map, Value, json};

use crate::editor::work_count::{Work, note};

/// The keyword each option of the explaining validator asks first.
const KEYWORD: &str = "x-config-editor-explained";

/// How many times one value of a document is tried by an option for its
/// explanation: far more than a value meets in a schema with options that
/// exclude each other (a union of dozens of kinds), a bound for one that
/// meets the same options again on each level.
const TRIES_PER_VALUE: u32 = 256;

thread_local! {
    /// How many times each value (by its address) was tried by an option,
    /// during the one validation running on this thread.
    static TRIES: RefCell<HashMap<usize, u32>> = RefCell::new(HashMap::new());
}

/// The [`KEYWORD`] keyword: whether its value may still be explained.
struct Explained;

impl<'i> Keyword<'i> for Explained {
    fn validate(&self, _instance: &'i Value) -> Result<(), ValidationError<'i>> {
        Ok(())
    }

    fn is_valid(&self, instance: &'i Value) -> bool {
        note(Work::OptionTried);
        TRIES.with(|tries| {
            let mut tries = tries.borrow_mut();
            let count = tries
                .entry(std::ptr::from_ref(instance) as usize)
                .or_insert(0);
            *count += 1;
            *count <= TRIES_PER_VALUE
        })
    }
}

/// The explaining validator for `schema` (as the main validator is built
/// from it), or `None` when it does not build (the main one then explains,
/// unbounded). A draft-04 or -06 schema is read as draft-07, the first
/// draft with `if`, when it has no keyword draft-07 reads otherwise.
pub(super) fn build(schema: &Value) -> Option<Validator> {
    let mut copy = schema.clone();
    if let Some(Value::String(draft)) = copy.get_mut("$schema") {
        let newer: &[&str] = if draft.contains("draft-04") {
            &[
                "id",
                "const",
                "contains",
                "propertyNames",
                "if",
                "then",
                "else",
            ]
        } else if draft.contains("draft-06") {
            &["if", "then", "else"]
        } else {
            &[]
        };
        if !newer.is_empty() {
            // A keyword the draft ignores (or reads otherwise, as draft-04
            // `id`) that draft-07 would check: not read as draft-07.
            if mentions_key(schema, newer) {
                return None;
            }
            *draft = "http://json-schema.org/draft-07/schema#".to_string();
        }
    }
    wrap(&mut copy, "");
    jsonschema::options()
        .with_keyword(KEYWORD, |_, _, _| Ok(Box::new(Explained)))
        .build(&copy)
        .ok()
}

/// Whether some object in `value` has one of `keys` (a property of that
/// name too: a draft is only read as another when that cannot matter).
fn mentions_key(value: &Value, keys: &[&str]) -> bool {
    match value {
        Value::Object(members) => members
            .iter()
            .any(|(key, member)| keys.contains(&key.as_str()) || mentions_key(member, keys)),
        Value::Array(items) => items.iter().any(|item| mentions_key(item, keys)),
        _ => false,
    }
}

/// The errors of `value` under `validator` (an explaining one), each
/// value of the document explained a bounded number of times.
pub(super) fn errors<'v>(validator: &'v Validator, value: &'v Value) -> Vec<ValidationError<'v>> {
    TRIES.with(|tries| tries.borrow_mut().clear());
    let errors = validator.iter_errors(value).collect();
    // Not kept between validations: the addresses mean nothing after.
    TRIES.with(|tries| *tries.borrow_mut() = HashMap::new());
    errors
}

/// Keywords whose value is data, not schemas: left as written.
const DATA: [&str; 4] = ["const", "enum", "default", "examples"];

/// Keywords whose value maps names to schemas.
const NAMED: [&str; 6] = [
    "properties",
    "patternProperties",
    "$defs",
    "definitions",
    "dependentSchemas",
    "dependencies",
];

/// Wraps each option of each `oneOf` and `anyOf` in `value` (a schema or
/// a list of them, at the JSON pointer `at` of its resource):
/// `{"if": {KEYWORD: true}, "then": option, "else": {"not": {"not": option}}}`,
/// the `else` naming the option by a `$ref` (a copy would double the
/// schema at each level of options).
fn wrap(value: &mut Value, at: &str) {
    match value {
        Value::Object(members) => {
            // A schema with its own `$id` starts a resource: `#/…` inside
            // it is from there.
            let starts_resource = members
                .get("$id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty() && !id.starts_with('#'));
            let at = if starts_resource { "" } else { at };
            for (key, member) in members.iter_mut() {
                let here = format!("{at}/{}", escape(key));
                if DATA.contains(&key.as_str()) {
                    continue;
                }
                if NAMED.contains(&key.as_str()) {
                    if let Value::Object(named) = member {
                        for (name, schema) in named.iter_mut() {
                            wrap(schema, &format!("{here}/{}", escape(name)));
                        }
                    }
                    continue;
                }
                match member {
                    Value::Array(options)
                        if matches!(key.as_str(), "anyOf" | "oneOf")
                            && options
                                .iter()
                                .all(|option| option.is_object() || option.is_boolean()) =>
                    {
                        for (index, option) in options.iter_mut().enumerate() {
                            let then = format!("{here}/{index}/then");
                            wrap(option, &then);
                            *option = wrapped(std::mem::take(option), &then);
                        }
                    }
                    _ => wrap(member, &here),
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                wrap(item, &format!("{at}/{index}"));
            }
        }
        _ => {}
    }
}

/// `option`, once placed at the pointer `then`, asking [`KEYWORD`] first.
fn wrapped(option: Value, then: &str) -> Value {
    let mut members = Map::new();
    members.insert("if".to_string(), json!({ KEYWORD: true }));
    members.insert("then".to_string(), option);
    members.insert(
        "else".to_string(),
        json!({"not": {"not": {"$ref": format!("#{}", fragment(then))}}}),
    );
    Value::Object(members)
}

/// `schema` (a part of the explaining validator's schema, as a `not`
/// error holds it) as it was written: each option without its wrapper.
pub(super) fn unwrapped(schema: &Value) -> std::borrow::Cow<'_, Value> {
    if !schema.to_string().contains(KEYWORD) {
        return std::borrow::Cow::Borrowed(schema);
    }
    let mut copy = schema.clone();
    unwrap(&mut copy);
    std::borrow::Cow::Owned(copy)
}

fn unwrap(value: &mut Value) {
    if let Value::Object(members) = value
        && members
            .get("if")
            .and_then(|test| test.get(KEYWORD))
            .is_some()
        && let Some(option) = members.remove("then")
    {
        *value = option;
        unwrap(value);
        return;
    }
    match value {
        Value::Object(members) => members.values_mut().for_each(unwrap),
        Value::Array(items) => items.iter_mut().for_each(unwrap),
        _ => {}
    }
}

/// `segment` escaped for a JSON pointer.
fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// The JSON pointer `pointer` written as a URI fragment: each character a
/// fragment may not hold percent-encoded.
fn fragment(pointer: &str) -> String {
    let mut written = String::with_capacity(pointer.len());
    for byte in pointer.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&byte) {
            written.push(byte as char);
        } else {
            written.push_str(&format!("%{byte:02X}"));
        }
    }
    written
}

/// The segments of a schema path that make an option of the explaining
/// validator's schema: `anyOf`/`oneOf`, the option's index, and `then`
/// (explained) or `else` (not).
fn option_at<'s>(segments: &[&'s str]) -> Option<&'s str> {
    match segments {
        [keyword, index, branch, ..]
            if matches!(*keyword, "anyOf" | "oneOf")
                && !index.is_empty()
                && index.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            Some(branch)
        }
        _ => None,
    }
}

/// `path`, a schema path of the explaining validator, as a path of the
/// schema it was built from: without each option's `then`.
pub(super) fn original_path(path: &str) -> std::borrow::Cow<'_, str> {
    if !path.contains("/then") {
        return std::borrow::Cow::Borrowed(path);
    }
    let segments: Vec<&str> = path.split('/').collect();
    let mut kept = Vec::with_capacity(segments.len());
    let mut index = 0;
    while index < segments.len() {
        kept.push(segments[index]);
        if option_at(&segments[index..]) == Some("then") {
            kept.push(segments[index + 1]);
            index += 3;
        } else {
            index += 1;
        }
    }
    std::borrow::Cow::Owned(kept.join("/"))
}

/// Whether `error` is an option that refused its value without saying
/// why: its value was explained enough times elsewhere.
pub(super) fn unexplained(error: &ValidationError<'_>) -> bool {
    if !matches!(error.kind(), ValidationErrorKind::Not { .. }) {
        return false;
    }
    let segments: Vec<&str> = error.schema_path().as_str().rsplit('/').take(4).collect();
    matches!(
        segments.as_slice(),
        ["not", "else", index, "anyOf" | "oneOf"]
            if !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
    )
}

#[cfg(test)]
mod tests {
    use super::{fragment, original_path};

    #[test]
    fn an_explained_options_path_reads_as_the_schemas() {
        assert_eq!(
            original_path("/$defs/node/anyOf/0/then/properties/all/items/anyOf/2/then/type"),
            "/$defs/node/anyOf/0/properties/all/items/anyOf/2/type"
        );
        // An option's own `then` stays.
        assert_eq!(
            original_path("/oneOf/1/then/then/minimum"),
            "/oneOf/1/then/minimum"
        );
        // A property named `anyOf` is no option.
        assert_eq!(
            original_path("/properties/anyOf/then/type"),
            "/properties/anyOf/then/type"
        );
    }

    #[test]
    fn a_pointer_is_written_as_a_fragment() {
        assert_eq!(fragment("/properties/a b/%"), "/properties/a%20b/%25");
        assert_eq!(fragment("/$defs/x~1y"), "/$defs/x~1y");
    }
}
