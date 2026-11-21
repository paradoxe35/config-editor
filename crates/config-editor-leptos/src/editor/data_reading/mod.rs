//! How a text reads as data: the one answer every feature asks for a
//! language that does not write JSON's shapes one to one (XML, HCL
//! blocks). Such a language writes a list by repeating a key, so a key
//! written once is one value or a list's only item; XML writes every
//! scalar as text, so `8080` is text or a number; an empty element is an
//! empty map, the empty string or `null`. The answer depends on the
//! schemas that apply to the node (`applies`), and without a schema it is
//! the language's own reading (an element written once is that value).
//!
//! [`read`] reads a text once (shared through `parse_cache`) and returns a
//! [`DataReading`]: the value, and for each node read otherwise than the
//! language's own parse, how ([`Node`]). Completion, diagnostics, the
//! paths, the save and the codec ask it instead of deciding again.
//!
//! The language hook is data: [`Repetition`] (what the language repeats)
//! and the schema-free reading of a lone item (`Repeats::lone_item`, see
//! [`lone_spelling`]).

mod conform;
mod text;
mod written;

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use serde_json::Value;

use super::applies::Entry;
use super::language::{ConfigLanguage, ParseError, Repetition};

use conform::conform_at;
#[cfg(feature = "xml")]
pub(crate) use text::plain_scalar;
pub(crate) use text::text_literal;
pub(crate) use written::lone_spelling;

/// A text read as data.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DataReading {
    value: Value,
    /// The pointers (in `value`) of the lists that are a value written
    /// once, read as a list's only item because the schemas there want a
    /// list.
    lists_of_one: BTreeSet<String>,
    /// The scalars the schemas read otherwise than the language's parse
    /// (`"8"` read as `8`, an empty element `{}` read as `""`), each with
    /// the parse's value.
    retyped: BTreeMap<String, Value>,
}

/// How one node of a [`DataReading`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Node<'r> {
    /// Whether it is one value or a list.
    pub(crate) occurs: Occurs,
    /// Whether its scalar is the language's own reading or the schema's.
    pub(crate) scalar: Scalar<'r>,
}

/// Whether a node is one value, or a list made of a value written once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Occurs {
    /// As the language reads it: a value, or a list written as one (or by
    /// repeating its key).
    AsWritten,
    /// A value written once that the schemas read as a list's only item
    /// (a lone `<dns>` where only a list is allowed): its `/0` is the
    /// value written.
    ListOfOne,
}

/// Whether a scalar reads as the language's parse does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scalar<'r> {
    /// As parsed (or no scalar).
    AsParsed,
    /// As the schemas read it, where the parse read this value (XML text
    /// `"8"` read as the integer a schema wants).
    Retyped(&'r Value),
}

impl DataReading {
    /// A reading that is the language's parse.
    pub(crate) fn parsed(value: Value) -> Self {
        Self {
            value,
            ..Self::default()
        }
    }

    /// The value read.
    pub(crate) fn value(&self) -> &Value {
        &self.value
    }

    /// The value read, taken.
    pub(crate) fn into_value(self) -> Value {
        self.value
    }

