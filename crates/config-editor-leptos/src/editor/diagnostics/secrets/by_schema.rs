//! The secrets a schema message must not quote, found by the schema and
//! by their texts.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::editor::applies::{Applies, ref_target};
use crate::editor::language::{SchemaPathSegment, pointer_index, pointer_segments};

/// The secrets a message must not quote, by schema and by text.
///
/// By schema: a value is a secret's where the schema marks its path
/// secret, read as the schema reads any document, so a list where one
/// value is wanted is that value repeated (an XML element written twice
/// reads as a list of two, each holding the secret field). A string or
/// number where the schema wants a list or a map holding a secret (a
/// broken text read as one string) may be that holder's text: it is
/// hidden whole.
///
/// By text: a quoted value hides each scalar whose text is a secret's,
/// wherever it is, so a secret one reading places elsewhere (an option
/// reading its field as a list of one) is hidden too.
pub(in crate::editor::diagnostics) struct Secrets<'s> {
    schema: Option<SchemaSecrets<'s>>,
    texts: HashSet<String>,
}

impl<'s> Secrets<'s> {
    /// No secrets.
    pub(in crate::editor::diagnostics) fn none() -> Self {
        Self {
            schema: None,
            texts: HashSet::new(),
        }
    }

    /// The secrets `root` marks in `document` (the one the problems are
    /// found in), written in a language that repeats a key for a list or
    /// not (`repeats_arrays`), and the texts of the values at the pointers
    /// of each document `readings` gives.
    pub(in crate::editor::diagnostics) fn new(
        document: &'s Value,
        root: &'s Value,
        repeats_arrays: bool,
        readings: &[(&Value, Vec<String>)],
    ) -> Self {
        let mut texts = HashSet::new();
        for (document, pointers) in readings {
            for pointer in pointers {
                if let Some(value) = document.pointer(pointer) {
                    scalar_texts(value, &mut texts);
                }
            }
        }
        Self {
            schema: Some(SchemaSecrets {
                document,
                root,
                applies: Applies::blind(root, repeats_arrays),
                below: RefCell::new(HashMap::new()),
            }),
            texts,
        }
    }

    /// Whether `value` (at `pointer`) is a secret field's or inside one.
    pub(in crate::editor::diagnostics) fn holds(&self, pointer: &str, value: &Value) -> bool {
        self.is_secret_text(value)
            || self
                .schema
                .as_ref()
                .is_some_and(|schema| schema.marks_at_or_above(&schema.path(pointer)))
    }

    /// Whether `value` (at `pointer`) holds a secret, or is one.
    pub(in crate::editor::diagnostics) fn touches(&self, pointer: &str, value: &Value) -> bool {
        self.holds(pointer, value)
            || self.holds_text(value)
            || self
                .schema
                .as_ref()
                .is_some_and(|schema| schema.may_hold(&schema.path(pointer)))
    }

    /// `value` (at `pointer`) with each secret inside it `mark`.
    pub(in crate::editor::diagnostics) fn hidden(
        &self,
        pointer: &str,
        value: &Value,
        mark: &Value,
    ) -> Value {
        let mut hidden = value.clone();
        if let Some(schema) = &self.schema {
            schema.hide(&mut hidden, &mut schema.path(pointer), mark);
        }
        if !self.texts.is_empty() {
            self.hide_texts(&mut hidden, mark);
        }
        hidden
    }

    /// Whether `value` is a scalar whose text is a secret's.
    fn is_secret_text(&self, value: &Value) -> bool {
        !self.texts.is_empty()
            && scalar_text(value).is_some_and(|text| self.texts.contains(text.as_ref()))
    }

    fn holds_text(&self, value: &Value) -> bool {
        if self.texts.is_empty() {
            return false;
        }
        match value {
            Value::Array(items) => items.iter().any(|item| self.holds_text(item)),
            Value::Object(map) => map.values().any(|child| self.holds_text(child)),
            scalar => self.is_secret_text(scalar),
        }
    }

    fn hide_texts(&self, value: &mut Value, mark: &Value) {
        match value {
            Value::Array(items) => items
                .iter_mut()
                .for_each(|item| self.hide_texts(item, mark)),
            Value::Object(map) => map
                .values_mut()
                .for_each(|child| self.hide_texts(child, mark)),
            scalar => {
                if self.is_secret_text(scalar) {
                    *scalar = mark.clone();
                }
            }
        }
    }
}

/// What the schema marks secret, read blind (whatever is written), each
/// path walked once for the problems of one text.
struct SchemaSecrets<'s> {
    /// The document the problems' pointers are in.
    document: &'s Value,
    root: &'s Value,
    applies: Applies<'s, 's>,
    /// Whether each schema (by address) marks a secret in itself or below.
    below: RefCell<HashMap<*const Value, bool>>,
}

