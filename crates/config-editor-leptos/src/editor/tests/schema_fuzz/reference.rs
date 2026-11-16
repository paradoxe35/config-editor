//! What the checks compare with: the `jsonschema` crate's verdict on a
//! value, and the editor's own reading and problems of a text.

use crate::editor::language::pointer_key;
use serde_json::Value;

use crate::editor::Language;
use crate::editor::codec;
use crate::editor::diagnostics::{
    Diagnostic, Severity, host_problems, validate_keyed, with_host_problems,
};
use crate::editor::schema::Schema;

/// The editor's compiled schema and the reference validator, both from
/// `schema`; `None` when either refuses it.
pub(super) struct Both {
    pub(super) editor: Schema,
    pub(super) reference: jsonschema::Validator,
}

pub(super) fn compile(schema: &Value) -> Option<Both> {
    let reference = jsonschema::validator_for(schema).ok()?;
    let editor = Schema::compile(&schema.to_string()).ok()?;
    Some(Both { editor, reference })
}

impl Both {
    /// The schema as the editor reads it (references made pointers).
    pub(super) fn read_schema(&self) -> &Value {
        &self.editor.bundle.schema
    }

    /// `doc` written in `language`, when it can write it.
    pub(super) fn written(&self, language: Language, doc: &Value) -> Option<String> {
        codec::serialize(language.behavior(), doc, None).ok()
    }

    /// `text` read in `language` as the editor reads it for this schema.
    pub(super) fn read(&self, language: Language, text: &str) -> Option<Value> {
        codec::read(language.behavior(), text, Some(self.read_schema())).ok()
    }

    /// The editor's errors (no warnings) for `text`.
    pub(super) fn problems(&self, language: Language, text: &str) -> Vec<Diagnostic> {
        self.editor
            .validate(language, text)
            .into_iter()
            .filter(|problem| problem.severity == Severity::Error)
            .collect()
    }

    /// The errors an Edit-as view in `edit` shows for its text `view`,
    /// whose host in `host` holds `host_text`: the view's own, and while
    /// it cannot show the host exactly (`inexact`), the host's it does not
    /// have (as the editor finds them, see `diagnostics::host_reading`).
    pub(super) fn view_problems(
        &self,
        (edit, view): (Language, &str),
        (host, host_text): (Language, &str),
        inexact: bool,
    ) -> Vec<Diagnostic> {
        let strings = crate::strings::default_strings();
        let bundle = &self.editor.bundle;
        let (own, keys) = validate_keyed(
            view,
            Some(bundle),
            None,
            true,
            edit.behavior(),
            &[],
            strings,
        );
        let host = inexact
            .then(|| {
                host_problems(
                    host_text,
                    host.behavior(),
                    view,
                    edit.behavior(),
                    bundle,
                    &[],
                    strings,
                )
            })
            .flatten();
        let shown = match &host {
            Some(host) => with_host_problems(&own, &keys, host, view, strings),
            None => own,
        };
        shown
            .into_iter()
            .filter(|problem| problem.severity == Severity::Error)
            .collect()
    }

    pub(super) fn valid(&self, value: &Value) -> bool {
        self.reference.is_valid(value)
    }

    /// Whether `witness` holds `key` at its root with no error at or
    /// inside it: the key's schema can be met there.
    pub(super) fn met(&self, witness: &Value, key: &str) -> bool {
        let pointer = format!("/{}", pointer_key(key));
        witness.pointer(&pointer).is_some()
            && !self.reference.iter_errors(witness).any(|error| {
                let at = error.instance_path().as_str();
                at == pointer || at.starts_with(&format!("{pointer}/"))
            })
    }
}

/// The messages of `problems`, one per line.
pub(super) fn messages(problems: &[Diagnostic]) -> String {
    problems
        .iter()
        .map(|problem| {
            format!(
                "{:?}:{:?} {}",
                problem.line, problem.column, problem.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether `left` and `right` are the same value, numbers compared as
/// numbers (a host keeping its spelling `0.00` for a typed `0` holds the
/// same number, as JSON Schema reads it).
pub(super) fn same(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(a, b)| same(a, b))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, a)| right.get(key).is_some_and(|b| same(a, b)))
        }
        _ => left == right,
    }
}

/// Whether `value` holds a value XML cannot write as itself whatever the
/// schema (documented: an empty list or object, a list in a list, a
/// `null`, text with spaces at its ends).
pub(super) fn xml_cannot_hold(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim() != text,
        Value::Array(items) => {
            items.is_empty()
                || items
                    .iter()
                    .any(|item| item.is_array() || xml_cannot_hold(item))
        }
        Value::Object(map) => map.is_empty() || map.values().any(xml_cannot_hold),
        _ => false,
    }
}

/// Whether every `default` and `examples` entry of `schema` fits the
/// schema it annotates (a schema's own wrong default is its author's).
pub(super) fn annotations_fit(schema: &Value) -> bool {
    let defs = schema.get("$defs").cloned();
    let mut fit = true;
    walk(schema, &mut |node| {
        let Some(map) = node.as_object() else {
            return;
        };
        let mut given: Vec<&Value> = map.get("default").into_iter().collect();
        if let Some(Value::Array(examples)) = map.get("examples") {
            given.extend(examples);
        }
        if given.is_empty() {
            return;
        }
        let mut alone = node.clone();
        if let Some(defs) = &defs {
            alone["$defs"] = defs.clone();
        }
        if let Ok(validator) = jsonschema::validator_for(&alone) {
            fit &= given.iter().all(|value| validator.is_valid(value));
        }
    });
    fit
}

/// Every node of `value`, outermost first.
pub(super) fn walk(value: &Value, visit: &mut dyn FnMut(&Value)) {
    visit(value);
    match value {
        Value::Array(items) => items.iter().for_each(|item| walk(item, visit)),
        Value::Object(map) => map.values().for_each(|child| walk(child, visit)),
        _ => {}
    }
}

/// Whether `value` is, or holds, an empty value (`null`, `""`, `{}`): a
/// slot left to type.
pub(super) fn holds_empty(value: &Value) -> bool {
    let mut found = false;
    walk(value, &mut |node| {
        found |= match node {
            Value::Null => true,
            Value::String(text) => text.is_empty(),
            Value::Object(map) => map.is_empty(),
            _ => false,
        }
    });
    found
}
