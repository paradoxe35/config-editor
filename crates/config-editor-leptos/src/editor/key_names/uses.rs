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
    let found = walked(document, behavior, schema, identity, Wanted::Name(name));
    placed(
        source,
        behavior,
        found
            .into_iter()
            .map(|(path, part, _)| (path, part))
            .collect(),
    )
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

/// The name written over byte `at` of `source`: a value, or a key read
/// through `propertyNames`, whose schemas carry the keyword, as a key of
/// the first map it names (of its first `from` schema, if any). Found by
/// where each such name is written, so it reaches places the caret's own
/// reading of the line does not (an item of a list written inline).
pub(crate) fn name_written_at(
    source: &str,
    behavior: &dyn ConfigLanguage,
    schema: &Value,
    identity: &SchemaIdentity,
    at: TextSize,
) -> Option<NameRef> {
    let parsed = crate::editor::parse_cache::parsed(behavior, source);
    let document = parsed.as_ref().as_ref().ok()?;
    note(Work::UsesWalk);
    let found = walked(document, behavior, schema, identity, Wanted::Any);
    let hits = placed(
        source,
        behavior,
        found
            .iter()
            .map(|(path, part, _)| (path.clone(), *part))
            .collect(),
    );
    let hit = hits
        .iter()
        .find(|hit| hit.range.start() <= at && at <= hit.range.end())?;
    found
        .into_iter()
        .find(|(path, part, _)| *path == hit.path && *part == hit.part)
        .and_then(|(_, _, name)| name)
}

/// What a walk looks for.
#[derive(Clone, Copy)]
enum Wanted<'w> {
    /// The uses of this name.
    Name(&'w NameRef),
    /// Every name, each with the map it names.
    Any,
}

/// The places of `document` a walk for `wanted` finds, in document order,
/// each with its name when every name is wanted.
fn walked(
    document: &Value,
    behavior: &dyn ConfigLanguage,
    schema: &Value,
    identity: &SchemaIdentity,
    wanted: Wanted<'_>,
) -> Vec<(FieldPath, UsePart, Option<NameRef>)> {
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
        wanted,
        found: &mut found,
    };
    walk.value(document, &mut Vec::new(), 0);
    found
}

/// One document's walk: the places found so far.
struct Walk<'w, 's, 'd> {
    applies: &'w Applies<'s, 'd>,
    identity: &'w SchemaIdentity,
    wanted: Wanted<'w>,
    found: &'w mut Vec<(FieldPath, UsePart, Option<NameRef>)>,
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
                // The keys' schemas, read only where a key may be wanted.
                let keys = match self.wanted {
                    Wanted::Name(name) if !members.contains_key(&name.name) => Vec::new(),
                    _ => key_branches(&here.branches(), self.applies.root()),
                };
                for (key, member) in members {
                    if let Some(name) = self.names(&keys, key) {
                        let mut at = path.clone();
                        at.push(SchemaPathSegment::Key(key.clone()));
                        self.found.push((FieldPath::from(at), UsePart::Key, name));
                    }
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
                if let Some(name) = self.names(&here.branches(), &scalar_text(scalar)) {
                    self.found
                        .push((FieldPath::from(path.clone()), UsePart::Value, name));
                }
            }
        }
    }

    /// Whether `text`, read by `branches`, is what the walk looks for:
    /// `Some(None)` for a use of the wanted name, `Some(Some(name))` for a
    /// name when every name is wanted.
    fn names(&self, branches: &[&Value], text: &str) -> Option<Option<NameRef>> {
        if branches.is_empty() {
            return None;
        }
        match self.wanted {
            Wanted::Name(name) => {
                (text == name.name && name.named_by(branches, self.identity)).then_some(None)
            }
            Wanted::Any => {
                let place = branches
                    .iter()
                    .flat_map(|branch| key_places(branch))
                    .next()?;
                Some(Some(NameRef {
                    schema: place.from.first().map_or_else(
                        || self.identity.clone(),
                        |id| SchemaIdentity::Id(id.clone()),
                    ),
                    pointer: place.pointer,
                    name: text.to_string(),
                }))
            }
        }
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
