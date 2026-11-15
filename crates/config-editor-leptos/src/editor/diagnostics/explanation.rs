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
//! asks first whether it may explain its value (the [`KEYWORD`] keyword):
//! each option explains each value of the document once, the first time
//! the validator collects its errors. Then the option is the option, with
//! every error; after, the option is the same schema twice negated, which
//! holds the same values but says only that it does not match (an
//! [`unexplained`] error; `specific::Explanations` reads the first
//! explanation for it). Which values an option holds never changes, only
//! how much is said about the ones it refuses, so the work is linear in
//! the document's size, however many chains of options reach a value.
//!
//! The validator also asks an option whether it holds a value without
//! collecting errors (an `anyOf` first asks whether any option holds it);
//! those answers explain nothing. So the option is
//! `{"allOf": [{MARK: n}, {"if": {KEYWORD: n}, "then": …, "else": …}]}`:
//! [`MARK`] is only read when errors are collected, right before
//! [`KEYWORD`] (see [`Asked`]); [`LEFT`], right after the option's
//! errors.
//!
//! A `$ref` target reached again for the same value is explained once in
//! each option's explanation too (see [`refs`]), and every explanation
//! spends a budget linear in the document (see [`bound`]): past it, the
//! problems say they are incomplete. A schema this validator cannot be
//! built from is explained by [`bounded`], under the same budget.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use jsonschema::error::{ValidationError, ValidationErrorKind};
use jsonschema::{Keyword, Validator};
use serde_json::{Map, Value, json};

#[cfg(test)]
use crate::editor::work_count::note_many;
use crate::editor::work_count::{Work, note};

mod bound;
mod refs;

pub(super) use refs::is_stop;

/// The keyword each option of the explaining validator asks first.
const KEYWORD: &str = "x-config-editor-explained";

/// The keyword read right before [`KEYWORD`] when errors are collected.
const MARK: &str = "x-config-editor-asked";

/// The keyword read right after an option's errors are collected.
const LEFT: &str = "x-config-editor-left";

/// An option (its number in the schema) and a value of the document (its
/// address).
type Asking = (u64, usize);

/// A `$ref`'s definition (its number), a value of the document (its
/// address), and the option explaining it (`None`: the document).
type Referred = (u64, usize, Option<Asking>);

thread_local! {
    /// The option and value whose errors are being collected, between
    /// [`MARK`] and [`KEYWORD`].
    static ASKED: Cell<Option<Asking>> = const { Cell::new(None) };
    /// Each option and value explained, during the one validation running
    /// on this thread.
    static EXPLAINED: RefCell<HashSet<Asking>> = RefCell::new(HashSet::new());
    /// The options whose errors are being collected, the innermost last.
    static EXPLAINING: RefCell<Vec<Asking>> = const { RefCell::new(Vec::new()) };
    /// Each `$ref` target and value explained, in each option's
    /// explanation, during the one validation running on this thread.
    static REFERRED: RefCell<HashSet<Referred>> = RefCell::new(HashSet::new());
}

