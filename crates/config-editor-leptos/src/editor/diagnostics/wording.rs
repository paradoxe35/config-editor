//! How a schema problem reads: the validator's error in the editor's words
//! (the strings tables where a host can translate them), a secret field's
//! value never quoted and a long value abbreviated. A value that matches
//! none of a `oneOf`'s or `anyOf`'s options, or more than one of a
//! `oneOf`'s, names the options (their titles) and says what each misses.

use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::Value;

use crate::EditorStrings;
use crate::strings::fill;

/// Longest value quoted whole in a message; a longer one is cut with `…`.
const MAX_QUOTED_VALUE: usize = 80;

/// Most options a "matches none of the options" message gives reasons for.
const MAX_OPTIONS_LISTED: usize = 5;

/// Longest description that names an untitled option; a longer one is cut.
const MAX_OPTION_NAME: usize = 40;

/// What the wording reads beside the error.
pub(in crate::editor) struct Wording<'a> {
    /// The schema the validator was built from (`$ref`s as pointers).
    pub root: &'a Value,
    pub strings: &'a EditorStrings,
    /// Whether the value at a pointer is a secret field's (or in one).
    pub secret: &'a dyn Fn(&str) -> bool,
}

impl Wording<'_> {
    /// `error` worded, its value hidden when secret and cut when long.
    pub(in crate::editor) fn message(&self, error: &ValidationError<'_>) -> String {
        self.shown(error, self.worded(error))
    }

    fn worded(&self, error: &ValidationError<'_>) -> String {
        let strings = self.strings;
        match error.kind() {
            ValidationErrorKind::AnyOf { context } => expected_types(error, context, strings)
                .or_else(|| self.none_matches(error, context, "anyOf"))
                .unwrap_or_else(|| error.to_string()),
            ValidationErrorKind::OneOfNotValid { context } => {
                expected_types(error, context, strings)
                    .or_else(|| self.none_matches(error, context, "oneOf"))
                    .unwrap_or_else(|| error.to_string())
            }
            ValidationErrorKind::OneOfMultipleValid { context } => {
                self.several_match(error, context)
            }
            // Every allowed value, not "or 2 other candidates".
            ValidationErrorKind::Enum { options } => match options.as_array() {
                Some(options) => fill(
                    &strings.not_one_of,
                    &[
                        ("value", &error.instance()),
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
                None => error.to_string(),
            },
            ValidationErrorKind::Type { .. } => {
                let message = error.to_string();
                listed_types(&message)
                    .map(|types| {
                        fill(
                            &strings.wrong_type,
                            &[("value", &error.instance()), ("types", &types.join(", "))],
                        )
                    })
                    .unwrap_or(message)
            }
            ValidationErrorKind::Format { format } => fill(
                &strings.not_format,
                &[
                    ("value", &error.instance()),
                    ("format", &Value::from(format.as_str())),
                ],
            ),
            // The pattern as JSON writes it: a line break in it escaped.
            ValidationErrorKind::Pattern { pattern } => format!(
                "{} does not match {}",
                error.instance(),
                Value::from(pattern.as_str())
            ),
            ValidationErrorKind::FalseSchema => {
                super::super::false_schema::message(&error.instance().to_string())
            }
            ValidationErrorKind::Contains => self.contains(error),
            ValidationErrorKind::Not { schema } => {
                match crate::editor::completion::keys_kept_apart(schema, self.root) {
                    Some(keys) => {
                        let quoted: Vec<String> = keys
                            .iter()
                            .map(|key| Value::from(*key).to_string())
                            .collect();
                        fill(
                            &strings.keys_used_together,
                            &[("key", &quoted[0]), ("others", &quoted[1..].join(", "))],
                        )
                    }
                    None => fill(
                        &strings.must_not_match,
                        &[("value", &error.instance()), ("schema", schema)],
                    ),
                }
            }
            _ => error.to_string(),
        }
    }

    /// A list `contains` (with its `minContains` or `maxContains`, the
    /// keyword the validator names) refuses: how many of its items the
    /// `contains` schema allows, against how many it wants.
    fn contains(&self, error: &ValidationError<'_>) -> String {
        let schema_path = error.schema_path().as_str();
        let (parent, keyword) = schema_path.rsplit_once('/').unwrap_or(("", schema_path));
        let limit = self
            .root
            .pointer(parent)
            .and_then(|schema| schema.get(keyword));
        let value = error.instance();
        match (keyword, limit) {
            ("minContains", Some(limit)) => fill(
                &self.strings.too_few_contained,
                &[("value", &value), ("limit", limit)],
            ),
            ("maxContains", Some(limit)) => fill(
                &self.strings.too_many_contained,
                &[("value", &value), ("limit", limit)],
            ),
            _ => fill(&self.strings.no_item_contained, &[("value", &value)]),
        }
    }

    /// `message` about `error` with its value hidden when it is a secret
    /// field's, and abbreviated rather than quoted whole when long.
    fn shown(&self, error: &ValidationError<'_>, message: String) -> String {
        let instance = error.instance().to_string();
        if (self.secret)(error.instance_path().as_str()) {
            message.replace(&instance, &self.strings.secret_value)
        } else if instance.chars().count() > MAX_QUOTED_VALUE {
            let short: String = instance.chars().take(MAX_QUOTED_VALUE).collect();
            message.replacen(&instance, &format!("{short}…"), 1)
        } else {
            message
        }
    }

    /// "matches none of the oneOf options: Cloudflare needs "cloudflare";
    /// Route 53 needs "route53"": each option the value could be meant for
    /// (not one wanting another type) with its first problem, a missing
    /// key before an unexpected one before anything else.
    fn none_matches(
        &self,
        error: &ValidationError<'_>,
        branches: &[Vec<ValidationError<'_>>],
        keyword: &str,
    ) -> Option<String> {
        let path = error.instance_path().as_str();
        let names = self.option_names(error, branches.len());
        let typed = super::text_for_typed_option(error, branches);
        let mut reasons: Vec<String> = branches
            .iter()
            .zip(&names)
            .filter(|(branch, _)| typed || !super::wrong_type(branch, path))
            .filter_map(|(branch, name)| {
                let errors: Vec<&ValidationError<'_>> =
                    branch.iter().flat_map(super::specific_errors).collect();
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
            let key = key.replace("~1", "/").replace("~0", "~");
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
            .pointer(error.schema_path().as_str())
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
        Some(if description.chars().count() > MAX_OPTION_NAME {
            let short: String = description.chars().take(MAX_OPTION_NAME).collect();
            format!("{}…", short.trim_end())
        } else {
            description.to_string()
        })
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
    branches: &[Vec<ValidationError<'_>>],
    strings: &EditorStrings,
) -> Option<String> {
    let types = expected_type_names(error, branches)?;
    Some(fill(
        &strings.wrong_type,
        &[("value", &error.instance()), ("types", &types.join(", "))],
    ))
}

/// Whether `error` (an `anyOf` or a `oneOf` none of whose options match)
/// is only that every option wants another type for the value: a problem
/// with the value, not with which keys an object has.
pub(in crate::editor) fn every_option_wants_another_type(error: &ValidationError<'_>) -> bool {
    match error.kind() {
        ValidationErrorKind::AnyOf { context } | ValidationErrorKind::OneOfNotValid { context } => {
            expected_type_names(error, context).is_some()
        }
        _ => false,
    }
}

/// The types the options of [`expected_types`] want, when each wants
/// another type for the value.
fn expected_type_names(
    error: &ValidationError<'_>,
    branches: &[Vec<ValidationError<'_>>],
) -> Option<Vec<String>> {
    let path = error.instance_path().as_str();
    let mut types: Vec<String> = Vec::new();
    for branch in branches {
        let mut typed = false;
        for nested in branch.iter().filter(|nested| {
            matches!(nested.kind(), ValidationErrorKind::Type { .. })
                && nested.instance_path().as_str() == path
        }) {
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
