//! Reading a document the way the schema expects, for languages that write
//! arrays by repeating their key (a lone value where only a list is allowed
//! is a one-item list) and XML, whose values are all text: its text is read
//! as the type of an option it fits (`<workers>4</workers>` is the number
//! for "an integer or `auto`", `<v>1.3</v>` the string a `const` lists).

use super::*;

/// Reads `value` the way `schema` expects for a language that writes arrays
/// by repeating their key: each lone value (or, for blocks, lone object)
/// where only an array is allowed becomes a one-item array (a single XML
/// `<dns>` element is `dns: [...]`), and XML text is typed by the schema.
pub(crate) fn conform_arrays(value: &mut Value, schema: &Value, repetition: Repetition) {
    conform(value, &AtPath::Schema(schema), schema, repetition, 0);
}

/// [`conform_arrays`] where `at` is what applies to `value` (every
/// subschema at its path combined): read as they allow.
pub(super) fn conform(
    value: &mut Value,
    at: &AtPath<'_>,
    root: &Value,
    repetition: Repetition,
    depth: usize,
) {
    if depth > 64 || at.leaves().is_empty() {
        return;
    }
    // An object decides the conditionals it is under (`if kind is a`).
    let applied = at.applied(value.is_object().then_some(&*value), root);
    let branches = applied.branches();
    let wants_array = branches.iter().any(|branch| {
        branch.get("items").is_some()
            || branch.get("prefixItems").is_some()
            || type_includes(branch, "array")
    });
    if repetition == Repetition::Elements {
        read_text(value, at, &branches, root);
    }
    let repeatable = match repetition {
        Repetition::Elements => !value.is_array(),
        Repetition::Blocks => value.is_object(),
        _ => false,
    };
    if wants_array && repeatable && !branches.iter().any(|branch| accepts(branch, value)) {
        *value = Value::Array(vec![value.take()]);
    }
    // Each child read with every schema that applies to it there.
    let segments: Vec<SchemaPathSegment> = match &*value {
        Value::Array(items) => (0..items.len()).map(SchemaPathSegment::ArrayItem).collect(),
        Value::Object(map) => map.keys().cloned().map(SchemaPathSegment::Key).collect(),
        _ => return,
    };
    let children: Vec<AtPath<'_>> = segments
        .iter()
        .map(|segment| applied.child(segment))
        .collect();
    match value {
        Value::Array(items) => {
            for (item, at) in items.iter_mut().zip(&children) {
                conform(item, at, root, repetition, depth + 1);
            }
        }
        Value::Object(map) => {
            for (child, at) in map.values_mut().zip(&children) {
                conform(child, at, root, repetition, depth + 1);
            }
        }
        _ => {}
    }
}

/// XML text (`value`) read as the schema wants it: a number, boolean or
/// `null` the parser read is the text it is written as where the schema
/// wants that ([`read_as_text`]); text that is one in another spelling
/// (`1.50`, `1e3`, ` 4 `) is that value where the schema wants one and not
/// the text; and empty text (an empty element, read as `{}`, or an empty
/// attribute) is the empty string where the schema wants text, else
/// `null` where it allows that.
fn read_text(value: &mut Value, at: &AtPath<'_>, branches: &[&Value], root: &Value) {
    let allowed = |candidate: &Value| {
        branches.iter().any(|branch| {
            accepts(branch, candidate)
                || listed(branch).any(|listed| listed == candidate)
                || whole(candidate).is_some_and(|whole| accepts(branch, &whole))
        })
    };
    let empty = match value {
        Value::Number(_) | Value::Bool(_) | Value::Null => {
            let text = scalar_text(value);
            if read_as_text(value, &text, at, branches, root) {
                *value = Value::String(text);
            }
            return;
        }
        Value::String(text) => {
            // Text the schema takes as it is stays text (`007` for "an
            // integer or a string").
            let fits = |candidate: &Value| at.decides(candidate, root) == Some(true);
            if let Some(typed) = text_literal(text)
                && allowed(&typed)
                && !fits(&Value::String(text.clone()))
                && !read_as_text(&typed, text, at, branches, root)
            {
                *value = typed;
                return;
            }
            text.is_empty()
        }
        Value::Object(map) => map.is_empty(),
        Value::Array(_) => false,
    };
    if !empty || allowed(value) {
        return;
    }
    if branches
        .iter()
        .any(|branch| type_includes(branch, "string"))
    {
        *value = Value::String(String::new());
    } else if allowed(&Value::Null) {
        *value = Value::Null;
    }
}

/// The number, boolean or `null` XML text writes in any spelling a reader
/// takes for one (`1.50`, `.5`, `1e3`, `007`, `+4`, ` true `).
fn text_literal(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value @ (Value::Bool(_) | Value::Null)) => Some(value),
        _ => crate::editor::decimal_text::decimal(trimmed, true),
    }
}

/// A float with no fraction (`1e3`) as the integer it is, which an
/// `integer` type allows.
fn whole(value: &Value) -> Option<Value> {
    let number = value.as_number().filter(|number| number.is_f64())?;
    let float = number.as_f64()?;
    (float.fract() == 0.0 && float.abs() < 9.0e15).then(|| Value::from(float as i64))
}

/// Whether XML text read as a number, boolean or `null` (`value`) is the
/// string it is written as (`text`): where a string the schema lists is
/// that text (`const: "1.3"`), else where no option `value` fits but its
/// text does (`oneOf [integer ≥ 10, string]` for `4`), else where no option
/// allows its kind and text is allowed (`type: string`, or only strings
/// listed).
fn read_as_text(
    value: &Value,
    text: &str,
    at: &AtPath<'_>,
    branches: &[&Value],
    root: &Value,
) -> bool {
    let text = Value::String(text.to_string());
    let listed: Vec<&Value> = branches.iter().flat_map(|branch| listed(branch)).collect();
    if listed.contains(&&text) && !listed.contains(&value) {
        return true;
    }
    let fits = |candidate: &Value| at.decides(candidate, root) == Some(true);
    if fits(value) {
        return false;
    }
    if fits(&text) {
        return true;
    }
    let allowed = listed.contains(&value) || branches.iter().any(|branch| accepts(branch, value));
    !allowed
        && (branches
            .iter()
            .any(|branch| type_includes(branch, "string"))
            || (!listed.is_empty() && listed.iter().all(|allowed| allowed.is_string())))
}

/// The values `branch` lists as the only ones allowed (`const`, `enum`).
pub(super) fn listed(branch: &Value) -> impl Iterator<Item = &Value> {
    branch.get("const").into_iter().chain(
        branch
            .get("enum")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
    )
}

/// A scalar as XML text writes it.
pub(super) fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
