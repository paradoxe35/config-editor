//! The known name under a place of the text, for the hover card's "defined
//! in" and go to definition: a value of a field with the keyword, or a key
//! of an object whose `propertyNames` has it, that is one of the names
//! offered there; and what Find uses searches for there, which is also
//! the key defining a name.

use serde_json::Value;

use super::{KeyNameSource, KeyOrigin, NameLookup, NameRef, Offered, key_branches, key_places};
use crate::FieldPath;
use crate::editor::applies::{Applies, scalar_text};
use crate::editor::completion::{FieldHit, Part};
use crate::editor::language::{ConfigLanguage, SchemaPathSegment};
use crate::editor::schema::SchemaBundle;
use crate::editor::text::TextRange;

/// Runs `read` with the text of the name `field` is (a value, or a key
/// named through `propertyNames`), the schemas reading it (the key's
/// schemas for a key, not its value's) and the document; `None` on
/// anything else or in a text that does not parse.
fn with_name<R>(
    source: &str,
    field: &FieldHit,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
    read: impl FnOnce(String, &[&Value], &Value) -> Option<R>,
) -> Option<R> {
    if field.part == Part::ItemStart {
        return None;
    }
    let parsed = crate::editor::parse_cache::parsed(behavior, source);
    let document = parsed.as_ref().as_ref().ok()?;
    let document_read = || Some(document);
    let applies = Applies::new(
        &bundle.schema,
        crate::editor::data_reading::lone_items(behavior),
        &document_read,
    );
    let (text, branches) = if field.part == Part::Key {
        let (SchemaPathSegment::Key(key), object) = field.path.split_last()? else {
            return None;
        };
        let object = applies.at(object).branches();
        (key.clone(), key_branches(&object, &bundle.schema))
    } else {
        let written = applies.value_at(&field.path)?;
        if written.is_object() || written.is_array() {
            return None;
        }
        (scalar_text(written), applies.at(&field.path).branches())
    };
    read(text, &branches, document)
}

/// The name `field` (a hit of the text, see `completion::field_at`) is,
/// when it is the value of a field whose schemas offer names, or the key
/// of an object whose `propertyNames` offer them (the key's schemas, not
/// its value's), and it is one of them; with where it comes from.
pub(crate) fn name_at(
    source: &str,
    field: &FieldHit,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
    names: &dyn KeyNameSource,
) -> Option<Offered> {
    with_name(
        source,
        field,
        bundle,
        behavior,
        |text, branches, document| {
            let lookup = NameLookup {
                identity: &bundle.identity,
                source: names,
                kept: None,
            };
            lookup
                .offered(branches, || Some(document))
                .into_iter()
                .find(|offered| offered.name == text)
        },
    )
}

/// What Find uses searches for at `field`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Searched {
    pub(crate) name: NameRef,
    /// `field` is the key defining it, not a use.
    pub(crate) definition: bool,
}

/// What Find uses searches for at `field`: a use's name (a known one as
/// it is offered; one defined nowhere as a key of the first map its
/// keyword names, of the first `from` schema there, if any), or the key
/// `field` is when it is a key of a map names come from: one the schema
/// names, or that `names_map` (another schema's keyword) says is one.
pub(crate) fn searched_at(
    source: &str,
    field: &FieldHit,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
    names: &dyn KeyNameSource,
    names_map: &dyn Fn(&str) -> bool,
) -> Option<Searched> {
    let used = with_name(
        source,
        field,
        bundle,
        behavior,
        |text, branches, document| {
            let lookup = NameLookup {
                identity: &bundle.identity,
                source: names,
                kept: None,
            };
            if let Some(known) = lookup
                .offered(branches, || Some(document))
                .into_iter()
                .find(|offered| offered.name == text)
            {
                return Some(NameRef {
                    schema: known.schema,
                    pointer: known.pointer,
                    name: known.name,
                });
            }
            let place = branches
                .iter()
                .flat_map(|branch| key_places(branch))
                .next()?;
            Some(NameRef {
                schema: place.from.first().map_or_else(
                    || bundle.identity.clone(),
                    |id| super::SchemaIdentity::Id(id.clone()),
                ),
                pointer: place.pointer,
                name: text,
            })
        },
    );
    if let Some(name) = used {
        return Some(Searched {
            name,
            definition: false,
        });
    }
    if field.part != Part::Key {
        return None;
    }
    let (SchemaPathSegment::Key(key), map) = field.path.split_last()? else {
        return None;
    };
    let pointer = FieldPath::from(map.to_vec()).to_pointer();
    let named = super::pointers_in(&bundle.schema).contains(&pointer) || names_map(&pointer);
    named.then(|| Searched {
        name: NameRef {
            schema: bundle.identity.clone(),
            pointer,
            name: key.clone(),
        },
        definition: true,
    })
}

/// Where the key `name` names is written in `source`, when it is a key of
/// the document itself: its key's bytes.
pub(crate) fn key_written(
    source: &str,
    name: &Offered,
    behavior: &dyn ConfigLanguage,
) -> Option<TextRange> {
    if name.origin != KeyOrigin::ThisDocument {
        return None;
    }
    let pointer = format!(
        "{}/{}",
        name.pointer,
        crate::editor::language::pointer_key(&name.name)
    );
    let span = crate::editor::paths::written_key_span(behavior, source, &pointer)?;
    let byte_at = |column| {
        let units = crate::editor::paths::offset_of(
            source,
            crate::editor::paths::TextPosition::new(span.line, column),
            false,
        );
        crate::editor::text::byte_of(source, usize::from(units))
    };
    let start = byte_at(span.start_column);
    Some(TextRange::new(start, byte_at(span.end_column).max(start)))
}