    /// How the node at `pointer` (in the value read) reads.
    pub(crate) fn node(&self, pointer: &str) -> Node<'_> {
        Node {
            occurs: if self.lists_of_one.contains(pointer) {
                Occurs::ListOfOne
            } else {
                Occurs::AsWritten
            },
            scalar: self
                .retyped
                .get(pointer)
                .map_or(Scalar::AsParsed, Scalar::Retyped),
        }
    }

    /// Notes the value at `pointer`, read already, as made the only item
    /// of a list: what is noted under it moves under its `/0`.
    fn list_of_one_at(&mut self, pointer: &str) {
        let under = format!("{pointer}/");
        // Pointers under `pointer` sort together, from `{pointer}/` to
        // `{pointer}0` (`0` follows `/`).
        let end = format!("{pointer}0");
        let item = format!("{pointer}/0");
        let moved = |old: &str| format!("{item}{}", &old[pointer.len()..]);
        let lists: Vec<String> = self
            .lists_of_one
            .range(under.clone()..end.clone())
            .cloned()
            .collect();
        for old in lists {
            self.lists_of_one.remove(&old);
            self.lists_of_one.insert(moved(&old));
        }
        let scalars: Vec<String> = self
            .retyped
            .range(under..end)
            .map(|(old, _)| old.clone())
            .collect();
        for old in scalars {
            if let Some(parsed) = self.retyped.remove(&old) {
                self.retyped.insert(moved(&old), parsed);
            }
        }
        if let Some(parsed) = self.retyped.remove(pointer) {
            self.retyped.insert(item, parsed);
        }
        if self.lists_of_one.remove(pointer) {
            self.lists_of_one.insert(format!("{pointer}/0"));
        }
        self.lists_of_one.insert(pointer.to_string());
    }

    /// The lists of one under `pointer` (not at it), at most `most`, each
    /// as its pointer from there (`/a/0/b`).
    fn lists_of_one_under(&self, pointer: &str, most: usize) -> Vec<String> {
        let under = format!("{pointer}/");
        let end = format!("{pointer}0");
        self.lists_of_one
            .range(under..end)
            .take(most)
            .map(|list| list[pointer.len()..].to_string())
            .collect()
    }

    /// Notes the list of one at `pointer` read back as its item, the value
    /// written once: what is noted under its `/0` moves back under it
    /// (the inverse of [`Self::list_of_one_at`]).
    fn lone_at(&mut self, pointer: &str) {
        self.lists_of_one.remove(pointer);
        let item = format!("{pointer}/0");
        let under = format!("{item}/");
        let end = format!("{item}0");
        let moved = |old: &str| format!("{pointer}{}", &old[item.len()..]);
        let lists: Vec<String> = self
            .lists_of_one
            .range(under.clone()..end.clone())
            .cloned()
            .collect();
        for old in lists {
            self.lists_of_one.remove(&old);
            self.lists_of_one.insert(moved(&old));
        }
        let scalars: Vec<String> = self
            .retyped
            .range(under..end)
            .map(|(old, _)| old.clone())
            .collect();
        for old in scalars {
            if let Some(parsed) = self.retyped.remove(&old) {
                self.retyped.insert(moved(&old), parsed);
            }
        }
        if let Some(parsed) = self.retyped.remove(&item) {
            self.retyped.insert(pointer.to_string(), parsed);
        }
        if self.lists_of_one.remove(&item) {
            self.lists_of_one.insert(pointer.to_string());
        }
    }

    /// Whether a value written once reads as a list of one anywhere.
    #[cfg(feature = "schema-validation")]
    pub(crate) fn has_lists_of_one(&self) -> bool {
        !self.lists_of_one.is_empty()
    }

    /// `pointer` (in the value read) as the text writes it: without the
    /// `/0` after each list of one (the value written once), `None` when
    /// it is written so.
    pub(crate) fn written(&self, pointer: &str) -> Option<String> {
        if self.lists_of_one.is_empty() {
            return None;
        }
        let mut read = String::with_capacity(pointer.len());
        let mut written = String::with_capacity(pointer.len());
        for segment in pointer.split('/').skip(1) {
            let lone = segment == "0" && self.node(&read).occurs == Occurs::ListOfOne;
            read.push('/');
            read.push_str(segment);
            if !lone {
                written.push('/');
                written.push_str(segment);
            }
        }
        (written != pointer).then_some(written)
    }

    /// The language's parse this reading was made from (each scalar read
    /// otherwise put back, each list of one read as its item), `None` when
    /// the reading is the parse.
    #[cfg(any(test, feature = "schema-validation"))]
    pub(crate) fn as_parsed(&self) -> Option<Value> {
        if self.lists_of_one.is_empty() && self.retyped.is_empty() {
            return None;
        }
        let mut value = self.value.clone();
        for pointer in self.retyped.keys() {
            if let (Scalar::Retyped(parsed), Some(slot)) =
                (self.node(pointer).scalar, value.pointer_mut(pointer))
            {
                *slot = parsed.clone();
            }
        }
        // A list inside another sorts after it: the inner one is read as
        // its item first, while the outer one's pointer still holds.
        for pointer in self.lists_of_one.iter().rev() {
            if let Some(slot) = value.pointer_mut(pointer)
                && let Value::Array(items) = slot
                && items.len() == 1
            {
                *slot = items.remove(0);
            }
        }
        Some(value)
    }
}

