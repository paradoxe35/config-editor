//! Find uses: the places of one document that use a name, the engine
//! behind `Shift+F12`, the hover card's "Uses" link and `tools::uses_at`
//! (and the one Rename builds on).
//!
//! A use is a scalar value, or a key an object's `propertyNames` reads,
//! whose schemas carry the keyword naming the map the name is a key of
//! (the pointer, and the schema of the documents defining it: the
//! document's own, or one of `from`), equal to the name. The schemas are
//! read exactly as the suggestions and the hover read them (`applies`,
//! every branch), so items, object fields and `anyOf` branches count;
//! comments, free text, `enum`/`examples` words and the same text under
//! another pointer never do. A document costs one walk of its parse per
//! request, and nothing runs while typing.

use serde_json::Value;

use super::{SchemaIdentity, key_branches, key_places};
use crate::FieldPath;
use crate::editor::applies::{Applies, scalar_text};
use crate::editor::language::{ConfigLanguage, SchemaPathSegment, Span};
use crate::editor::paths::{self, TextPosition};
use crate::editor::text::{TextRange, TextSize, byte_of};
use crate::editor::work_count::{Work, note};

/// A name searched for: a key of the map at `pointer` of the documents
/// whose schema is `schema`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NameRef {
    pub(crate) schema: SchemaIdentity,
    pub(crate) pointer: String,
    pub(crate) name: String,
}

/// Which part of a field uses the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UsePart {
    /// Its value.
    Value,
    /// Its key (named through `propertyNames`).
    Key,
}

/// One use of a name in a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UseHit {
    pub(crate) path: FieldPath,
    pub(crate) part: UsePart,
    /// Where it is written: the value or key as the text has it.
    pub(crate) range: TextRange,
    /// Its zero-based line.
    pub(crate) line: usize,
    /// That line's text.
    pub(crate) text: String,
}

impl UseHit {
    /// The use as `tools::uses_at` gives it, in `source` (the text it was
    /// found in): its range in lines and character columns.
    pub(crate) fn found(self, source: &str) -> crate::FoundUse {
        let place = |at: TextSize| {
            let (line, column) = paths::line_column(source, at);
            TextPosition::new(line, column)
        };
        crate::FoundUse {
            range: crate::TextRange::new(place(self.range.start()), place(self.range.end())),
            line: self.line + 1,
            text: self.text,
            path: self.path,
        }
    }
}

/// Values walked into at most: a deeper document is not searched past it.
const MAX_DEPTH: usize = 64;

impl NameRef {
    /// Whether one of the places a schema's keyword names (read off
    /// `branches`, a document of schema `identity`) is this name's map.
    fn named_by(&self, branches: &[&Value], identity: &SchemaIdentity) -> bool {
        branches.iter().any(|branch| {
            key_places(branch).iter().any(|place| {
                place.pointer == self.pointer
                    && (*identity == self.schema
                        || self
                            .schema
                            .id()
                            .is_some_and(|id| place.from.iter().any(|from| from == id)))
            })
        })
    }

    /// Whether `schema` (of a document of schema `identity`) names this
    /// name's map anywhere: a document whose schema does not is not read.
    pub(crate) fn mentioned_in(&self, schema: &Value, identity: &SchemaIdentity) -> bool {
        let mut found = false;
        crate::editor::schema::each_schema(schema, &(), &mut |_, members, ()| {
            if let Some(keyword) = members.get(super::KEYWORD) {
                let only = serde_json::json!({ super::KEYWORD: keyword });
                found |= self.named_by(&[&only], identity);
            }
            (!found).then_some(())
        });
        found
    }
}

/// The uses of `name` in `source`, a document in `behavior` whose schema
/// is `schema` (its identity `identity`), in the order written. None in a
/// text that does not parse.
pub(crate) fn uses_in(
    source: &str,
    behavior: &dyn ConfigLanguage,
    schema: &Value,
    identity: &SchemaIdentity,
    name: &NameRef,
) -> Vec<UseHit> {
    if !name.mentioned_in(schema, identity) {
        return Vec::new();
    }
    let parsed = crate::editor::parse_cache::parsed(behavior, source);
    let Ok(document) = parsed.as_ref() else {
        return Vec::new();
    };
    note(Work::UsesWalk);
    let found = walked(document, behavior, schema, identity, name);
    placed(source, behavior, found)
}

