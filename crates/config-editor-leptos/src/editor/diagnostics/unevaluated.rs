//! What `unevaluatedProperties` and `unevaluatedItems` really refuse.
//!
//! A value that fails inside an applicator (`allOf`, `$ref`, `if`, …) makes
//! that whole subschema fail, and a failing subschema evaluates nothing: the
//! validator then refuses every key it declares (a type mistake under a
//! Compose service's `healthcheck` would refuse `ports` and `healthcheck`).
//! Beside such an error only the keys (or items) no applied subschema could
//! evaluate are refused; the error itself says what is wrong. An object
//! without other errors keeps the validator's answer (a `oneOf` branch that
//! did not match evaluates nothing, rightly).

use std::collections::HashSet;

use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::Value;

use super::super::completion::{branches_for, item_schema, named_properties, schema_at_path_in};
use super::super::language::SchemaPathSegment;

pub(super) struct Unevaluated<'a> {
    root: &'a Value,
    document: &'a Value,
    repeats_arrays: bool,
    /// Where the other errors are.
    failing: HashSet<String>,
}

impl<'a> Unevaluated<'a> {
    pub(super) fn new<'e, 'v: 'e>(
        root: &'a Value,
        document: &'a Value,
        repeats_arrays: bool,
        errors: impl IntoIterator<Item = &'e ValidationError<'v>>,
    ) -> Self {
        let failing = errors
            .into_iter()
            .filter(|error| {
                !matches!(
                    error.kind(),
                    ValidationErrorKind::UnevaluatedProperties { .. }
                        | ValidationErrorKind::UnevaluatedItems { .. }
                )
            })
            .map(|error| error.instance_path().as_str().to_string())
            .collect();
        Self {
            root,
            document,
            repeats_arrays,
            failing,
        }
    }

    /// Whether another error is at `path` or inside its value.
    fn beside_errors(&self, path: &str) -> bool {
        self.failing.iter().any(|failing| {
            failing
                .strip_prefix(path)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
    }

    /// The branches applying to the value at `path` (its conditionals
    /// decided by the value), with that value.
    fn branches(&self, path: &str) -> (Vec<&'a Value>, Option<&'a Value>) {
        let segments = SchemaPathSegment::from_pointer(path);
        let instance = self.document.pointer(path);
        let branches = schema_at_path_in(
            self.root,
            &segments,
            self.repeats_arrays,
            Some(self.document),
        )
        .into_iter()
        .flat_map(|schema| branches_for(schema, self.root, instance))
        .collect();
        (branches, instance)
    }

    /// The keys of `unexpected` (refused at `path`) to report.
    pub(super) fn refused_keys<'k>(&self, path: &str, unexpected: &'k [String]) -> Vec<&'k String> {
        if !self.beside_errors(path) {
            return unexpected.iter().collect();
        }
        let (branches, _) = self.branches(path);
        unexpected
            .iter()
            .filter(|key| {
                !branches.iter().any(|branch| {
                    !named_properties(branch, key).is_empty()
                        || branch
                            .get("additionalProperties")
                            .is_some_and(|additional| additional != &Value::Bool(false))
                })
            })
            .collect()
    }

    /// Whether the array at `path`, refused items, has an item no applied
    /// subschema could evaluate.
    pub(super) fn refuses_items(&self, path: &str) -> bool {
        if !self.beside_errors(path) {
            return true;
        }
        let (branches, instance) = self.branches(path);
        let length = instance.and_then(Value::as_array).map_or(0, Vec::len);
        (0..length).any(|index| {
            !branches
                .iter()
                .any(|branch| item_schema(branch, index).is_some())
        })
    }
}
