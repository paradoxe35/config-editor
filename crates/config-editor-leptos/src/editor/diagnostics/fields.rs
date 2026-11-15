//! Some fields of a document by pointer (the secret ones, the host's
//! placeholders), looked up rather than compared with each problem.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};

use serde_json::Value;

/// Some fields of a document, by pointer: whether a path is one of them
/// or inside one is a lookup of each of its ancestors, and the ones inside
/// a path are a range of the sorted pointers, not a pass over them all
/// (thousands of secret placeholders, checked for each problem).
pub(super) struct Fields(BTreeSet<String>);

impl Fields {
    pub(super) fn new(pointers: Vec<String>) -> Self {
        super::super::work_count::note(super::super::work_count::Work::TokenScan);
        Self(pointers.into_iter().collect())
    }

    /// Whether `path` is one of the fields or inside one.
    pub(super) fn holds(&self, path: &str) -> bool {
        if self.0.is_empty() {
            return false;
        }
        let mut end = path.len();
        loop {
            if self.0.contains(&path[..end]) {
                return true;
            }
            if end == 0 {
                return false;
            }
            end = path[..end].rfind('/').unwrap_or(0);
        }
    }

    /// The fields strictly inside `path`, by pointer.
    pub(super) fn within<'s>(&'s self, path: &str) -> impl Iterator<Item = &'s str> + 's {
        let prefix = format!("{path}/");
        self.0
            .range(prefix.clone()..)
            .take_while(move |pointer| pointer.starts_with(&prefix))
            .map(String::as_str)
    }

    /// Whether some field is strictly inside `path`.
    pub(super) fn any_within(&self, path: &str) -> bool {
        !self.0.is_empty() && self.within(path).next().is_some()
    }
}

/// The secret fields of a document, and the texts of their values: a
/// quoted value hides each scalar whose text is a secret's, wherever it
/// is, so a secret one reading places elsewhere (an option reading its
/// field as a list of one) is hidden too.
pub(super) struct Secrets {
    fields: Fields,
    texts: HashSet<String>,
}

impl Secrets {
    /// The secrets at `pointers` of each document `readings` gives
    /// (the one the problems are found in first).
    pub(super) fn new(readings: &[(&Value, Vec<String>)]) -> Self {
        let mut texts = HashSet::new();
        for (document, pointers) in readings {
            for pointer in pointers {
                if let Some(value) = document.pointer(pointer) {
                    scalar_texts(value, &mut texts);
                }
            }
        }
        let fields = readings
            .first()
            .map(|(_, pointers)| pointers.clone())
            .unwrap_or_default();
        Self {
            fields: Fields::new(fields),
            texts,
        }
    }

    /// Whether `value` (at `pointer`) is a secret field's or inside one.
    pub(super) fn holds(&self, pointer: &str, value: &Value) -> bool {
        self.fields.holds(pointer) || self.is_secret_text(value)
    }

    /// Whether `value` is a scalar whose text is a secret's.
    fn is_secret_text(&self, value: &Value) -> bool {
        !self.texts.is_empty()
            && scalar_text(value).is_some_and(|text| self.texts.contains(text.as_ref()))
    }

    /// Whether `value` (at `pointer`) holds a secret, or is one.
    pub(super) fn touches(&self, pointer: &str, value: &Value) -> bool {
        self.holds(pointer, value) || self.fields.any_within(pointer) || self.holds_text(value)
    }

    /// `value` (at `pointer`) with each secret inside it `mark`.
    pub(super) fn hidden(&self, pointer: &str, value: &Value, mark: &Value) -> Value {
        let mut hidden = value.clone();
        for inner in self.fields.within(pointer) {
            if let Some(slot) = hidden.pointer_mut(&inner[pointer.len()..]) {
                *slot = mark.clone();
            }
        }
        if !self.texts.is_empty() {
            self.hide_texts(&mut hidden, mark);
        }
        hidden
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
