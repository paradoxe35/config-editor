//! How a schema problem reads: the validator's error in the editor's words
//! (the strings tables where a host can translate them), a secret field's
//! value never quoted (nor one inside the value told, a secret item of a
//! list or field of an item) and a long value abbreviated. A value that matches
//! none of a `oneOf`'s or `anyOf`'s options, or more than one of a
//! `oneOf`'s, names the options (their titles) and says what each misses.

use std::collections::{BTreeSet, HashSet};
use std::ops::Range;

use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::Value;
use unicode_segmentation::GraphemeCursor;

use super::fields::Secrets;
use super::{Explanations, Fields, PlaceholderPattern};
use crate::EditorStrings;
use crate::strings::fill;

/// What stands for a secret inside a value while it is written as JSON,
/// then replaced by the strings' "the secret value": a private-use
/// character JSON writes as it is, in no text a document holds.
const SECRET_MARK: &str = "\u{F8FF}secret\u{F8FF}";

/// Longest value quoted whole in a message; a longer one is cut with `…`.
const MAX_QUOTED_VALUE: usize = 80;

/// Most options a "matches none of the options" message gives reasons for.
const MAX_OPTIONS_LISTED: usize = 5;

/// Most levels of options (an option's problem being that a value in it
/// matches none of its own options) a message explains; deeper ones are
/// named only, so a message stays short however deep the options go.
const MAX_EXPLAINED_LEVELS: usize = 3;

/// Longest description that names an untitled option; a longer one is cut.
const MAX_OPTION_NAME: usize = 40;

/// What the wording reads beside the error.
pub(in crate::editor) struct Wording<'a> {
    /// The schema the validator was built from (`$ref`s as pointers).
    pub root: &'a Value,
    pub strings: &'a EditorStrings,
    /// The secrets: a value one of them or inside one is never quoted,
    /// and one holding one is quoted with each hidden.
    pub secrets: &'a Secrets<'a>,
    /// The host's placeholders: an item holding one is told repeated in
    /// a list by no message (see `placeholders`).
    pub placeholders: &'a Fields,
    /// The placeholders a cut value never ends inside: the host's tokens
    /// and what the placeholder patterns find.
    pub tokens: &'a Tokens<'a>,
    /// How many levels of options the message being worded is inside.
    pub levels: std::cell::Cell<usize>,
    /// Each option's first explanation, for the places it was not
    /// explained again.
    pub explanations: &'a Explanations<'a>,
}