/// [`uses_in`] a document in `language`.
pub(crate) fn uses_in_language(
    source: &str,
    language: crate::Language,
    schema: &Value,
    identity: &SchemaIdentity,
    name: &NameRef,
) -> Vec<UseHit> {
    uses_in(source, language.behavior(), schema, identity, name)
}

/// The places of `document` that use `name`, in document order.
fn walked(
    document: &Value,
    behavior: &dyn ConfigLanguage,
    schema: &Value,
    identity: &SchemaIdentity,
    name: &NameRef,
) -> Vec<(FieldPath, UsePart)> {
    let read = || Some(document);
    let applies = Applies::new(
        schema,
        crate::editor::data_reading::lone_items(behavior),
        &read,
    );
    let mut found = Vec::new();
    let mut walk = Walk {
        applies: &applies,
        identity,
        name,
        found: &mut found,
    };
    walk.value(document, &mut Vec::new(), 0);
    found
}

/// One document's walk: the places found so far.
struct Walk<'w, 's, 'd> {
    applies: &'w Applies<'s, 'd>,
    identity: &'w SchemaIdentity,
    name: &'w NameRef,
    found: &'w mut Vec<(FieldPath, UsePart)>,
}

impl Walk<'_, '_, '_> {
    fn value(&mut self, value: &Value, path: &mut Vec<SchemaPathSegment>, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        let here = self.applies.at(path);
        // Nothing applies here, nor anywhere inside.
        if here.entry().is_empty() {
            return;
        }
        match value {
            Value::Object(members) => {
                // The keys' schemas, read only where the name is a key.
                if members.contains_key(&self.name.name) {
                    let keys = key_branches(&here.branches(), self.applies.root());
                    if self.uses(&keys, &self.name.name) {
                        let mut at = path.clone();
                        at.push(SchemaPathSegment::Key(self.name.name.clone()));
                        self.found.push((FieldPath::from(at), UsePart::Key));
                    }
                }
                for (key, member) in members {
                    path.push(SchemaPathSegment::Key(key.clone()));
                    self.value(member, path, depth + 1);
                    path.pop();
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    path.push(SchemaPathSegment::ArrayItem(index));
                    self.value(item, path, depth + 1);
                    path.pop();
                }
            }
            scalar => {
                if self.uses(&here.branches(), &scalar_text(scalar)) {
                    self.found
                        .push((FieldPath::from(path.clone()), UsePart::Value));
                }
            }
        }
    }

    /// Whether `text`, read by `branches`, is a use of the name.
    fn uses(&self, branches: &[&Value], text: &str) -> bool {
        text == self.name.name && self.name.named_by(branches, self.identity)
    }
}

/// The places `found` in `source`, each where the text writes it (all read
/// in one batch), in the order written; a place the text does not show
/// (read through an alias it writes once, say) is where it is written.
fn placed(
    source: &str,
    behavior: &dyn ConfigLanguage,
    found: Vec<(FieldPath, UsePart)>,
) -> Vec<UseHit> {
    if found.is_empty() {
        return Vec::new();
    }
    let pointers: Vec<String> = found.iter().map(|(path, _)| path.to_pointer()).collect();
    let mut hits: Vec<UseHit> = paths::with_spans(behavior, source, &pointers, || {
        found
            .into_iter()
            .zip(&pointers)
            .filter_map(|((path, part), pointer)| {
                let span = match part {
                    UsePart::Value => paths::problem_span(behavior, source, pointer, false, false),
                    UsePart::Key => paths::problem_key_span(behavior, source, pointer),
                }?;
                Some(hit(source, path, part, span))
            })
            .collect()
    });
    hits.sort_by_key(|hit| hit.range.start());
    // Two paths read through one written place are one use there.
    hits.dedup_by(|one, other| one.range == other.range);
    hits
}

/// The use of `path` written at `span` of `source`, kept to its line.
fn hit(source: &str, path: FieldPath, part: UsePart, span: Span) -> UseHit {
    let text = paths::split_line(source, span.line - 1)
        .unwrap_or_default()
        .to_string();
    let end_column = span.end_column.min(text.chars().count() + 1);
    let at = |column: usize| -> TextSize {
        let units = paths::offset_of(source, TextPosition::new(span.line, column), false);
        byte_of(source, usize::from(units))
    };
    let start = at(span.start_column);
    let end = at(end_column.max(span.start_column));
    UseHit {
        path,
        part,
        range: TextRange::new(start, end),
        line: span.line - 1,
        text,
    }
}