impl<'s> SchemaSecrets<'s> {
    /// The path of `pointer` in the document: a step into a list is an
    /// item, any other a key.
    fn path(&self, pointer: &str) -> Vec<SchemaPathSegment> {
        let mut value = Some(self.document);
        pointer_segments(pointer)
            .into_iter()
            .map(|step| {
                let segment = match (value, pointer_index(&step)) {
                    (Some(Value::Array(_)), Some(index)) => SchemaPathSegment::ArrayItem(index),
                    _ => SchemaPathSegment::Key(step),
                };
                value = value.and_then(|value| match (&segment, value) {
                    (SchemaPathSegment::ArrayItem(index), Value::Array(items)) => items.get(*index),
                    (SchemaPathSegment::Key(key), Value::Object(map)) => map.get(key),
                    _ => None,
                });
                segment
            })
            .collect()
    }

    /// Whether the schema marks `path` or a field holding it secret.
    fn marks_at_or_above(&self, path: &[SchemaPathSegment]) -> bool {
        (0..=path.len()).any(|depth| marked(&self.applies.at(&path[..depth]).branches()))
    }

    /// Whether the schema marks a secret at `path` or anywhere below it.
    fn may_hold(&self, path: &[SchemaPathSegment]) -> bool {
        self.holds_below(&self.applies.at(path).branches())
    }

    /// Hides in `value` (at `path`) each value the schema marks secret and
    /// each string or number where it wants a container holding one.
    fn hide(&self, value: &mut Value, path: &mut Vec<SchemaPathSegment>, mark: &Value) {
        let branches = self.applies.at(path).branches();
        if marked(&branches) {
            *value = mark.clone();
            return;
        }
        if !self.holds_below(&branches) {
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    path.push(SchemaPathSegment::Key(key.clone()));
                    self.hide(child, path, mark);
                    path.pop();
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter_mut().enumerate() {
                    path.push(SchemaPathSegment::ArrayItem(index));
                    self.hide(item, path, mark);
                    path.pop();
                }
            }
            Value::String(_) | Value::Number(_) => *value = mark.clone(),
            Value::Bool(_) | Value::Null => {}
        }
    }

    /// Whether one of `branches` marks a secret in itself or in a schema
    /// inside it (through its references).
    fn holds_below(&self, branches: &[&'s Value]) -> bool {
        branches.iter().any(|branch| {
            let key = std::ptr::from_ref(*branch);
            if let Some(known) = self.below.borrow().get(&key) {
                return *known;
            }
            let found = marks_inside(branch, self.root);
            self.below.borrow_mut().insert(key, found);
            found
        })
    }
}

/// Whether the schemas applying to a field (`branches`) make it a secret.
fn marked(branches: &[&Value]) -> bool {
    branches.iter().any(|branch| marks(branch.as_object()))
}

/// Whether a schema's `members` mark its value secret.
fn marks(members: Option<&serde_json::Map<String, Value>>) -> bool {
    members.is_some_and(|members| {
        ["writeOnly", "x-secret"]
            .iter()
            .any(|mark| members.get(*mark) == Some(&Value::Bool(true)))
    })
}

/// Whether `schema` or a schema inside it marks a secret, its `$ref`s
/// (in `root`) followed, each schema read once (a cycle ends).
fn marks_inside(schema: &Value, root: &Value) -> bool {
    let mut read = HashSet::new();
    let mut next = vec![schema];
    let mut found = false;
    while let Some(schema) = next.pop() {
        if !read.insert(std::ptr::from_ref(schema)) {
            continue;
        }
        crate::editor::schema::each_schema(schema, &(), &mut |_, members, ()| {
            if let Some(Value::String(reference)) = members.get("$ref")
                && let Some(target) = ref_target(reference, root)
            {
                next.push(target);
            }
            found |= marks(Some(members));
            (!found).then_some(())
        });
        if found {
            return true;
        }
    }
    false
}

/// The text of a string or number (a secret may be read as either: XML
/// text `1234` is a number where another reading holds the string).
fn scalar_text(value: &Value) -> Option<Cow<'_, str>> {
    match value {
        Value::String(text) => Some(Cow::Borrowed(text)),
        Value::Number(number) => Some(Cow::Owned(number.to_string())),
        _ => None,
    }
}

/// The texts of the strings and numbers in `value`.
fn scalar_texts(value: &Value, out: &mut HashSet<String>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| scalar_texts(item, out)),
        Value::Object(map) => map.values().for_each(|child| scalar_texts(child, out)),
        scalar => out.extend(scalar_text(scalar).map(Cow::into_owned)),
    }
}