/// The number an option's keywords hold.
fn option_number(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

/// The [`MARK`] keyword: the errors of its option are being collected
/// (the validator reads a keyword's `validate` only then).
struct Asked(u64);

impl<'i> Keyword<'i> for Asked {
    fn validate(&self, instance: &'i Value) -> Result<(), ValidationError<'i>> {
        let value = std::ptr::from_ref(instance) as usize;
        ASKED.with(|asked| asked.set(Some((self.0, value))));
        Ok(())
    }

    fn is_valid(&self, _instance: &'i Value) -> bool {
        true
    }
}

/// The [`KEYWORD`] keyword: whether its option explains its value. Only
/// when the option's errors are collected (see [`Asked`]) and the option
/// never explained the value before.
struct Explained(u64);

impl<'i> Keyword<'i> for Explained {
    fn validate(&self, _instance: &'i Value) -> Result<(), ValidationError<'i>> {
        Ok(())
    }

    fn is_valid(&self, instance: &'i Value) -> bool {
        note(Work::OptionTried);
        let asking = (self.0, std::ptr::from_ref(instance) as usize);
        if ASKED.with(Cell::take) != Some(asking) {
            // Only whether the option holds the value: either answer does.
            return true;
        }
        EXPLAINING.with(|explaining| explaining.borrow_mut().push(asking));
        if EXPLAINED.with(|explained| explained.borrow().contains(&asking)) || !bound::spend() {
            return false;
        }
        EXPLAINED.with(|explained| explained.borrow_mut().insert(asking))
    }
}

/// The [`LEFT`] keyword: its option's errors are collected.
struct Left;

impl<'i> Keyword<'i> for Left {
    fn validate(&self, _instance: &'i Value) -> Result<(), ValidationError<'i>> {
        EXPLAINING.with(|explaining| explaining.borrow_mut().pop());
        Ok(())
    }

    fn is_valid(&self, _instance: &'i Value) -> bool {
        true
    }
}

/// The [`refs::KEYWORD`] keyword: whether its `$ref`'s target explains its
/// value. Only when its errors are collected (see [`Asked`]), for a value
/// that is an object or a list, and the target never explained the value
/// before in the explanation of the same option (and the budget allows
/// it).
struct Referring(u64);

impl<'i> Keyword<'i> for Referring {
    fn validate(&self, _instance: &'i Value) -> Result<(), ValidationError<'i>> {
        Ok(())
    }

    fn is_valid(&self, instance: &'i Value) -> bool {
        let value = std::ptr::from_ref(instance) as usize;
        if ASKED.with(Cell::take) != Some((self.0, value)) {
            return true;
        }
        if !(instance.is_object() || instance.is_array()) {
            return true;
        }
        let option = EXPLAINING.with(|explaining| explaining.borrow().last().copied());
        let referred = (self.0, value, option);
        if REFERRED.with(|known| known.borrow().contains(&referred)) || !bound::spend() {
            return false;
        }
        REFERRED.with(|known| known.borrow_mut().insert(referred))
    }
}

/// The explaining validator for `schema` (as the main validator is built
/// from it), or `None` when it does not build (then [`bounded`]
/// explains). A draft-04 or -06 schema is read as draft-07, the first
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
    let mut count = 0;
    wrap(&mut copy, "", &mut count);
    refs::wrap(&mut copy, &mut count, refs::Form::Explained);
    jsonschema::options()
        .with_keyword(MARK, |_, value, _| {
            Ok(Box::new(Asked(option_number(value))))
        })
        .with_keyword(KEYWORD, |_, value, _| {
            Ok(Box::new(Explained(option_number(value))))
        })
        .with_keyword(LEFT, |_, _, _| Ok(Box::new(Left)))
        .with_keyword(refs::KEYWORD, |_, value, _| {
            Ok(Box::new(Referring(option_number(value))))
        })
        .with_keyword(refs::SPENT, |_, value, _| {
            Ok(Box::new(bound::Spent(option_number(value))))
        })
        .build(&copy)
        .ok()
}

/// The validator that says why a document is invalid for a schema the
/// explaining one cannot be built from (in its own draft, which may have
/// no `if`): each `$ref` target explained each time it is reached, while
/// the budget lasts (see [`refs::Form::Stopping`]). `None` when it does
/// not build either.
pub(super) fn bounded(schema: &Value) -> Option<Validator> {
    let mut copy = schema.clone();
    refs::wrap(&mut copy, &mut 0, refs::Form::Stopping);
    jsonschema::options()
        .with_keyword(MARK, |_, value, _| {
            Ok(Box::new(Asked(option_number(value))))
        })
        .with_keyword(refs::STOP, |_, value, _| {
            Ok(Box::new(bound::Stop(option_number(value))))
        })
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

/// The errors of `value` under `validator` (an explaining one, or a
/// [`bounded`] one), each value of the document explained once by each
/// option, and whether they are all its errors: not when the budget ran
/// out (see [`bound`]).
pub(super) fn errors<'v>(
    validator: &'v Validator,
    value: &'v Value,
) -> (Vec<ValidationError<'v>>, bool) {
    bound::start(value);
    EXPLAINED.with(|explained| explained.borrow_mut().clear());
    REFERRED.with(|referred| referred.borrow_mut().clear());
    EXPLAINING.with(|explaining| explaining.borrow_mut().clear());
    ASKED.with(|asked| asked.set(None));
    // A bounded validator's options before `$ref`s, at the top: their
    // target's errors in their place, as the validator gives them.
    let errors = without_stops(validator.iter_errors(value));
    #[cfg(test)]
    note_many(Work::ErrorCollected, collected(&errors));
    // Not kept between validations: the addresses mean nothing after.
    EXPLAINED.with(|explained| *explained.borrow_mut() = HashSet::new());
    REFERRED.with(|referred| *referred.borrow_mut() = HashSet::new());
    (errors, !bound::stopped())
}

/// `errors`, each option a bounded validator adds before a `$ref` (see
/// [`is_stop`]) replaced by its target's errors (inside other options,
/// `specific` reads them so).
fn without_stops<'v>(
    errors: impl IntoIterator<Item = ValidationError<'v>>,
) -> Vec<ValidationError<'v>> {
    let mut kept = Vec::new();
    for error in errors {
        if !is_stop(&error) {
            kept.push(error);
            continue;
        }
        if let ValidationErrorKind::AnyOf { mut context } = error.into_parts().kind {
            kept.extend(without_stops(context.swap_remove(1)));
        }
    }
    kept
}

/// How many errors `errors` holds, its options' errors included.
#[cfg(test)]
fn collected(errors: &[ValidationError<'_>]) -> usize {
    errors
        .iter()
        .map(|error| {
            1 + match error.kind() {
                ValidationErrorKind::AnyOf { context }
                | ValidationErrorKind::OneOfNotValid { context } => {
                    context.iter().map(|branch| collected(branch)).sum()
                }
                _ => 0,
            }
        })
        .sum()
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
/// a list of them, at the JSON pointer `at` of its resource), numbering
/// them on from `count` (see [`wrapped`]).
fn wrap(value: &mut Value, at: &str, count: &mut u64) {
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
                            wrap(schema, &format!("{here}/{}", escape(name)), count);
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
                            let then = format!("{here}/{index}/allOf/1/then");
                            wrap(option, &then, count);
                            *count += 1;
                            *option = wrapped(std::mem::take(option), &then, *count);
                        }
                    }
                    _ => wrap(member, &here, count),
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                wrap(item, &format!("{at}/{index}"), count);
            }
        }
        _ => {}
    }
}