/// Whether `language` writes a list by repeating its key, so a key written
/// once is one value or its list's only item (see the module docs): what
/// `applies` reads under a key, its schemas and, where they name a list,
/// its item's.
pub(crate) fn lone_items(language: &dyn ConfigLanguage) -> bool {
    language.repetition() != Repetition::None
}

/// `text` read in `language` as data, under `schema` when the language
/// writes data otherwise than JSON's shapes (see the module docs); from
/// the last reading of the same text (see `parse_cache`).
pub(crate) fn read(
    language: &dyn ConfigLanguage,
    text: &str,
    schema: Option<&Value>,
) -> Result<Rc<DataReading>, ParseError> {
    let repetition = language.repetition();
    let text_scalars = language.scalars_are_text();
    match schema.filter(|_| reads_by_schema(language)) {
        Some(schema) => super::parse_cache::conformed(language, text, schema, |value| {
            conform::conform(value, schema, repetition, text_scalars)
        }),
        None => super::parse_cache::parse(language, text)
            .map(|value| Rc::new(DataReading::parsed(value))),
    }
}

/// Whether a schema changes how `language` reads a text: it repeats keys
/// for lists, or writes every scalar as text (a custom language with
/// `scalars: "text"` too).
fn reads_by_schema(language: &dyn ConfigLanguage) -> bool {
    language.repetition() != Repetition::None || language.scalars_are_text()
}

/// The value of [`read`].
pub(crate) fn value(
    language: &dyn ConfigLanguage,
    text: &str,
    schema: Option<&Value>,
) -> Result<Value, ParseError> {
    read(language, text, schema).map(|reading| reading.value().clone())
}

/// [`read`] without the shared parse: a text read once on the side (a
/// stand-in for one that does not parse) does not push out the one on
/// screen.
#[cfg(feature = "protect")]
pub(crate) fn read_aside(
    language: &dyn ConfigLanguage,
    text: &str,
    schema: Option<&Value>,
) -> Result<DataReading, ParseError> {
    super::work_count::note_many(super::work_count::Work::ParsedBytes, text.len());
    let value = language.parse(text)?;
    let repetition = language.repetition();
    Ok(match schema.filter(|_| reads_by_schema(language)) {
        Some(schema) => conform::conform(value, schema, repetition, language.scalars_are_text()),
        None => DataReading::parsed(value),
    })
}

/// `value` (a value of one field, read by `language`) as the schemas of
/// `at` read it: what a suggestion writes there reads back so.
pub(crate) fn value_under(
    language: &dyn ConfigLanguage,
    value: Value,
    at: &Entry<'_>,
    root: &Value,
) -> Value {
    if reads_by_schema(language) {
        conform_at(
            value,
            at,
            root,
            language.repetition(),
            language.scalars_are_text(),
        )
        .into_value()
    } else {
        value
    }
}

/// `value` read in place as data under `schema`, for a language repeating
/// `repetition` (the tests read parsed values so).
#[cfg(test)]
pub(crate) fn conform_arrays(value: &mut Value, schema: &Value, repetition: Repetition) {
    *value = conform::conform(
        value.take(),
        schema,
        repetition,
        repetition == Repetition::Elements,
    )
    .into_value();
}

#[cfg(test)]
mod tests;
