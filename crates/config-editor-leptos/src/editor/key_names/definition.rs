//! The known name under a place of the text, for the hover card's "defined
//! in" and go to definition: a value of a field with the keyword, or a key
//! of an object whose `propertyNames` has it, that is one of the names
//! offered there.

use super::{KeyNameSource, KeyOrigin, NameLookup, Offered, key_branches};
use crate::editor::applies::{Applies, scalar_text};
use crate::editor::completion::{FieldHit, Part};
use crate::editor::language::{ConfigLanguage, SchemaPathSegment};
use crate::editor::schema::SchemaBundle;
use crate::editor::text::TextSize;

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
    if field.part == Part::ItemStart {
        return None;
    }
    let parsed = crate::editor::parse_cache::parsed(behavior, source);
    let document = parsed.as_ref().as_ref().ok()?;
    let read = || Some(document);
    let applies = Applies::new(
        &bundle.schema,
        crate::editor::data_reading::lone_items(behavior),
        &read,
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
    let lookup = NameLookup {
        identity: &bundle.identity,
        source: names,
        kept: None,
    };
    lookup
        .offered(&branches, || Some(document))
        .into_iter()
        .find(|offered| offered.name == text)
}

/// Where the key `name` names is written in `source`, when it is a key of
/// the document itself: the start of its key.
pub(crate) fn key_written(
    source: &str,
    name: &Offered,
    behavior: &dyn ConfigLanguage,
) -> Option<TextSize> {
    if name.origin != KeyOrigin::ThisDocument {
        return None;
    }
    let pointer = format!(
        "{}/{}",
        name.pointer,
        crate::editor::language::pointer_key(&name.name)
    );
    let span = crate::editor::paths::memo_key_span(behavior, source, &pointer)?;
    let units = crate::editor::paths::offset_of(
        source,
        crate::editor::paths::TextPosition::new(span.line, span.start_column),
        false,
    );
    Some(crate::editor::text::byte_of(source, usize::from(units)))
}