/// `option` (the option numbered `number`), once placed at the pointer
/// `then`, asking [`KEYWORD`] first:
/// `{"allOf": [{MARK: number}, {"if": {KEYWORD: number}, "then": option,
/// "else": {"not": {"not": option}}}, {LEFT: number}]}`, the `else`
/// naming the option by a `$ref` (a copy would double the schema at each
/// level of options).
fn wrapped(option: Value, then: &str, number: u64) -> Value {
    let mut asking = Map::new();
    asking.insert("if".to_string(), json!({ KEYWORD: number }));
    asking.insert("then".to_string(), option);
    asking.insert(
        "else".to_string(),
        json!({"not": {"not": {"$ref": format!("#{}", fragment(then))}}}),
    );
    json!({"allOf": [{ MARK: number }, Value::Object(asking), { LEFT: number }]})
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
    if let Some(option) = value
        .get_mut("allOf")
        .and_then(|all| all.get_mut(1))
        .filter(|asking| {
            asking
                .get("if")
                .and_then(|test| test.get(KEYWORD))
                .is_some()
        })
        .and_then(|asking| asking.get_mut("then"))
    {
        *value = std::mem::take(option);
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
/// validator's schema: `anyOf`/`oneOf`, the option's index, `allOf/1`
/// (see [`wrapped`]) and `then` (explained) or `else` (not).
fn option_at<'s>(segments: &[&'s str]) -> Option<&'s str> {
    match segments {
        [keyword, index, "allOf", "1", branch, ..]
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
/// schema it was built from: without each option's `allOf/1/then`.
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
            index += 5;
        } else {
            index += 1;
        }
    }
    std::borrow::Cow::Owned(kept.join("/"))
}

/// Whether `error` is an option that refused its value without saying
/// why: it explained the value before.
pub(super) fn unexplained(error: &ValidationError<'_>) -> bool {
    if !matches!(error.kind(), ValidationErrorKind::Not { .. }) {
        return false;
    }
    let segments: Vec<&str> = error.schema_path().as_str().rsplit('/').take(6).collect();
    matches!(
        segments.as_slice(),
        ["not", "else", "1", "allOf", index, "anyOf" | "oneOf"]
            if !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{bounded, build, errors, fragment, original_path};

    /// Both validators stop at the budget for a node whose recursion runs
    /// only through `$recursiveRef`, three members deep at each level.
    #[test]
    fn a_recursion_through_recursive_ref_stops_at_the_budget_in_both_validators() {
        let member =
            |key: &str| json!({"required": [key], "properties": {"c": {"$recursiveRef": "#"}}});
        let schema = json!({"$schema": "https://json-schema.org/draft/2019-09/schema",
            "$recursiveAnchor": true, "allOf": [member("m"), member("d"), member("e")]});
        let mut document = json!({});
        for _ in 0..16 {
            document = json!({ "c": document });
        }
        for validator in [build(&schema).unwrap(), bounded(&schema).unwrap()] {
            let (found, complete) = errors(&validator, &document);
            assert!(!complete && !found.is_empty() && found.len() < 200_000);
        }
    }

    #[test]
    fn an_explained_options_path_reads_as_the_schemas() {
        assert_eq!(
            original_path(
                "/$defs/node/anyOf/0/allOf/1/then/properties/all/items/anyOf/2/allOf/1/then/type"
            ),
            "/$defs/node/anyOf/0/properties/all/items/anyOf/2/type"
        );
        // An option's own `then` stays.
        assert_eq!(
            original_path("/oneOf/1/allOf/1/then/then/minimum"),
            "/oneOf/1/then/minimum"
        );
        // A property named `anyOf` is no option.
        assert_eq!(
            original_path("/properties/anyOf/then/type"),
            "/properties/anyOf/then/type"
        );
        // An option's own `allOf` stays.
        assert_eq!(
            original_path("/anyOf/0/allOf/1/then/allOf/1/then/type"),
            "/anyOf/0/allOf/1/then/type"
        );
    }

    #[test]
    fn a_pointer_is_written_as_a_fragment() {
        assert_eq!(fragment("/properties/a b/%"), "/properties/a%20b/%25");
        assert_eq!(fragment("/$defs/x~1y"), "/$defs/x~1y");
    }
}