impl Wording<'_> {
    /// `error` worded, its value hidden when secret (each secret inside
    /// it hidden) and cut when long.
    pub(in crate::editor) fn message(&self, error: &ValidationError<'_>) -> String {
        let strings = self.strings;
        // Written as JSON only when the message quotes it.
        let value = std::cell::LazyCell::new(|| self.value(error));
        match error.kind() {
            ValidationErrorKind::AnyOf { .. } | ValidationErrorKind::OneOfNotValid { .. } => {
                let keyword = if matches!(error.kind(), ValidationErrorKind::AnyOf { .. }) {
                    "anyOf"
                } else {
                    "oneOf"
                };
                let branches = self.explanations.branches(error).unwrap_or_default();
                expected_types(error, &branches, &value, strings)
                    .or_else(|| self.none_matches(error, &branches, keyword))
                    .unwrap_or_else(|| self.validators_words(error))
            }
            ValidationErrorKind::OneOfMultipleValid { context } => {
                self.several_match(error, context)
            }
            // Every allowed value, not "or 2 other candidates".
            ValidationErrorKind::Enum { options } => match options.as_array() {
                Some(options) => fill(
                    &strings.not_one_of,
                    &[
                        ("value", &*value),
                        (
                            "allowed",
                            &options
                                .iter()
                                .map(Value::to_string)
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    ],
                ),
                None => self.validators_words(error),
            },
            ValidationErrorKind::Type { .. } => {
                let message = error.to_string();
                listed_types(&message)
                    .map(|types| {
                        fill(
                            &strings.wrong_type,
                            &[("value", &*value), ("types", &types.join(", "))],
                        )
                    })
                    .unwrap_or_else(|| self.validators_words(error))
            }
            ValidationErrorKind::Format { format } => fill(
                &strings.not_format,
                &[
                    ("value", &*value),
                    ("format", &Value::from(format.as_str())),
                ],
            ),
            // The pattern as JSON writes it: a line break in it escaped.
            ValidationErrorKind::Pattern { pattern } => format!(
                "{} does not match {}",
                *value,
                Value::from(pattern.as_str())
            ),
            ValidationErrorKind::FalseSchema => super::super::schema::false_schema_message(&value),
            ValidationErrorKind::Contains => self.contains(error),
            // Counts, not the whole list or object printed.
            ValidationErrorKind::MinItems { limit } => {
                self.counted(error, &strings.too_few_items, *limit)
            }
            ValidationErrorKind::MaxItems { limit } => {
                self.counted(error, &strings.too_many_items, *limit)
            }
            ValidationErrorKind::MinProperties { limit } => {
                self.counted(error, &strings.too_few_keys, *limit)
            }
            ValidationErrorKind::MaxProperties { limit } => {
                self.counted(error, &strings.too_many_keys, *limit)
            }
            ValidationErrorKind::UniqueItems => self.repeated(error),
            ValidationErrorKind::Not { schema } => {
                let schema = &*super::explanation::unwrapped(schema);
                match self.kept_apart(error, schema) {
                    Some(keys) => {
                        let quoted: Vec<String> = keys
                            .into_iter()
                            .map(|key| Value::from(key).to_string())
                            .collect();
                        match &quoted[..] {
                            [key, other] => fill(
                                &strings.keys_used_together,
                                &[("key", key), ("other", other)],
                            ),
                            _ => fill(
                                &strings.keys_all_used_together,
                                &[("keys", &quoted.join(", "))],
                            ),
                        }
                    }
                    None => self.refused_by_not(error, schema, &value),
                }
            }
            _ => self.validators_words(error),
        }
    }

    /// The value of `error` as a message quotes it: "the secret value"
    /// for a secret field's, each secret inside it hidden, cut when long.
    fn value(&self, error: &ValidationError<'_>) -> String {
        self.value_at(error.instance_path().as_str(), error.instance())
    }

    /// `value` (at `pointer`) as a message quotes it: "the secret value"
    /// for a secret field's, each secret inside it hidden, cut when long.
    pub(in crate::editor) fn value_at(&self, pointer: &str, value: &Value) -> String {
        if self.secrets.holds(pointer, value) {
            return self.strings.secret_value.clone();
        }
        if !self.secrets.touches(pointer, value) {
            return cut(&value.to_string(), self.tokens);
        }
        let mark = Value::from(SECRET_MARK);
        let hidden = self.secrets.hidden(pointer, value, &mark);
        cut(
            &hidden
                .to_string()
                .replace(&mark.to_string(), &self.strings.secret_value),
            self.tokens,
        )
    }

    /// Whether the value of `error` is a secret or holds one.
    fn holds_secret(&self, error: &ValidationError<'_>) -> bool {
        self.secrets
            .touches(error.instance_path().as_str(), error.instance())
    }

    /// The validator's own message for `error`, the value it starts with
    /// quoted as [`Self::value`] quotes it.
    fn validators_words(&self, error: &ValidationError<'_>) -> String {
        let message = error.to_string();
        let instance = error.instance().to_string();
        let value = if self.holds_secret(error) {
            self.value(error)
        } else {
            cut(&instance, self.tokens)
        };
        if value == instance {
            message
        } else {
            message.replacen(&instance, &value, 1)
        }
    }

    /// A value a `not` (`schema`) refuses, in plain words where the `not`
    /// is one keyword: a type, a `const` or `enum`, keys refused one by
    /// one (an `anyOf` of single `required` keys); else the schema as
    /// JSON. A value holding a secret is not told the values or pattern
    /// it matches: they would tell what the secret is.
    fn refused_by_not(&self, error: &ValidationError<'_>, schema: &Value, value: &str) -> String {
        let strings = self.strings;
        let instance = error.instance();
        let quoted = |names: Vec<String>| {
            names
                .iter()
                .map(|name| Value::from(name.as_str()).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        // A `not` every value matches (`not: {}`, `not: true`): no value
        // is allowed, as a `false` schema says.
        let annotation = |keyword: &str| matches!(keyword, "title" | "description" | "$comment");
        if *schema == Value::Bool(true)
            || schema
                .as_object()
                .is_some_and(|members| members.keys().all(|keyword| annotation(keyword)))
        {
            return super::super::schema::false_schema_message(value);
        }
        // A `type` beside a `const` or `enum` adds nothing to their names.
        let only = schema.as_object().and_then(|members| {
            let named = members.contains_key("const") || members.contains_key("enum");
            let mut keywords = members
                .iter()
                .filter(|(keyword, _)| !annotation(keyword) && !(named && *keyword == "type"));
            let one = keywords.next()?;
            keywords.next().is_none().then_some(one)
        });
        let secret = self.holds_secret(error);
        let worded = match only {
            Some((keyword, _)) if secret && matches!(keyword.as_str(), "pattern" | "enum") => None,
            Some((keyword, pattern @ Value::String(_))) if keyword == "pattern" => Some(fill(
                &strings.must_not_match_pattern,
                &[("value", &value), ("pattern", pattern)],
            )),
            Some((keyword, Value::String(name))) if keyword == "type" => Some(fill(
                &strings.must_not_be_type,
                &[("value", &value), ("types", &quoted(vec![name.clone()]))],
            )),
            Some((keyword, Value::Array(names))) if keyword == "type" => {
                let names: Option<Vec<String>> = names
                    .iter()
                    .map(|name| name.as_str().map(str::to_string))
                    .collect();
                names.map(|names| {
                    fill(
                        &strings.must_not_be_type,
                        &[("value", &value), ("types", &quoted(names))],
                    )
                })
            }
            // The value is the one refused: not said twice (`3 must not be 3`).
            Some((keyword, _)) if keyword == "const" => None,
            Some((keyword, Value::Array(refused))) if keyword == "enum" && refused.len() > 1 => {
                Some(fill(
                    &strings.must_not_be,
                    &[
                        ("value", &value),
                        (
                            "values",
                            &refused
                                .iter()
                                .map(Value::to_string)
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    ],
                ))
            }
            Some((keyword, Value::Array(options))) if keyword == "anyOf" => {
                let keys: Option<Vec<&str>> = options
                    .iter()
                    .map(
                        |option| match option.get("required")?.as_array()?.as_slice() {
                            [key] if option.as_object()?.len() == 1 => key.as_str(),
                            _ => None,
                        },
                    )
                    .collect();
                let held: Vec<String> = keys
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|key| instance.get(key).is_some())
                    .map(str::to_string)
                    .collect();
                (!held.is_empty()).then(|| {
                    fill(
                        &strings.must_not_have,
                        &[("value", &value), ("keys", &quoted(held))],
                    )
                })
            }
            _ => None,
        };
        worded.unwrap_or_else(|| {
            fill(
                &strings.must_not_match,
                &[("value", &value), ("schema", schema)],
            )
        })
    }

    /// The keys a `not` (`schema`) refuses written together, when that is
    /// all it refuses: the set the object holds whole, of the `not`'s own
    /// or, under a key's dependent schema, of that key and the ones the
    /// `not` keeps apart from it.
    fn kept_apart(&self, error: &ValidationError<'_>, schema: &Value) -> Option<Vec<String>> {
        use crate::editor::completion::{dependent_keys_kept_apart, keys_kept_apart};
        let schema_path = super::explanation::original_path(error.schema_path().as_str());
        let mut segments = schema_path.rsplit('/');
        let trigger = match (segments.next(), segments.next(), segments.next()) {
            (Some("not"), Some(trigger), Some("dependentSchemas" | "dependencies")) => {
                Some(crate::editor::language::unescaped_key(trigger))
            }
            _ => None,
        };
        let sets = match &trigger {
            Some(trigger) => dependent_keys_kept_apart(trigger, schema, self.root)?,
            None => keys_kept_apart(schema, self.root)?,
        };
        let written = error.instance();
        let written = written.as_object();
        let whole = |set: &&Vec<&str>| {
            written.is_some_and(|object| set.iter().all(|key| object.contains_key(*key)))
        };
        let set = sets.iter().find(whole).or(sets.first())?;
        Some(set.iter().map(|key| key.to_string()).collect())
    }

    /// A list `contains` (with its `minContains` or `maxContains`, the
    /// keyword the validator names) refuses: how many of its items the
    /// `contains` schema allows, against how many it wants.
    fn contains(&self, error: &ValidationError<'_>) -> String {
        let schema_path = super::explanation::original_path(error.schema_path().as_str());
        let (parent, keyword) = schema_path.rsplit_once('/').unwrap_or(("", &schema_path));
        let limit = self
            .root
            .pointer(parent)
            .and_then(|schema| schema.get(keyword));
        match (keyword, limit) {
            ("minContains", Some(limit)) => {
                fill(&self.strings.too_few_contained, &[("limit", limit)])
            }
            ("maxContains", Some(limit)) => {
                fill(&self.strings.too_many_contained, &[("limit", limit)])
            }
            _ => fill(&self.strings.no_item_contained, &[]),
        }
    }

    /// A list or object with too few or too many items or keys: `text`
    /// with its count and the `limit`.
    fn counted(&self, error: &ValidationError<'_>, text: &str, limit: u64) -> String {
        let count = match &**error.instance() {
            Value::Array(items) => items.len(),
            Value::Object(members) => members.len(),
            _ => 0,
        };
        fill(
            text,
            &[
                ("limit", &Value::from(limit)),
                ("count", &Value::from(count)),
            ],
        )
    }

    /// A list holding an item more than once: the first item repeated
    /// (of those holding no placeholder), hidden when secret and cut when
    /// long.
    fn repeated(&self, error: &ValidationError<'_>) -> String {
        let path = error.instance_path().as_str();
        let instance = error.instance();
        let shown = super::placeholders::repeated_item(instance, path, self.placeholders)
            .map(|(index, item)| self.value_at(&format!("{path}/{index}"), item))
            .unwrap_or_default();
        fill(&self.strings.repeated_item, &[("item", &shown)])
    }

    /// "matches none of the oneOf options: Cloudflare needs "cloudflare";
    /// Route 53 needs "route53"": each option the value could be meant for
    /// (not one wanting another type) with its first problem, a missing
    /// key before an unexpected one before anything else.
    fn none_matches(
        &self,
        error: &ValidationError<'_>,
        branches: &[&[ValidationError<'_>]],
        keyword: &str,
    ) -> Option<String> {
        let path = error.instance_path().as_str();
        let names = self.option_names(error, branches.len());
        let typed = super::text_for_typed_option(error, branches);
        let levels = self.levels.get();
        self.levels.set(levels + 1);
        let mut reasons: Vec<String> = branches
            .iter()
            .zip(&names)
            .filter(|(branch, _)| typed || !super::wrong_type(branch, path))
            .filter_map(|(branch, name)| {
                // Past the levels explained, or where the explanation did
                // not reach: the option's name alone.
                if levels >= MAX_EXPLAINED_LEVELS
                    || branch.iter().any(super::explanation::unexplained)
                {
                    return Some(name.clone());
                }
                let errors: Vec<&ValidationError<'_>> = branch
                    .iter()
                    .flat_map(|nested| super::specific_errors(self.explanations, nested))
                    .collect();
                let at_path =
                    |error: &&&ValidationError<'_>| error.instance_path().as_str() == path;
                let first = errors
                    .iter()
                    .filter(at_path)
                    .find(|error| matches!(error.kind(), ValidationErrorKind::Required { .. }))
                    .or_else(|| {
                        errors
                            .iter()
                            .filter(at_path)
                            .find(|error| unexpected(error).is_some())
                    })
                    .or_else(|| errors.first())?;
                Some(self.reason(name, first, path))
            })
            .collect();
        self.levels.set(levels);
        if reasons.is_empty() {
            return None;
        }
        if reasons.len() > MAX_OPTIONS_LISTED {
            reasons.truncate(MAX_OPTIONS_LISTED);
            reasons.push("…".to_string());
        }
        Some(fill(
            &self.strings.no_option_matches,
            &[("keyword", &keyword), ("reasons", &reasons.join("; "))],
        ))
    }

    /// Why the option `name` does not take the value at `path`: `error`,
    /// the first of its problems.
    fn reason(&self, name: &str, error: &ValidationError<'_>, path: &str) -> String {
        let strings = self.strings;
        let at = error.instance_path().as_str();
        if at == path
            && let ValidationErrorKind::Required { property } = error.kind()
        {
            let key = property
                .as_str()
                .map_or_else(|| property.to_string(), str::to_string);
            return fill(&strings.option_needs, &[("option", &name), ("key", &key)]);
        }
        if at == path
            && let Some(key) = unexpected(error)
        {
            return fill(&strings.option_refuses, &[("option", &name), ("key", &key)]);
        }
        // A key whose schema is `false`.
        if let ValidationErrorKind::FalseSchema = error.kind()
            && let Some(key) = at
                .strip_prefix(path)
                .and_then(|rest| rest.strip_prefix('/'))
                .filter(|key| !key.contains('/'))
        {
            let key = crate::editor::language::unescaped_key(key);
            return fill(&strings.option_refuses, &[("option", &name), ("key", &key)]);
        }
        let message = self.message(error);
        let inner = at.strip_prefix(path).unwrap_or(at);
        let problem = if inner.is_empty() {
            message
        } else {
            format!("{inner}: {message}")
        };
        fill(
            &strings.option_problem,
            &[("option", &name), ("problem", &problem)],
        )
    }

    /// "matches more than one of the oneOf options (Cloudflare, Route 53)":
    /// the options the value matches (those without errors).
    fn several_match(
        &self,
        error: &ValidationError<'_>,
        branches: &[Vec<ValidationError<'_>>],
    ) -> String {
        let names = self.option_names(error, branches.len());
        let matched: Vec<&str> = branches
            .iter()
            .zip(&names)
            .filter(|(branch, _)| branch.is_empty())
            .map(|(_, name)| name.as_str())
            .collect();
        fill(
            &self.strings.several_options_match,
            &[("options", &matched.join(", "))],
        )
    }

    /// The names of the `count` options of the combinator `error` is about:
    /// each one's title, else the start of its description, else "option
    /// 2" (through a `$ref` when the option is one); "option 2" too for a
    /// name several options share, which tells none of them apart.
    fn option_names(&self, error: &ValidationError<'_>, count: usize) -> Vec<String> {
        let options = self
            .root
            .pointer(&super::explanation::original_path(
                error.schema_path().as_str(),
            ))
            .and_then(Value::as_array)
            .filter(|options| options.len() == count);
        let named: Vec<Option<String>> = (0..count)
            .map(|index| options.and_then(|options| self.named(&options[index])))
            .collect();
        named
            .iter()
            .enumerate()
            .map(|(index, name)| {
                name.clone()
                    .filter(|name| {
                        named
                            .iter()
                            .flatten()
                            .filter(|other| *other == name)
                            .count()
                            == 1
                    })
                    .unwrap_or_else(|| {
                        fill(&self.strings.option_number, &[("number", &(index + 1))])
                    })
            })
            .collect()
    }

    /// The title or the start of the description of `option`, or of the
    /// schema its `$ref` names.
    fn named(&self, option: &Value) -> Option<String> {
        let mut schemas = vec![option];
        let mut schema = option;
        while let Some(target) = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| super::super::completion::ref_target(reference, self.root))
        {
            if schemas.iter().any(|seen| std::ptr::eq(*seen, target)) {
                break;
            }
            schemas.push(target);
            schema = target;
        }
        let text = |name: &str| {
            schemas.iter().find_map(|schema| {
                schema
                    .get(name)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
            })
        };
        if let Some(title) = text("title") {
            return Some(title.to_string());
        }
        let description = text("description")?.lines().next()?.trim();
        Some(
            match cut_point(description, MAX_OPTION_NAME, &Tokens::default()) {
                Some(end) => format!("{}…", description[..end].trim_end()),
                None => description.to_string(),
            },
        )
    }
}

/// The first key `error` says its object may not have.
fn unexpected(error: &ValidationError<'_>) -> Option<String> {
    match error.kind() {
        ValidationErrorKind::AdditionalProperties { unexpected }
        | ValidationErrorKind::UnevaluatedProperties { unexpected } => unexpected.first().cloned(),
        _ => None,
    }
}

/// For an `anyOf` whose every branch wants another type for the value:
/// `true is not of type "integer", "string", "null"`, instead of "not
/// valid under any of the schemas" (or the problems of the one option of
/// text, `1.5` for `integer` or `"auto"`).
fn expected_types(
    error: &ValidationError<'_>,
    branches: &[&[ValidationError<'_>]],
    value: &str,
    strings: &EditorStrings,
) -> Option<String> {
    let types = expected_type_names(error, branches)?;
    Some(fill(
        &strings.wrong_type,
        &[("value", &value), ("types", &types.join(", "))],
    ))
}

impl Wording<'_> {
    /// Whether `error` (an `anyOf` or a `oneOf` none of whose options match)
    /// is only that every option wants another type for the value: a problem
    /// with the value, not with which keys an object has.
    pub(in crate::editor) fn every_option_wants_another_type(
        &self,
        error: &ValidationError<'_>,
    ) -> bool {
        self.explanations
            .branches(error)
            .is_some_and(|branches| expected_type_names(error, &branches).is_some())
    }
}

/// The types the options of [`expected_types`] want, when each wants
/// another type for the value.
fn expected_type_names(
    error: &ValidationError<'_>,
    branches: &[&[ValidationError<'_>]],
) -> Option<Vec<String>> {
    let path = error.instance_path().as_str();
    let mut types: Vec<String> = Vec::new();
    for branch in branches {
        let mut typed = false;
        for nested in super::own_errors(branch, path)
            .into_iter()
            .filter(|nested| {
                matches!(nested.kind(), ValidationErrorKind::Type { .. })
                    && nested.instance_path().as_str() == path
            })
        {
            typed = true;
            for name in listed_types(&nested.to_string())? {
                if !types.contains(&name) {
                    types.push(name);
                }
            }
        }
        if !typed {
            return None;
        }
    }
    (!types.is_empty()).then_some(types)
}

/// The types a validator's type message lists: `"integer"` and `"string"`
/// from `true is not of types "integer", "string"`.
pub(super) fn listed_types(message: &str) -> Option<Vec<String>> {
    let listed = message
        .split_once(" is not of type")
        .map(|(_, rest)| rest)?;
    let listed = listed.strip_prefix('s').unwrap_or(listed);
    Some(
        listed
            .split(',')
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .collect(),
    )
}

/// Texts a cut never ends inside (half a token on the page is a piece of
/// one, which P1 forbids): the host's placeholder tokens, looked up by
/// their lengths rather than searched for one by one, and what a
/// placeholder pattern finds (another document's token pasted here).
#[derive(Default)]
pub(in crate::editor) struct Tokens<'p> {
    texts: HashSet<String>,
    lengths: BTreeSet<usize>,
    patterns: &'p [PlaceholderPattern],
}

impl<'p> Tokens<'p> {
    pub(super) fn new<'t>(
        texts: impl IntoIterator<Item = &'t str>,
        patterns: &'p [PlaceholderPattern],
    ) -> Self {
        let texts: HashSet<String> = texts
            .into_iter()
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .collect();
        let lengths = texts.iter().map(String::len).collect();
        Self {
            texts,
            lengths,
            patterns,
        }
    }

    /// The byte ranges of `text` the patterns find.
    fn found(&self, text: &str) -> Vec<Range<usize>> {
        self.patterns
            .iter()
            .flat_map(|pattern| pattern(text))
            .collect()
    }

    /// The start of the first token of `text` (`found` holding what the
    /// patterns find there) that byte `at` falls strictly inside.
    fn around(&self, text: &str, at: usize, found: &[Range<usize>]) -> Option<usize> {
        let known = self.lengths.iter().filter_map(|&length| {
            (at.saturating_sub(length - 1)..at).find(|&start| {
                text.get(start..start + length)
                    .is_some_and(|piece| self.texts.contains(piece))
            })
        });
        let matched = found
            .iter()
            .filter(|range| range.start < at && at < range.end)
            .map(|range| range.start);
        known.chain(matched).min()
    }
}

/// `text` (a value as JSON), cut with `…` past [`MAX_QUOTED_VALUE`]
/// characters, before a token or a character it would split.
pub(super) fn cut(text: &str, tokens: &Tokens<'_>) -> String {
    match cut_point(text, MAX_QUOTED_VALUE, tokens) {
        Some(end) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// Where `text` longer than `max` characters is cut: at most `max`
/// characters in, never inside one of `tokens` nor inside a character as
/// the user sees it. `None` when it is not longer.
fn cut_point(text: &str, max: usize, tokens: &Tokens<'_>) -> Option<usize> {
    let (mut end, _) = text.char_indices().nth(max)?;
    let found = tokens.found(text);
    loop {
        let before = grapheme_floor(text, tokens.around(text, end, &found).unwrap_or(end));
        if before == end {
            return Some(end);
        }
        end = before;
    }
}

/// The last place at or before byte `at` (a character's start) of `text`
/// where a character as the user sees it starts.
fn grapheme_floor(text: &str, at: usize) -> usize {
    let mut cursor = GraphemeCursor::new(at, text.len(), true);
    match cursor.is_boundary(text, 0) {
        Ok(false) => cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0),
        _ => at,
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_QUOTED_VALUE, Tokens, cut};

    /// A long value is cut before a placeholder it would split (the host's
    /// own, or one a placeholder pattern finds), and never inside a
    /// character drawn as one (P1: no piece of a token shows).
    #[test]
    fn a_cut_value_keeps_its_tokens_and_characters_whole() {
        let token = "«secret 3f2a»";
        let other = "«secret 9b1c»";
        let pattern: super::PlaceholderPattern = Box::new(move |text: &str| {
            text.match_indices(other)
                .map(|(at, found)| at..at + found.len())
                .collect()
        });
        let patterns = [pattern];
        let tokens = Tokens::new([token], &patterns);
        let pieces = [
            token,
            other,
            "👨\u{200d}👩\u{200d}👧",
            "e\u{301}",
            "🇫🇷",
            "日",
            "\u{1112}\u{1161}\u{11ab}",
            "क्षि",
            "\u{0995}\u{09cd}\u{09b7}",
            "\u{0e01}\u{0e33}",
        ];
        for piece in pieces {
            let width = piece.chars().count();
            for lead in MAX_QUOTED_VALUE.saturating_sub(width + 1)..=MAX_QUOTED_VALUE {
                let head = "a".repeat(lead);
                let text = format!("{head}{piece}{}", "b".repeat(20));
                let shown = cut(&text, &tokens);
                let kept = shown.strip_suffix('…').expect("a long value is cut");
                assert!(text.starts_with(kept), "{shown:?}");
                assert!(kept.chars().count() <= MAX_QUOTED_VALUE, "{shown:?}");
                assert!(
                    kept.len() <= head.len() || kept.len() >= head.len() + piece.len(),
                    "{piece:?} split in {shown:?}"
                );
            }
        }
        // The token is kept when it fits, and a short value is not cut.
        let text = format!("{token}{}", "b".repeat(100));
        assert!(cut(&text, &tokens).starts_with(token));
        assert_eq!(cut(token, &tokens), token);
    }

    /// A problem quoting a long value holding one of the host's
    /// placeholders, in a field it does not stand for, never shows part of
    /// it (P1).
    #[cfg(feature = "yaml")]
    #[test]
    fn a_problem_never_quotes_part_of_a_placeholder() {
        let token = "«secret 3f2a»";
        let schema =
            crate::editor::schema::Schema::compile(r#"{"properties": {"a": {"type": "string"}}}"#)
                .expect("compiles");
        let accepted = [(token.to_string(), "/b".to_string())];
        // For some of these leads the token starts before the cut and ends
        // after it.
        for lead in 57..80 {
            let text = format!("a:\n  note: {}{token}\nb: {token}\n", "x".repeat(lead));
            let problems = super::super::validate_with(
                &text,
                Some(&schema.bundle),
                None,
                true,
                crate::editor::Language::YAML.behavior(),
                &accepted,
                crate::strings::default_strings(),
            );
            assert!(
                problems.iter().any(|problem| problem.message.contains('…')),
                "{problems:#?}"
            );
            for problem in &problems {
                let marks = problem.message.matches('«').count();
                assert_eq!(
                    marks,
                    problem.message.matches(token).count(),
                    "{problem:#?}"
                );
            }
        }
    }
}
