//! The host's untouched secret placeholders stand for values the editor
//! does not know: no problem judges one by its value. Not one inside it
//! (its pattern, length, format, type), and not one of the list or map
//! holding it that its value decides: a list's items told apart
//! (`uniqueItems`: the same token stands for each field's own secret),
//! how many items its `contains` schema allows, an `enum`, `const` or
//! `not` of the whole, or no `anyOf`/`oneOf` option matching when an
//! option misses only through placeholders.

use jsonschema::error::{ValidationError, ValidationErrorKind};
use serde_json::Value;

use super::Fields;

/// Whether `error` judges the value of a placeholder of `placeholders`
/// (its schema `root` read for a `minContains`).
pub(super) fn judges_placeholder(
    error: &ValidationError<'_>,
    placeholders: &Fields,
    root: &Value,
) -> bool {
    let path = error.instance_path().as_str();
    if placeholders.holds(path) {
        return true;
    }
    if !placeholders.any_within(path) {
        return false;
    }
    match error.kind() {
        ValidationErrorKind::UniqueItems => {
            repeated_item(error.instance(), path, placeholders).is_none()
        }
        ValidationErrorKind::Contains => !too_short_to_contain(error, root),
        ValidationErrorKind::Enum { .. }
        | ValidationErrorKind::Constant { .. }
        | ValidationErrorKind::Not { .. } => true,
        ValidationErrorKind::AnyOf { context } | ValidationErrorKind::OneOfNotValid { context } => {
            context.iter().any(|option| {
                option
                    .iter()
                    .all(|nested| judges_placeholder(nested, placeholders, root))
            })
        }
        _ => false,
    }
}

/// The first item of the list `items` (at `path`) written again later,
/// among the items holding no placeholder (one holding one may stand for
/// any value).
pub(super) fn repeated_item<'v>(
    items: &'v Value,
    path: &str,
    placeholders: &Fields,
) -> Option<(usize, &'v Value)> {
    let items = items.as_array()?;
    let known: Vec<(usize, &Value)> = items
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            let pointer = format!("{path}/{index}");
            !placeholders.holds(&pointer) && !placeholders.any_within(&pointer)
        })
        .collect();
    known.iter().enumerate().find_map(|(at, (index, item))| {
        known[..at]
            .iter()
            .any(|(_, earlier)| earlier == item)
            .then_some((*index, *item))
    })
}

/// Whether a `contains` problem (`minContains` when the schema has one)
/// holds whatever the placeholders stand for: the list has fewer items
/// than it needs to contain.
fn too_short_to_contain(error: &ValidationError<'_>, root: &Value) -> bool {
    let schema_path = error.schema_path().as_str();
    let (parent, keyword) = schema_path.rsplit_once('/').unwrap_or(("", schema_path));
    let needed = match keyword {
        "minContains" => root
            .pointer(parent)
            .and_then(|schema| schema.get(keyword))
            .and_then(Value::as_u64),
        "contains" => Some(1),
        // Too many items allowed: a placeholder may be one that is not.
        _ => None,
    };
    let count = error.instance().as_array().map_or(0, Vec::len) as u64;
    needed.is_some_and(|needed| count < needed)
}
