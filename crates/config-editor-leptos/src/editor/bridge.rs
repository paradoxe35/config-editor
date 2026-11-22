//! Editing a document in one language while the host keeps it in another
//! (`<ConfigEditor language=Language::XML edit_as=Language::HCL>`).
//!
//! The host's text (`value`, `baseline`, `initial_value`) is shown converted;
//! edits convert back before `on_change`, keeping the host's framing. Both
//! directions go through `codec`, so no language needs bridge support.
//!
//! Positions in the texts are bytes (`TextSize`, `TextRange`), cut through
//! `text::slice`; a language hook's `usize` bytes convert where they come
//! in.

#![deny(
    clippy::string_slice,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use super::Language;
use super::byte_order_mark;
use super::codec::{self, Conversion, ConvertError};
use super::data_reading::DataReading;
use super::diagnostics::{Diagnostic, Severity};
use super::edit_as::{EditAsProblem, EditAsText, SaveError};
use super::item_sources::{ItemSource, ItemSources, Pairing, SaveTold};
use super::language::{ConfigLanguage, pointer_key};
use super::schema::SchemaBundle;
use crate::editor::paths::FieldPath;
use std::collections::HashMap;
use std::sync::Arc;

/// Each item's source, by its pointer in the view's value.
type ItemMap = HashMap<String, ItemSource>;

mod changed_forms;
mod collapsed;
mod host_strings;
mod kept_comments;
pub(super) mod links;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod many_renames_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod many_values_tests;
mod merge;
mod moved_forms;
mod read_back;
mod schema_readings;
mod shared;
mod tags;
mod written_anew;
use changed_forms::{emptied_quotes, text_scalars, typed_spellings, whole_floats, wide_numbers};
#[cfg(all(
    test,
    feature = "schema-validation",
    feature = "protect",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
pub(super) use merge::paired;
use merge::{View, merge_changes};

/// A save's patch of the host's text: `previous` made to read as `new`.
type Patch = fn(
    &dyn ConfigLanguage,
    &str,
    &serde_json::Value,
    &serde_json::Value,
    Option<&serde_json::Value>,
    SaveTold<'_>,
) -> Option<String>;

/// The patches that change only what one edit touched, in the order tried.
const PRECISE: [Patch; 5] = [
    codec::patch_reorder,
    codec::patch_scalars,
    codec::patch_key_renames,
    codec::patch_scalar_to_container,
    codec::patch_inline,
];

// How many saves wrote the host whole (its comments and formatting gone):
// the save property tests require none for a value edit.
#[cfg(test)]
thread_local! {
    pub(super) static WHOLE_RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Clone)]
pub(super) struct Bridge {
    pub(super) host: Language,
    pub(super) edit: Language,
    /// Reads lone values as arrays where the schema expects them (XML).
    pub(super) schema: Option<Arc<SchemaBundle>>,
}

impl Bridge {
    /// `None` when both languages are the same.
    pub(super) fn new(
        host: Language,
        edit: Language,
        schema: Option<Arc<SchemaBundle>>,
    ) -> Option<Self> {
        (host != edit).then_some(Bridge { host, edit, schema })
    }

    /// How the host's texts read: what names a view's items by their
    /// host paths (see `links::HostReading`).
    pub(super) fn reading(&self) -> links::HostReading {
        links::HostReading {
            language: self.host,
            schema: self.schema.clone(),
        }
    }

    fn shape(&self) -> Option<&serde_json::Value> {
        self.schema.as_deref().map(|bundle| &bundle.schema)
    }

    /// `host_text` shown in the edit language: the document after a
    /// leading byte order mark (the save puts it back).
    pub(super) fn to_edit(&self, host_text: &str) -> Result<Conversion, ConvertError> {
        let (host_text, _) = byte_order_mark::split(host_text);
        if host_text.trim().is_empty() {
            return Ok(Conversion {
                text: String::new(),
                losses: Vec::new(),
            });
        }
        codec::convert(
            host_text,
            self.host.behavior(),
            self.edit.behavior(),
            None,
            self.shape(),
        )
    }

    /// `edit_text`, a text of this view with unsaved edits, written in
    /// `next` (another view's language): each scalar the user left as the
    /// editor wrote it (`untouched`, see `links::untouched`) as the host
    /// holds it, which this view may read otherwise (XML reads the host's
    /// string `"2021"` as the number 2021), the rest as this view reads it.
    pub(super) fn to_other_view(
        &self,
        edit_text: &str,
        next: Language,
        untouched: Option<&[links::Untouched]>,
    ) -> Result<Conversion, ConvertError> {
        let edit = self.edit.behavior();
        let reading = super::data_reading::read(edit, edit_text, self.shape())
            .map_err(ConvertError::Parse)?;
        codec::convertible(edit, edit_text)?;
        let value = self.host_values(&reading, untouched.unwrap_or_default(), None);
        let conversion = codec::render(&value, next.behavior(), None, self.shape())?;
        codec::convertible(next.behavior(), &conversion.text)?;
        Ok(conversion)
    }

    /// Whether `edit_text` can be a text of this view that stood for the
    /// host's `host_text`: it reads in the edit language with the keys at
    /// the top that the view of `host_text` holds. The same text in
    /// another view's language (cut in one view, pasted back after a
    /// switch) is another document. (The values may read otherwise: a
    /// list of one converted again need not read as the lone element the
    /// host holds.)
    pub(super) fn shows_as(&self, host_text: &str, edit_text: &str) -> bool {
        let edit = self.edit.behavior();
        let read = |text: &str| codec::read(edit, text, self.shape()).ok();
        let Some(shown) = self.to_edit(host_text).ok() else {
            return false;
        };
        if shown.text == edit_text {
            return true;
        }
        match (read(edit_text), read(&shown.text)) {
            (Some(serde_json::Value::Object(held)), Some(serde_json::Value::Object(shown))) => {
                held.len() == shown.len() && held.keys().all(|key| shown.contains_key(key))
            }
            (Some(held), Some(shown)) => !held.is_object() && !shown.is_object(),
            _ => edit_text.trim().is_empty() && host_text.trim().is_empty(),
        }
    }

    /// `previous_host` is the host's last text, whose framing is kept: a
    /// save in a session that began on it, of `edit_text` typed over the
    /// view of it (see [`Self::typed_over_view`]).
    #[cfg(test)]
    #[allow(dead_code, reason = "used by the tests of language pairs")]
    pub(super) fn to_host(
        &self,
        edit_text: &str,
        previous_host: &str,
    ) -> Result<String, SaveError> {
        let untouched = self.typed_over_view(edit_text, previous_host);
        self.to_host_linked(
            edit_text,
            previous_host,
            previous_host,
            None,
            untouched.as_deref(),
        )
    }

    /// The record a test's user leaves of typing `edit_text` over the view
    /// of `previous_host` (the record the editor's links keep, which a
    /// text given whole has none of): each scalar still written as the
    /// view wrote it at its place, or as the lone value a list was made of
    /// (an element copied), left alone, standing for that host value;
    /// every other scalar typed. `None` when the host shows no view.
    #[cfg(test)]
    pub(super) fn typed_over_view(
        &self,
        edit_text: &str,
        previous_host: &str,
    ) -> Option<Vec<links::Untouched>> {
        fn scalars(value: &serde_json::Value, at: &mut String, out: &mut Vec<String>) {
            let length = at.len();
            match value {
                serde_json::Value::Object(map) => {
                    for (key, child) in map {
                        at.push('/');
                        at.push_str(&pointer_key(key));
                        scalars(child, at, out);
                        at.truncate(length);
                    }
                }
                serde_json::Value::Array(items) => {
                    for (index, child) in items.iter().enumerate() {
                        at.push('/');
                        at.push_str(&index.to_string());
                        scalars(child, at, out);
                        at.truncate(length);
                    }
                }
                _ => out.push(at.clone()),
            }
        }
        let shown = self.to_edit(previous_host).ok()?.text;
        let edit = self.edit.behavior();
        let (now, was) = (
            super::parse_cache::parsed(edit, edit_text),
            super::parse_cache::parsed(edit, &shown),
        );
        let (Ok(now), Ok(was)) = (now.as_ref(), was.as_ref()) else {
            return Some(Vec::new());
        };
        let paths = self.reading().paths(previous_host, was).unwrap_or_default();
        let host: Arc<str> = Arc::from(previous_host);
        let mut pointers = Vec::new();
        scalars(now, &mut String::new(), &mut pointers);
        Some(
            pointers
                .into_iter()
                .filter_map(|pointer| {
                    let value = now.pointer(&pointer);
                    let lone = pointer
                        .rsplit_once('/')
                        .filter(|(_, index)| index.parse::<usize>().is_ok())
                        .map(|(list, _)| list)
                        .filter(|list| {
                            was.pointer(list)
                                .is_some_and(|lone| !lone.is_array() && !lone.is_object())
                        });
                    let from = if was.pointer(&pointer) == value {
                        pointer.clone()
                    } else {
                        lone.filter(|list| was.pointer(list) == value)?.to_string()
                    };
                    let from = paths.value(&from);
                    Some(links::Untouched {
                        from,
                        host: host.clone(),
                        pointer,
                        copied: false,
                    })
                })
                .collect(),
        )
    }

    /// [`Self::to_host`] in an edit session that began on the host text
    /// `origin`, whose styles outlast the session's saves (the quotes of a
    /// string emptied and typed again). A leading byte order mark is no
    /// part of the document: the text after it is patched, and it stays.
    /// The host's text is the save only when it reads back as the view
    /// holds the document: a value it reads otherwise is
    /// [`SaveError::CannotKeep`]. The text comes with no record of its
    /// edits (a text the host gave): where only that record could decide
    /// the save, it waits and says so.
    pub(super) fn to_host_since(
        &self,
        edit_text: &str,
        previous_host: &str,
        origin: &str,
    ) -> Result<String, SaveError> {
        self.to_host_linked(edit_text, previous_host, origin, None, None)
    }

    /// [`Self::to_host_since`], told which item of the host each of the
    /// view's is by its links (see `links`), and the scalars the user left
    /// as the editor wrote them (`untouched`, `None` for a text with no
    /// record of its edits).
    pub(super) fn to_host_linked(
        &self,
        edit_text: &str,
        previous_host: &str,
        origin: &str,
        told: Option<&links::Told>,
        untouched: Option<&[links::Untouched]>,
    ) -> Result<String, SaveError> {
        let (previous, _) = byte_order_mark::split(previous_host);
        let (origin, _) = byte_order_mark::split(origin);
        // The steps of the save read the spans of the host's text and the
        // view's: each once; and ask once whether the host writes a value.
        let conversion = super::paths::sharing_spans(self.host.behavior(), previous, || {
            super::paths::sharing_spans(self.edit.behavior(), edit_text, || {
                codec::sharing_writes(|| {
                    self.to_unmarked_host(edit_text, previous, origin, told, untouched)
                })
            })
        })?;
        // A value the host would read back otherwise waits (a save written
        // whole is checked here, a patched one before it is read back).
        if let Some(pointer) = conversion.losses.first() {
            return Err(SaveError::CannotKeep {
                pointer: pointer.clone(),
            });
        }
        Ok(byte_order_mark::restore(previous_host, conversion.text))
    }

    /// `told`: each item's source, by its links; `untouched`: the scalars
    /// the user left as the editor wrote them (`None`: no record).
    fn to_unmarked_host(
        &self,
        edit_text: &str,
        previous_host: &str,
        origin: &str,
        told: Option<&links::Told>,
        untouched: Option<&[links::Untouched]>,
    ) -> Result<Conversion, SaveError> {
        // The host text before the last the links name, without its byte
        // order mark.
        let earlier = told.map(|told| {
            byte_order_mark::split(told.earlier.as_deref().unwrap_or_default())
                .0
                .to_string()
        });
        // A view whose links name no item of the host's last text is an
        // edit of the text they name (see `links::Told::base`): saved over
        // it, which holds their forms and comments.
        if let Some(told) = told
            && let Some(base) = told.base.as_deref()
        {
            let told =
                links::Told::listed(told.items().clone(), None, told.earlier.clone(), Vec::new());
            let base = byte_order_mark::split(base).0;
            return self.to_unmarked_host(edit_text, base, origin, Some(&told), untouched);
        }
        let items = told.zip(earlier.as_deref());
        // The host's last text as it was given, whose comments the save
        // keeps (the patches below may start from a respelled copy).
        let host_previous = previous_host;
        if edit_text.trim().is_empty() {
            return Ok(Conversion {
                text: String::new(),
                losses: Vec::new(),
            });
        }
        // Integers past 64 bits a host holds as written (XML text) are
        // saved as typed, as the view's own language saves them.
        let stood_in;
        let (edit_text, view_reading) =
            match super::data_reading::read(self.edit.behavior(), edit_text, self.shape()) {
                Ok(edited) => (edit_text, edited),
                Err(error) => {
                    let Some(text) = wide_numbers::as_text(
                        self.edit.behavior(),
                        self.host.behavior(),
                        edit_text,
                        &error,
                    ) else {
                        return Err(ConvertError::Parse(error).into());
                    };
                    stood_in = text;
                    let edited =
                        super::data_reading::read(self.edit.behavior(), &stood_in, self.shape())
                            .map_err(ConvertError::Parse)?;
                    (stood_in.as_str(), edited)
                }
            };
        codec::convertible(self.edit.behavior(), edit_text)?;
        let host = self.host.behavior();
        // A scalar left as the editor wrote it is its host value, whatever
        // the view read it as (see `links::untouched`).
        // The host's reading notes the lists it reads a value written once
        // as (the merge keeps the rest as the host writes them).
        let host_reading = super::data_reading::read(host, previous_host, self.shape()).ok();
        let edited = self.host_values(
            &view_reading,
            untouched.unwrap_or_default(),
            host_reading
                .as_ref()
                .map(|reading| (previous_host, reading.value())),
        );
        let Some(old) = host_reading.as_ref().map(|reading| reading.value().clone()) else {
            return Ok(codec::render(
                &edited,
                host,
                Some(previous_host),
                self.shape(),
            )?);
        };
        // Only what the user changed is saved: the edit is compared with the
        // host value as the edit language showed it, so values that language
        // cannot tell apart (XML reads `null`, `""` and `"1"` as text) keep
        // the host's own.
        // Keys the host reads two ways are compared as the edit spells them
        // (an XML attribute the view showed `@a` beside a child `a` a save
        // removed is still `@a`, so a child `a` added back is the child).
        // So are keys the edit language reads two ways, as the host spells
        // them (a host's `@a`, which an XML view shows as the attribute
        // `a`, stays `@a`).
        let edited = self.edit.behavior().spelled_like(edit_text, edited, &old);
        let old = host.spelled_like(previous_host, old, &edited);
        let shown = self.to_edit(previous_host).ok();
        let seen = shown.as_ref().and_then(|shown| {
            codec::read(self.edit.behavior(), &shown.text, self.shape())
                .ok()
                .map(|seen| self.edit.behavior().spelled_like(&shown.text, seen, &old))
                .map(|seen| host.spelled_like(previous_host, seen, &edited))
        });
        let unshown = shown
            .as_ref()
            .map(|shown| shown.losses.clone())
            .unwrap_or_default();
        // Numbers the edit kept but typed in another spelling (`4.80`).
        let respelled = match (&shown, &seen) {
            (Some(shown), Some(seen)) => typed_spellings::respelled(
                self.edit.behavior(),
                (&shown.text, edit_text),
                (seen, &edited),
            ),
            _ => Vec::new(),
        };
        // Whether the user moved keys in the view (not the edit language's
        // writer): the save then holds them in the order moved to.
        let moved = seen
            .as_ref()
            .is_some_and(|seen| merge::moved_by_user(self.edit.behavior(), seen, &edited));
        // Among keys the view showed (a rename moves none).
        let reordered = moved
            && seen
                .as_ref()
                .is_some_and(|seen| read_back::reorders(seen, &edited));
        // Which host item each item is, as the edit made it, told to the
        // merge, the writes and the checks that follow.
        let view_sources = items.map(|(told, earlier)| ItemSources {
            previous: previous_host.to_string(),
            origin: earlier.to_string(),
            items: if told.in_place() {
                HashMap::new()
            } else {
                told.items().clone()
            },
            in_place: told.in_place(),
        });
        let value = match &seen {
            // An edit where the view could not show the host's value (an
            // XML view of nested or empty lists) that does not map back onto
            // it waits: saving the view's approximation would change data.
            Some(seen) => merge_changes(
                &old,
                seen,
                &edited,
                &View {
                    unshown: &unshown,
                    repetition: self.edit.behavior().repetition(),
                    moves: moved,
                    host_reading: host_reading.as_deref(),
                    told: view_sources.as_ref(),
                },
            )
            .map_err(|pointer| {
                ConvertError::Unsupported(format!(
                    "{} cannot show the value at {pointer} exactly, so this edit of it cannot be saved: edit it as {} (at {pointer})",
                    self.edit.label(),
                    self.host.label()
                ))
            })?,
            None => edited.clone(),
        };
        // A view that writes every scalar as text (XML) only guesses types:
        // the edited values keep the host's.
        let value = match &seen {
            Some(seen) if self.edit.behavior().scalars_are_text() => {
                let record = match untouched {
                    Some(untouched) => text_scalars::Record::Kept {
                        untouched: untouched
                            .iter()
                            .map(|scalar| scalar.pointer.as_str())
                            .collect(),
                        copies: untouched
                            .iter()
                            .filter(|scalar| scalar.copied)
                            .map(|scalar| scalar.pointer.as_str())
                            .collect(),
                        reading: &view_reading,
                    },
                    None => text_scalars::Record::None,
                };
                text_scalars::in_host_types(&old, (seen, &edited), value, self.shape(), &record)
                    .map_err(|unsaved| self.unsaved_scalar(&unsaved))?
            }
            _ => value,
        };
        // A view that shows a whole float as an integer only shows it so: a
        // number typed over it stays a float, and over any float when the
        // view cannot tell the two (HCL's one number type).
        let value = match &seen {
            Some(seen) => whole_floats::kept(
                &old,
                seen,
                value,
                whole_floats::tells_floats(self.edit.behavior()),
            ),
            None => value,
        };
        // Each changed value in the host's own form (a TOML datetime read
        // back normalized), or the reason the save waits.
        let value = host_scalars(host, previous_host, &old, value)?;
        let sources = view_sources;
        let told = SaveTold::new(sources.as_ref()).with_schema(self.shape());
        // The host's text is patched to read back exactly as a full render
        // would (losses included), so its comments and formatting survive: a
        // value edit rewrites just those values, a structural one just those
        // lines. The render's text read back is that target, its keys the
        // host reads two ways spelled as the edit spells them (an XML
        // attribute `@a` whose child `a` is gone stays `@a`).
        let (rendered, target) =
            codec::render_told(&value, host, Some(previous_host), self.shape(), told)?;
        // A null the host would read back as something else (XML writes it
        // as empty text) is refused, as TOML refuses it: the save waits.
        if let Some(pointer) = codec::first_pointer(&value, &serde_json::Value::is_null)
            && target.pointer(&pointer).is_some_and(|read| !read.is_null())
        {
            return Err(SaveError::NoNull { pointer });
        }
        // So is an infinity or NaN the host has no number for (it would
        // write it as text).
        if let Some(pointer) =
            codec::first_pointer(&value, &|value| codec::non_finite::name(value).is_some())
            && target.pointer(&pointer) != value.pointer(&pointer)
        {
            return Err(SaveError::NoNonFinite { pointer });
        }
        // So is a string a host that writes scalars as text reads back as
        // another value (XML `<s>true</s>` is the boolean).
        if host.scalars_are_text()
            && let Some((pointer, read)) = host_strings::first_read_otherwise(&value, &target)
        {
            let string = value.pointer(&pointer).and_then(|value| value.as_str());
            return Err(ConvertError::Unsupported(host_strings::reason(
                self.host.label(),
                string.unwrap_or_default(),
                &read,
                &pointer,
            ))
            .into());
        }
        // The host writer may order keys its own way (TOML puts `reach = {}`
        // after arrays); the patch keeps the edit's order, the host's own.
        let target = in_order_of(target, &value);
        // So is an emptied map or list the host does not write (an HCL
        // block of labelled blocks, all gone): saved without it, the host
        // would read back otherwise than the view.
        if let Some(pointer) = dropped_empty(&value, &target, &mut String::new()) {
            return Err(SaveError::CannotKeep { pointer });
        }
        // A value the edit left as it was that the host would write
        // otherwise (a number the view showed in its own form) keeps the
        // host's text: the patches leave it, and it is no loss.
        let (target, kept) = with_untouched_kept(target, &value, &old);
        // Keys the edit put in another order are moved once the rest is
        // saved, with their lines and comments (`in_order`): the patches
        // write them in the host's order.
        let moved_to = (!codec::same_order(&target, &old)).then(|| target.clone());
        let target = match &moved_to {
            Some(_) => merge::keys_ordered_like(target, &old),
            None => target,
        };
        // A list the host held left with one item reads back as the host
        // spells it (see `collapsed`): no loss.
        let lost = |losses: &[String]| -> Vec<String> {
            losses
                .iter()
                .filter(|pointer| !collapsed::host_spelling(&old, &value, &target, pointer))
                .cloned()
                .collect()
        };
        let mut losses = lost(&rendered.losses);
        losses.retain(|pointer| !kept.contains(pointer));
        let rendered = Conversion {
            losses: lost(&rendered.losses),
            ..rendered
        };
        let shape = self.shape();
        // What the host writes once and reads in several places (a YAML
        // alias) is written out where the edit parts it from the rest; a
        // renamed key holding it parts nothing.
        let detached = shared::detached(host, previous_host, (&old, &target), shape);
        let previous_host = detached.as_deref().unwrap_or(previous_host);
        let restored =
            emptied_quotes::restored(host, previous_host, origin, (&old, &target), shape);
        let previous_host = restored.as_deref().unwrap_or(previous_host);
        // The text the save patches is the host's last text, of which the
        // sources tell, unless the save prepared it (parted a shared value,
        // put emptied quotes back): then it tells each write of a later
        // text which item is which, but not where the item is.
        let over = if detached.is_some() || restored.is_some() {
            told.later()
        } else {
            told
        };
        // The first patch that applies, of those that change only what one
        // edit touched. A patch that writes a map in another form (an HCL
        // block as an attribute) is not taken. Each patch places what it
        // writes from the text's lines split once (see `with_lines`).
        let first = |patches: &[Patch],
                     previous: &str,
                     old: &serde_json::Value,
                     target: &serde_json::Value,
                     told: SaveTold<'_>| {
            with_lines(host, previous, || {
                patches.iter().find_map(|patch| {
                    patch(host, previous, old, target, shape, told)
                        .filter(|patched| host.changed_form(previous, patched, told).is_none())
                })
            })
        };
        let precise =
            |previous: &str,
             old: &serde_json::Value,
             target: &serde_json::Value,
             told: SaveTold<'_>| first(&PRECISE, previous, old, target, told);
        let structure = |previous: &str,
                         old: &serde_json::Value,
                         target: &serde_json::Value,
                         told: SaveTold<'_>| {
            first(&[codec::patch_structure], previous, old, target, told)
        };
        // `previous` patched to read as `target`, or `Err` when items moved
        // cannot take their comments along: the save waits, rather than
        // leave the comments on the items now in their places. `told` says
        // whether `previous` is the host's last text.
        let save = |previous: &str,
                    old: &serde_json::Value,
                    target: &serde_json::Value,
                    told: SaveTold<'_>|
         -> Result<Option<String>, SaveError> {
            // A shared value this step parts from the rest written out (a
            // stage may part what the whole edit does not).
            let detached = shared::detached(host, previous, (old, target), shape);
            let told = if detached.is_some() {
                told.later()
            } else {
                told
            };
            let previous = detached.as_deref().unwrap_or(previous);
            if let Some(pointer) = with_lines(host, previous, || {
                codec::unmoved_comments(host, previous, old, target)
            }) {
                return Err(ConvertError::Unsupported(format!(
                    "these items cannot be moved with their comments: move them while editing as {} (at {pointer})",
                    self.host.label()
                ))
                .into());
            }
            // Several edits in one save: one step at a time, the first over
            // `previous`, each later one over the text the step before wrote.
            let stepwise = |previous: &str, old: &serde_json::Value, told: SaveTold<'_>| {
                let steps = codec::steps(old, target, told.sources());
                if steps.is_empty() {
                    return None;
                }
                let (mut text, mut current) = (previous.to_string(), old.clone());
                for (index, step) in steps.into_iter().enumerate() {
                    let told = if index == 0 { told } else { told.later() };
                    text = precise(&text, &current, &step, told)
                        .or_else(|| structure(&text, &current, &step, told))?;
                    current = step;
                }
                Some(text)
            };
            let whole_or_steps = |previous: &str, old: &serde_json::Value, told: SaveTold<'_>| {
                precise(previous, old, target, told).or_else(|| stepwise(previous, old, told))
            };
            // Keys renamed (the links tell which, values the rest) are
            // renamed in place first (a patch of the whole edit would write
            // a renamed map anew), then the rest is saved.
            let renames = codec::renames(old, target, told.sources());
            let renamed = (!renames.is_empty()).then(|| codec::with_renames(old, &renames, true));
            let renamed = renamed
                .filter(|renamed| renamed != target)
                .and_then(|renamed| {
                    let text = precise(previous, old, &renamed, told)?;
                    whole_or_steps(&text, &renamed, told.later())
                });
            // A step reads the host's forms from the text the one before
            // wrote: a value one step removes and another adds elsewhere
            // (an HCL expression moved to another block) is written anew
            // in a form that reads the same. The whole save made at once
            // keeps it.
            let patched = renamed
                .or_else(|| whole_or_steps(previous, old, told))
                .filter(|text| host.changed_form(previous, text, told).is_none());
            Ok(patched
                .or_else(|| structure(previous, old, target, told))
                // Block items no line patch tells apart (tagged maps)
                // removed whole.
                .or_else(|| first(&[codec::patch_removed_items], previous, old, target, told)))
        };
        // Items moved while others are added or removed: moved first (with
        // their comments), then the removed ones dropped, then the rest.
        // Which host item each item of the save is: the links first,
        // values telling the rest.
        let pairing =
            Pairing::of(Some(&old), &target, host.scalars_are_text()).told(over.over_last(), None);
        let stages = codec::move_stages(&old, &target, &pairing);
        // A stage that cannot be saved alone is saved with the rest, from
        // the stages saved before it.
        let staged = if stages.is_empty() {
            None
        } else {
            let (mut text, mut current) = (previous_host.to_string(), old.clone());
            let mut done = true;
            let mut at = over;
            for stage in &stages {
                let Some(next) = save(&text, &current, stage, at)? else {
                    break;
                };
                text = next;
                current = stage.clone();
                at = over.later();
            }
            match save(&text, &current, &target, at)? {
                Some(next) => text = next,
                None => done = false,
            }
            // Each stage reads which item is which in its own step (an
            // item changed in place is one removed and one added there):
            // the whole save must keep each item's form as the save made at
            // once does, else it is made at once.
            (done && current != old && host.changed_form(previous_host, &text, over).is_none())
                .then_some(text)
        };
        let patched = match staged {
            Some(text) => Some(text),
            None => save(previous_host, &old, &target, over)?,
        };
        // Keys the edit put in another order, besides what else it changed.
        let patched = patched.map(|text| match &moved_to {
            Some(order) => in_order(host, text, order, shape, over.later()),
            None => text,
        });
        let (conversion, as_text) = match patched {
            Some(text) => {
                let spelled = typed_spellings::kept(
                    (host, self.edit.behavior()),
                    (previous_host, edit_text),
                    (&old, &target),
                    (text, respelled),
                    shape,
                    over.sources(),
                );
                let conversion = Conversion {
                    text: spelled.text,
                    losses,
                };
                (conversion, spelled.as_text)
            }
            // No patch applies: the host is written anew, which keeps none
            // of its comments; a host with some waits.
            None => {
                if let Some(error) = written_anew::refused(
                    self.host,
                    (previous_host, &rendered.text),
                    (&old, &target),
                ) {
                    return Err(error);
                }
                #[cfg(test)]
                WHOLE_RENDERS.with(|count| count.set(count.get() + 1));
                (rendered, Vec::new())
            }
        };
        // A value the host would read back otherwise (XML writes a one-item
        // list as its item, and no empty list) waits: a save never writes
        // what the host reads back differently from the view.
        if let Some(pointer) = conversion.losses.first() {
            return Err(SaveError::CannotKeep {
                pointer: pointer.clone(),
            });
        }
        let intended = moved_to.as_ref().unwrap_or(&target);
        // A key or item the edit moved is written as where it was cut
        // from (see `moved_forms`), else the save waits.
        // Every item in place, and the save rewrote only a scalar the edit
        // changed: each is written as it was (see `moved_forms`).
        let listed = items.filter(|(told, _)| {
            !(told.in_place()
                && moved_forms::only_changed_scalar_rewritten(
                    host,
                    (previous_host, &conversion.text),
                    (&old, intended),
                ))
        });
        let conversion = match listed {
            Some((told, earlier)) => {
                let items = told.items();
                // The view's pointers as the save spells them: a view that
                // writes a list of one as its item (XML) names `/l/x` for
                // the save's `/l/0/x`. The sources name the host's paths
                // already.
                let respelled = items.keys().any(|to| intended.pointer(to).is_none());
                let view =
                    respelled.then(|| super::parse_cache::parsed(self.edit.behavior(), edit_text));
                let items: ItemMap = match view.as_ref().map(|view| view.as_ref().as_ref()) {
                    Some(Ok(view)) => {
                        let paths = links::HostPaths::between(intended, view);
                        items
                            .iter()
                            .map(|(to, source)| (paths.item(to).0, source.clone()))
                            .collect()
                    }
                    _ => items.clone(),
                };
                let text = self.moved_kept(
                    conversion.text,
                    intended,
                    &items,
                    (previous_host, &old),
                    earlier,
                    over,
                )?;
                // Values the cut or move of what they referred to wrote out
                // (YAML aliases), referring to it again where it is back:
                // each where its links put it, not a value now at its old
                // pointer. First as the host's last text wrote them, then
                // as the session's first did.
                let from_previous =
                    placed_from(&items, |source| matches!(source, ItemSource::Previous(_)));
                let text = host
                    .references_back(&text, host_previous, &from_previous)
                    .unwrap_or(text);
                let text = if origin == host_previous {
                    text
                } else {
                    let from_origin = placed_from(&items, |source| {
                        matches!(source, ItemSource::Origin(_)) && earlier == origin
                    });
                    host.references_back(&text, origin, &from_origin)
                        .unwrap_or(text)
                };
                Conversion { text, ..conversion }
            }
            None => conversion,
        };
        // Nor does it drop a comment of what the edit left alone (see
        // `kept_comments`), unless the user deleted it in the view.
        let shown_words = shown
            .as_ref()
            .map(|shown| kept_comments::comment_words(self.edit.behavior(), &shown.text))
            .unwrap_or_default();
        let deleted = |words: &str| {
            shown_words.iter().any(|shown| shown == words)
                && !kept_comments::comment_words(self.edit.behavior(), edit_text)
                    .iter()
                    .any(|now| now == words)
        };
        if let Some((pointer, words)) = kept_comments::first_lost(
            host,
            (host_previous, &conversion.text),
            (&old, intended),
            items.map(|(told, _)| {
                if told.in_place() {
                    kept_comments::Owners::InPlace
                } else {
                    kept_comments::Owners::Told(told.items())
                }
            }),
            &deleted,
        ) {
            return Err(ConvertError::Unsupported(format!(
                "this edit would drop the comment `{words}` of a line it leaves as it was: make this edit while editing as {} (at {pointer})",
                self.host.label()
            ))
            .into());
        }
        // Nor one the view still shows (its item gone, the comment's line
        // kept or pasted back): the save writes values, not the view's
        // comments, so it waits.
        if let Some(words) = kept_comments::dropped_but_shown(
            host,
            self.edit.behavior(),
            (host_previous, &conversion.text),
            edit_text,
        ) {
            return Err(ConvertError::Unsupported(format!(
                "this edit would drop the comment `{words}`, which the view still shows: delete it in the view too, or make the edit while editing as {}",
                self.host.label()
            ))
            .into());
        }
        // Nor does what is written read back otherwise, or keys in another
        // order than the user moved them to.
        let saved = read_back::Saved {
            text: &conversion.text,
            as_text: &as_text,
        };
        match read_back::first_mismatch(host, &saved, intended, reordered, shape) {
            Some(read_back::Mismatch::Value(pointer)) => {
                return Err(SaveError::CannotKeep { pointer });
            }
            Some(read_back::Mismatch::Order(pointer)) => {
                return Err(ConvertError::Unsupported(format!(
                    "{} writes this key in another place than the view moved it to, so the move cannot be saved: move it back, or edit it as {} (at {pointer})",
                    self.host.label(),
                    self.host.label()
                ))
                .into());
            }
            None => {}
        }
        // Nor is a text that would not convert back to the view (XML text
        // beside child elements, an entity only the document declares).
        codec::convertible(host, &conversion.text)?;
        self.tags_kept(previous_host, &conversion.text, (&old, &target))?;
        // The read-back check cannot tell forms that read the same (an HCL
        // expression and the literal text it reads as): the host can.
        if let Some(pointer) = host.changed_form(previous_host, &conversion.text, over) {
            return Err(self.changed_form(&pointer, intended));
        }
        Ok(conversion)
    }

    /// Why a scalar of a view that writes every scalar as text cannot be
    /// saved yet (see `text_scalars::Unsaved`).
    fn unsaved_scalar(&self, unsaved: &text_scalars::Unsaved) -> SaveError {
        let message = match unsaved {
            text_scalars::Unsaved::Emptied(pointer) => format!(
                "{} reads an empty element here, and {} holds no empty number or boolean: the edit is saved once it holds a value again (at {pointer})",
                self.edit.label(),
                self.host.label()
            ),
            text_scalars::Unsaved::Unrecorded(pointer) => format!(
                "{} reads this value as another type than {} holds, and this text came with no record of which values were edited, so the editor cannot tell whether to keep the host's type: edit the value in the view, or edit it as {} (at {pointer})",
                self.edit.label(),
                self.host.label(),
                self.host.label()
            ),
        };
        ConvertError::Unsupported(message).into()
    }

    /// The view's value (`view`, its reading) with each of `untouched` the
    /// value it stands for in its host text (`known`: a host text read
    /// already, and its value, which is not read again). An untouched
    /// scalar is named as the view's text writes it, and the reading may
    /// hold it as a list's only item (`<k3>3</k3>` where the schema wants
    /// a list reads `[3]`): moved or copied there, the host value goes in
    /// that item.
    fn host_values(
        &self,
        view: &DataReading,
        untouched: &[links::Untouched],
        known: Option<(&str, &serde_json::Value)>,
    ) -> serde_json::Value {
        let mut edited = view.value().clone();
        let mut read: Vec<(&Arc<str>, Option<std::borrow::Cow<'_, serde_json::Value>>)> =
            Vec::new();
        for scalar in untouched {
            let at = match read
                .iter()
                .position(|(host, _)| Arc::ptr_eq(host, &scalar.host))
            {
                Some(at) => at,
                None => {
                    let (text, _) = byte_order_mark::split(&scalar.host);
                    let value = match known {
                        Some((known, value)) if known == text => {
                            Some(std::borrow::Cow::Borrowed(value))
                        }
                        _ => codec::read(self.host.behavior(), text, self.shape())
                            .ok()
                            .map(std::borrow::Cow::Owned),
                    };
                    read.push((&scalar.host, value));
                    read.len() - 1
                }
            };
            let held = read[at]
                .1
                .as_deref()
                .and_then(|value| value.pointer(&scalar.from))
                .filter(|value| !value.is_object() && !value.is_array());
            // A scalar standing where its source stands (as written, or as
            // the reading's list item) is the host's value there, the list
            // the reading makes of it included: the merge keeps the host's
            // shape for it, so an unchanged view saves nothing.
            let read = view.read_pointer(&scalar.pointer);
            let at = if scalar.from == scalar.pointer || scalar.from == read {
                &scalar.pointer
            } else {
                &read
            };
            let slot = edited.pointer_mut(at);
            if let (Some(held), Some(slot)) = (held, slot) {
                *slot = held.clone();
            }
        }
        edited
    }

    /// `saved` with the scalars of each item `items` tells the edit moved
    /// written as the host wrote them where they were cut from (in the
    /// host's last text `previous`, reading `old`, or in the host text
    /// before it that the links name, `earlier`).
    fn moved_kept(
        &self,
        saved: String,
        intended: &serde_json::Value,
        items: &ItemMap,
        (previous, old): (&str, &serde_json::Value),
        origin: &str,
        told: SaveTold<'_>,
    ) -> Result<String, SaveError> {
        let host = self.host.behavior();
        let read = |text: &str| codec::read(host, text, self.shape()).ok();
        let pasted_back = items
            .values()
            .any(|source| matches!(source, ItemSource::Origin(_)));
        let origin_value = pasted_back.then(|| read(origin)).flatten();
        let sources = (
            moved_forms::Source {
                text: previous,
                value: old,
            },
            origin_value.as_ref().map(|value| moved_forms::Source {
                text: origin,
                value,
            }),
        );
        moved_forms::kept(host, saved, intended, items, sources, self.shape(), told)
            .map_err(|pointer| self.changed_form(&pointer, intended))
    }

    /// The save waits: it would write the value at `pointer` (of
    /// `intended`) in another form than the host does, said with what is
    /// wrong with it where the host knows (an HCL expression edited into
    /// one that is none).
    fn changed_form(&self, pointer: &str, intended: &serde_json::Value) -> SaveError {
        let reason = intended
            .pointer(pointer)
            .and_then(|value| self.host.behavior().changed_form_reason(value));
        let reason = reason.unwrap_or_else(|| {
            format!(
                "this edit would write the value at {pointer} in another form than {} writes it, though it reads the same: edit it as {}",
                self.host.label(),
                self.host.label()
            )
        });
        ConvertError::Unsupported(format!("{reason} (at {pointer})")).into()
    }

    /// Every tag of the host's `previous` text (YAML `!Sub`) on a value the
    /// edit kept is still on it in `saved`, where it is or where it moved,
    /// else the save waits: the edit language has no tags, so a value
    /// written anew loses its tag.
    fn tags_kept(
        &self,
        previous: &str,
        saved: &str,
        (old, target): (&serde_json::Value, &serde_json::Value),
    ) -> Result<(), SaveError> {
        let host = self.host.behavior();
        let before = host.tags(previous);
        if before.is_empty() {
            return Ok(());
        }
        let after = host.tags(saved);
        let lost = tags::first_lost(&before, &after, old, target);
        match lost {
            Some((pointer, tag)) => Err(ConvertError::Unsupported(format!(
                "{} has no tags, and this edit would write this value without its {} tag {tag}: edit it as {} (at {pointer})",
                self.edit.label(),
                self.host.label(),
                self.host.label()
            ))
            .into()),
            None => Ok(()),
        }
    }

    /// The error when the document cannot be saved. Syntax errors are left
    /// to validation.
    pub(super) fn problems(
        &self,
        edit_text: &str,
        result: &Result<String, SaveError>,
    ) -> Vec<EditAsProblem> {
        let host = self.host;
        match result {
            Ok(_) | Err(SaveError::Convert(ConvertError::Parse(_))) => Vec::new(),
            Err(error) => vec![EditAsProblem {
                at: locate_error(self.edit.behavior(), edit_text, error),
                text: EditAsText::NotSaved {
                    host,
                    error: error.clone(),
                },
            }],
        }
    }

    /// A warning on each value the edit language cannot show exactly (XML has
    /// no nested or empty lists): editing it there may change it.
    pub(super) fn view_problems(&self, shown: &Conversion) -> Vec<EditAsProblem> {
        shown
            .losses
            .iter()
            .map(|pointer| EditAsProblem {
                at: located(
                    self.edit.behavior(),
                    &shown.text,
                    pointer,
                    Severity::Warning,
                ),
                text: EditAsText::EditCannotShow {
                    pointer: pointer.clone(),
                    edit: self.edit,
                },
            })
            .collect()
    }
}

/// `text` (the host's saved text) with its maps' keys moved into
/// `target`'s order where it reads them in another; `text` itself when
/// they are in it or cannot move there.
fn in_order(
    host: &dyn ConfigLanguage,
    text: String,
    target: &serde_json::Value,
    shape: Option<&serde_json::Value>,
    told: SaveTold<'_>,
) -> String {
    let Ok(current) = codec::read(host, &text, shape) else {
        return text;
    };
    let current = host.spelled_like(&text, current, target);
    if current != *target || codec::same_order(&current, target) {
        return text;
    }
    with_lines(host, &text, || {
        codec::patch_key_order(host, &text, &current, target, shape, told)
            .filter(|moved| host.changed_form(&text, moved, told).is_none())
    })
    .unwrap_or(text)
}

/// `work` with what `host` reads of all of `text` (its lines, its scan)
/// read once for every lookup in it: a save placing many values (a
/// replace-all, a paste, a reorder) read the text from the top for each.
fn with_lines<T>(host: &dyn ConfigLanguage, text: &str, work: impl FnOnce() -> T) -> T {
    super::paths::with_spans(host, text, &[], work)
}

/// `text` (reading `old`) made to read as `new` by the first patch that
/// changes only what differs and keeps every form, as a save's single
/// step is.
fn patched(
    host: &dyn ConfigLanguage,
    text: &str,
    old: &serde_json::Value,
    new: &serde_json::Value,
    shape: Option<&serde_json::Value>,
) -> Option<String> {
    with_lines(host, text, || {
        PRECISE
            .iter()
            .chain(&[codec::patch_structure as Patch])
            .find_map(|patch| {
                patch(host, text, old, new, shape, SaveTold::default()).filter(|patched| {
                    host.changed_form(text, patched, SaveTold::default())
                        .is_none()
                })
            })
    })
}

/// `value` with each scalar changed from `old` as the host saves it there
/// ([`SaveLayout::scalar_edit`]), or the reason the save waits.
fn host_scalars(
    host: &dyn ConfigLanguage,
    previous: &str,
    old: &serde_json::Value,
    mut value: serde_json::Value,
) -> Result<serde_json::Value, SaveError> {
    let mut changed = Vec::new();
    changed_scalars(old, &value, String::new(), &mut changed);
    with_lines(host, previous, || {
        for pointer in changed {
            let Some(slot) = value.pointer_mut(&pointer) else {
                continue;
            };
            match host.scalar_edit(previous, &FieldPath::from(&pointer), slot) {
                Ok(Some(own)) => *slot = own,
                Ok(None) => {}
                Err(reason) => {
                    return Err(
                        ConvertError::Unsupported(format!("{reason} (at {pointer})")).into(),
                    );
                }
            }
        }
        Ok(value)
    })
}

/// The items of a save whose source `items` (their links) tell is `from`
/// (in one host text), as (pointer in the save, pointer in that text).
fn placed_from(items: &ItemMap, from: impl Fn(&ItemSource) -> bool) -> Vec<(String, String)> {
    items
        .iter()
        .filter(|(_, source)| from(source))
        .map(|(to, source)| (to.clone(), source.pointer().to_string()))
        .collect()
}

/// `target` (what the host reads back from writing `value`) with the host's
/// own `old` value wherever `value` keeps it but the host would write it
/// otherwise, and those pointers.
fn with_untouched_kept(
    mut target: serde_json::Value,
    value: &serde_json::Value,
    old: &serde_json::Value,
) -> (serde_json::Value, Vec<String>) {
    let mut differing = Vec::new();
    codec::differences(value, &target, String::new(), &mut differing);
    let mut kept = Vec::new();
    for pointer in differing {
        let at = if pointer == "/" { "" } else { pointer.as_str() };
        let Some(own) = old
            .pointer(at)
            .filter(|own| value.pointer(at) == Some(*own))
        else {
            continue;
        };
        if let Some(slot) = target.pointer_mut(at) {
            *slot = own.clone();
            kept.push(pointer);
        }
    }
    (target, kept)
}

/// The pointers of the scalars `new` changes where `old` holds one too.
fn changed_scalars(
    old: &serde_json::Value,
    new: &serde_json::Value,
    pointer: String,
    out: &mut Vec<String>,
) {
    use serde_json::Value;
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in right {
                if let Some(before) = left.get(key) {
                    let child = format!("{pointer}/{}", pointer_key(key));
                    changed_scalars(before, value, child, out);
                }
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (before, value)) in left.iter().zip(right).enumerate() {
                changed_scalars(before, value, format!("{pointer}/{index}"), out);
            }
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => {}
        _ if old != new => out.push(pointer),
        _ => {}
    }
}

/// Where an error is, on the value it names (`… (at /pointer)`) when it
/// names one.
pub(super) fn locate_error(
    behavior: &dyn ConfigLanguage,
    source: &str,
    error: &SaveError,
) -> Diagnostic {
    // A syntax error of `source` itself (a switch waiting on it) is placed
    // where its message says, beside the syntax problem.
    if let SaveError::Convert(convert) = error
        && let Some((line, column)) = syntax_error_place(convert)
    {
        return Diagnostic {
            line: Some(line),
            column: Some(column),
            end_column: Some(column + 1),
            severity: Severity::Error,
            ..Diagnostic::default()
        };
    }
    located(behavior, source, &error.pointer(), Severity::Error)
}

/// The line and column a syntax error names.
fn syntax_error_place(error: &ConvertError) -> Option<(usize, usize)> {
    match error {
        ConvertError::Parse(parse) => Some((parse.line?, parse.column.unwrap_or(1))),
        _ => None,
    }
}

/// A place on the value at `pointer` in `source`, or on the document's
/// first content when the value has no span (the message is left for the
/// problem to word).
fn located(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
    severity: Severity,
) -> Diagnostic {
    // A value missing from the text (the view dropped it) is placed on its
    // nearest ancestor that is there; a value inside an ancestor written
    // between brackets (an item of a TOML or HCL `[…]`), which the
    // language's own lookup does not reach, on itself.
    let wanted = FieldPath::from(pointer).texts();
    let mut at = pointer;
    let span = loop {
        let path = FieldPath::from(at);
        if let Some(span) = behavior
            .value_span(source, &path)
            .or_else(|| behavior.key_span(source, &path))
        {
            let rest = wanted.get(path.segments().len()..).unwrap_or_default();
            let inside = (!rest.is_empty())
                .then(|| codec::in_flow_container(behavior, source, span, rest, false))
                .flatten();
            break Some(inside.unwrap_or(span));
        }
        match at.rsplit_once('/') {
            Some((parent, _)) if !parent.is_empty() => at = parent,
            _ => break behavior.first_span(source),
        }
    };
    Diagnostic {
        line: span.map(|span| span.line),
        column: span.map(|span| span.start_column),
        end_column: span.map(|span| span.end_column),
        severity,
        ..Diagnostic::default()
    }
}

// Conversion tests pair languages, so they need every built-in one.

/// The pointer of the first empty map or list of `value` (at `pointer`)
/// `target` (the host's reading of it written) holds nothing at.
fn dropped_empty(
    value: &serde_json::Value,
    target: &serde_json::Value,
    pointer: &mut String,
) -> Option<String> {
    use serde_json::Value;
    let empty = match value {
        Value::Object(map) => map.is_empty(),
        Value::Array(items) => items.is_empty(),
        _ => return None,
    };
    if empty {
        return target.pointer(pointer).is_none().then(|| pointer.clone());
    }
    let length = pointer.len();
    let children: Vec<(String, &Value)> = match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, child)| (pointer_key(key), child))
            .collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, child)| (index.to_string(), child))
            .collect(),
        _ => Vec::new(),
    };
    for (segment, child) in children {
        pointer.push('/');
        pointer.push_str(&segment);
        let found = dropped_empty(child, target, pointer);
        pointer.truncate(length);
        if found.is_some() {
            return found;
        }
    }
    None
}

/// `value` with each object's keys in `order`'s order (keys `order` lacks
/// last, as they were).
fn in_order_of(value: serde_json::Value, order: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match (value, order) {
        (Value::Object(mut map), Value::Object(reference)) => {
            let mut ordered = serde_json::Map::new();
            for (key, child_order) in reference {
                if let Some(child) = map.remove(key) {
                    ordered.insert(key.clone(), in_order_of(child, child_order));
                }
            }
            ordered.extend(map);
            Value::Object(ordered)
        }
        (Value::Array(items), Value::Array(reference)) => Value::Array(
            items
                .into_iter()
                .enumerate()
                .map(|(index, item)| match reference.get(index) {
                    Some(child_order) => in_order_of(item, child_order),
                    None => item,
                })
                .collect(),
        ),
        (value, _) => value,
    }
}

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
#[allow(clippy::string_slice, reason = "the tests cut their own texts")]
mod tests {
    use super::super::Language;
    use super::super::diagnostics::{Diagnostic, Severity};
    use super::super::edit_as::EditAsProblem;
    use super::Bridge;

    fn worded(problems: Vec<EditAsProblem>) -> Vec<Diagnostic> {
        problems
            .iter()
            .map(|problem| problem.worded(crate::strings::default_strings()))
            .collect()
    }

    #[test]
    fn the_same_language_needs_no_bridge() {
        assert!(Bridge::new(Language::YAML, Language::YAML, None).is_none());
    }

    #[test]
    fn host_text_is_shown_converted_and_edits_convert_back_keeping_framing() {
        let bridge = Bridge::new(Language::XML, Language::HCL, None).unwrap();
        let host = "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>7835</listen>\n</burrow>\n";
        let shown = bridge.to_edit(host).unwrap();
        assert_eq!(shown.text, "listen = 7835\n");
        let saved = bridge.to_host("listen = 9000\n", host).unwrap();
        assert_eq!(
            saved,
            "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>9000</listen>\n</burrow>\n"
        );
    }

    #[test]
    fn a_value_the_host_cannot_carry_is_an_error_on_that_value_and_not_saved() {
        let bridge = Bridge::new(Language::XML, Language::YAML, None).unwrap();
        let edit = "hosts:\n  - only.example\nport: 80\n";
        let result = bridge.to_host(edit, "");
        assert!(result.is_err(), "{result:?}");
        let problems = worded(bridge.problems(edit, &result));
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].severity, Severity::Error);
        assert_eq!(problems[0].line, Some(1));
        assert_eq!(
            problems[0].message,
            "Not saved as XML: /hosts: XML cannot keep this value exactly, as it reads back differently: change it, or edit it as XML, to save"
        );
    }

    #[test]
    fn a_document_the_host_cannot_express_is_an_error_and_not_saved() {
        let bridge = Bridge::new(Language::TOML, Language::JSON, None).unwrap();
        let edit = "{\n  \"https\": null\n}\n";
        let result = bridge.to_host(edit, "");
        assert!(result.is_err());
        let problems = worded(bridge.problems(edit, &result));
        assert_eq!(problems[0].severity, Severity::Error);
        assert!(
            problems[0].message.starts_with("Not saved as TOML"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn syntax_errors_are_left_to_validation() {
        let bridge = Bridge::new(Language::JSON, Language::YAML, None).unwrap();
        let edit = "a: [\n";
        let result = bridge.to_host(edit, "");
        assert!(bridge.problems(edit, &result).is_empty());
    }

    /// Host documents with comments and hand formatting, the port to change,
    /// and the same document with only the port changed.
    const HOSTS: [(Language, &str, &str); 5] = [
        (
            Language::YAML,
            "# Burrow\nlisten: 7835 # the port\npublic_address: a.example\npool: {min_idle: 4, max_idle: 64}\n",
            "# Burrow\nlisten: 9000 # the port\npublic_address: a.example\npool: {min_idle: 4, max_idle: 64}\n",
        ),
        (
            Language::TOML,
            "# Burrow\nlisten = 7835 # the port\npublic_address = \"a.example\"\npool = { min_idle = 4, max_idle = 64 }\n",
            "# Burrow\nlisten = 9000 # the port\npublic_address = \"a.example\"\npool = { min_idle = 4, max_idle = 64 }\n",
        ),
        (
            Language::HCL,
            "# Burrow\nlisten = 7835 // the port\npublic_address = \"a.example\"\npool = { min_idle = 4, max_idle = 64 }\n",
            "# Burrow\nlisten = 9000 // the port\npublic_address = \"a.example\"\npool = { min_idle = 4, max_idle = 64 }\n",
        ),
        (
            Language::XML,
            "<!-- Burrow -->\n<listen>7835</listen> <!-- the port -->\n<public_address>a.example</public_address>\n",
            "<!-- Burrow -->\n<listen>9000</listen> <!-- the port -->\n<public_address>a.example</public_address>\n",
        ),
        (
            Language::JSON,
            "{ \"public_address\": \"a.example\", \"listen\": 7835,\n  \"pool\": {\"min_idle\": 4} }\n",
            "{ \"public_address\": \"a.example\", \"listen\": 9000,\n  \"pool\": {\"min_idle\": 4} }\n",
        ),
    ];

    /// The XML view repeats elements for nested lists and shows a one-item
    /// list as its item: a value edited there lands in its place in the
    /// host, the nesting and the list kept.
    #[cfg(all(feature = "yaml", feature = "xml"))]
    #[test]
    fn a_value_edited_in_a_view_without_nested_lists_keeps_the_nesting() {
        let host = "# c\nm:\n  - - a # ac\n    - b\n  - - - e\n    - f\n  - x\ndeep:\n  - k: v # kc\nn: 1 # n\n";
        let bridge = Bridge::new(Language::YAML, Language::XML, None).unwrap();
        for (from, to, expected) in [
            (">b<", ">B<", host.replace("- b", "- B")),
            (">e<", ">E<", host.replace("- e", "- E")),
            (">f<", ">F<", host.replace("- f", "- F")),
            (">x<", ">X<", host.replace("- x", "- X")),
            (">v<", ">w<", host.replace("k: v", "k: w")),
        ] {
            let shown = bridge.to_edit(host).unwrap().text;
            assert!(shown.contains(from), "{from} in {shown}");
            let saved = bridge.to_host(&shown.replace(from, to), host).unwrap();
            assert_eq!(saved, expected, "{from} -> {to}");
        }
    }

    #[test]
    fn a_value_edit_changes_only_that_value_in_the_host_text() {
        for (host, original, expected) in HOSTS {
            for &edit in Language::ALL.iter().filter(|edit| **edit != host) {
                let bridge = Bridge::new(host, edit, None).unwrap();
                let shown = bridge.to_edit(original).unwrap().text;
                let Some(at) = shown.find("7835") else {
                    panic!("{host} -> {edit}: port not shown in {shown}");
                };
                let edited = format!("{}9000{}", &shown[..at], &shown[at + 4..]);
                let saved = bridge.to_host(&edited, original).unwrap();
                assert_eq!(saved, expected, "{host} edited as {edit}");
            }
        }
    }

    #[test]
    fn a_structural_edit_adds_only_its_line_and_keeps_the_comment() {
        let bridge = Bridge::new(Language::YAML, Language::JSON, None).unwrap();
        let saved = bridge
            .to_host(
                "{\"listen\": 1, \"public_address\": \"b\"}",
                "listen: 1 # c\n",
            )
            .unwrap();
        assert_eq!(saved, "listen: 1 # c\npublic_address: b\n");
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn one_item_arrays_round_trip_through_xml_with_a_schema() {
        let schema = super::super::schema::build_schema(include_str!(
            "../../tests/fixtures/tunnel.schema.json"
        ))
        .unwrap();
        let host = "web:\n  letsencrypt:\n    email: a@b.c\n    dns:\n      - domains:\n          - a.example\n        cloudflare:\n          token: x\n";
        let bridge = Bridge::new(
            Language::YAML,
            Language::XML,
            Some(std::sync::Arc::new(schema)),
        )
        .unwrap();
        let shown = bridge.to_edit(host).unwrap();
        assert_eq!(shown.losses, Vec::<String>::new(), "{}", shown.text);
        let edited = shown.text.replace(">x<", ">z<");
        let saved = bridge.to_host(&edited, host).unwrap();
        assert_eq!(saved, host.replace("token: x", "token: z"));
    }

    #[test]
    fn adding_or_removing_a_key_keeps_the_hosts_comments() {
        let hosts = [
            (
                Language::YAML,
                "# Burrow server\nlisten: 7835 # port\npublic_address: a.example\n",
            ),
            (
                Language::TOML,
                "# Burrow server\nlisten = 7835 # port\npublic_address = \"a.example\"\n",
            ),
            (
                Language::HCL,
                "# Burrow server\nlisten = 7835 # port\npublic_address = \"a.example\"\n",
            ),
        ];
        for (host, text) in hosts {
            for edit in [Language::JSON, Language::YAML, Language::TOML] {
                let Some(bridge) = Bridge::new(host, edit, None) else {
                    continue;
                };
                let shown = bridge.to_edit(text).unwrap().text;
                // Add a key.
                let added_value = {
                    let mut value = edit.parse(&shown).unwrap();
                    value["insecure_plaintext"] = serde_json::Value::Bool(true);
                    edit.serialize(&value).unwrap()
                };
                let saved = bridge.to_host(&added_value, text).unwrap();
                assert!(
                    saved.contains("# Burrow server") && saved.contains("# port"),
                    "{host} as {edit} add:\n{saved}"
                );
                assert!(saved.contains("insecure_plaintext"), "{saved}");
                // Remove a key.
                let removed_value = {
                    let mut value = edit.parse(&shown).unwrap();
                    value.as_object_mut().unwrap().remove("public_address");
                    edit.serialize(&value).unwrap()
                };
                let saved = bridge.to_host(&removed_value, text).unwrap();
                assert!(
                    saved.contains("# Burrow server") && saved.contains("# port"),
                    "{host} as {edit} remove:\n{saved}"
                );
                assert!(!saved.contains("public_address"), "{saved}");
            }
        }
    }

    #[test]
    fn values_the_edit_language_cannot_tell_apart_keep_the_hosts_own() {
        let bridge = Bridge::new(Language::YAML, Language::XML, None).unwrap();
        let host = "public_address: null\nname: \"\"\nflag: \"true\"\ncount: \"1\"\ninsecure_plaintext: false\n";
        let shown = bridge.to_edit(host).unwrap().text;
        let edited = shown.replace(">false<", ">true<");
        let saved = bridge.to_host(&edited, host).unwrap();
        assert_eq!(
            saved,
            host.replace("insecure_plaintext: false", "insecure_plaintext: true")
        );
    }

    #[test]
    fn a_key_added_after_a_line_with_a_trailing_comment_keeps_the_comments() {
        for (host_language, host) in [
            (
                Language::YAML,
                "# Burrow\nlisten: 7835 # main port\ninsecure_plaintext: false\n",
            ),
            (
                Language::TOML,
                "# Burrow\nlisten = 7835 # main port\ninsecure_plaintext = false\n",
            ),
            (
                Language::HCL,
                "# Burrow\nlisten = 7835 # main port\ninsecure_plaintext = false\n",
            ),
        ] {
            let bridge = Bridge::new(host_language, Language::JSON, None).unwrap();
            let shown = bridge.to_edit(host).unwrap().text;
            let edited = shown.replace(
                "\"listen\": 7835,",
                "\"listen\": 7835,\n  \"public_address\": \"x\",",
            );
            let saved = bridge.to_host(&edited, host).unwrap();
            assert!(
                saved.contains("# Burrow") && saved.contains("# main port"),
                "{host_language}:\n{saved}"
            );
            assert!(saved.contains("public_address"), "{saved}");
        }
    }

    #[test]
    fn a_value_edit_inside_a_flow_map_keeps_the_comments() {
        let bridge = Bridge::new(Language::YAML, Language::JSON, None).unwrap();
        let host = "# head\npool: {min_idle: 4, max_idle: 8} # tuned\nlisten: 7835\n";
        let shown = bridge.to_edit(host).unwrap().text;
        let edited = shown.replace("\"max_idle\": 8", "\"max_idle\": 9");
        let saved = bridge.to_host(&edited, host).unwrap();
        assert_eq!(saved, host.replace("max_idle: 8", "max_idle: 9"));
    }

    #[test]
    fn renaming_a_key_keeps_its_trailing_comment() {
        let bridge = Bridge::new(Language::YAML, Language::JSON, None).unwrap();
        let host = "# head\nlisten: 7835 # main port\ninsecure_plaintext: false\n";
        let shown = bridge.to_edit(host).unwrap().text;
        let edited = shown.replace("\"listen\"", "\"listen_port\"");
        let saved = bridge.to_host(&edited, host).unwrap();
        assert!(saved.contains("# head"), "{saved}");
        assert!(saved.contains("listen_port: 7835 # main port"), "{saved}");
    }
}

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod structural_saves {
    use super::*;

    fn worded(problems: Vec<EditAsProblem>) -> Vec<Diagnostic> {
        problems
            .iter()
            .map(|problem| problem.worded(crate::strings::default_strings()))
            .collect()
    }

    /// The host text saved after `change` is made to the shown text.
    fn save(
        host: Language,
        edit: Language,
        host_text: &str,
        change: impl Fn(&str) -> String,
    ) -> String {
        let bridge = Bridge::new(host, edit, None).unwrap();
        let shown = bridge.to_edit(host_text).unwrap().text;
        bridge.to_host(&change(&shown), host_text).unwrap()
    }

    #[test]
    fn deleting_a_table_removes_it_and_its_comment_only() {
        let toml = "# head\nlisten = 7835 # port\n\n# acme client\n[clients.acme]\nsecret = \"s1\" # the secret\n\n# beta client\n[clients.beta]\nsecret = \"s2\"\n";
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("  beta:\n    secret: s2\n", "")
        });
        assert_eq!(
            saved,
            "# head\nlisten = 7835 # port\n\n# acme client\n[clients.acme]\nsecret = \"s1\" # the secret\n"
        );
    }

    #[test]
    fn deleting_a_block_removes_it_and_its_comment_only() {
        let hcl = "# head\nlisten = 7835 # port\n\nclients {\n  # acme\n  acme {\n    secret = \"s1\" # s\n  }\n\n  # beta\n  beta {\n    secret = \"s2\"\n  }\n}\n";
        let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
            shown.replace("  beta:\n    secret: s2\n", "")
        });
        assert_eq!(
            saved,
            "# head\nlisten = 7835 # port\n\nclients {\n  # acme\n  acme {\n    secret = \"s1\" # s\n  }\n}\n"
        );
    }

    #[test]
    fn an_edit_language_with_its_own_key_order_keeps_the_hosts() {
        let yaml = "# head\nclients:\n  acme:\n    secret: s1\npublic_address: x.example # addr\nlisten: 1 # port\n";
        let saved = save(Language::YAML, Language::TOML, yaml, |shown| {
            shown.replace(
                "secret = \"s1\"\n",
                "secret = \"s1\"\nmax_connections = 3\n",
            )
        });
        assert_eq!(
            saved,
            "# head\nclients:\n  acme:\n    secret: s1\n    max_connections: 3\npublic_address: x.example # addr\nlisten: 1 # port\n"
        );
    }

    #[test]
    fn adding_to_an_inline_container_rewrites_only_that_container() {
        let flow = "# head\ntcp: {nodelay: true} # tcp\nlisten: 1 # port\n";
        let saved = save(Language::YAML, Language::JSON, flow, |shown| {
            shown.replace(
                "\"nodelay\": true",
                "\"nodelay\": true,\n    \"keepalive_secs\": 3",
            )
        });
        assert_eq!(
            saved,
            "# head\ntcp: {nodelay: true, keepalive_secs: 3} # tcp\nlisten: 1 # port\n"
        );
        let sequence = "# head\nallow: [a, b] # s\nlisten: 1\n";
        let saved = save(Language::YAML, Language::JSON, sequence, |shown| {
            shown.replace("\"b\"", "\"b\",\n    \"c\"")
        });
        assert_eq!(saved, "# head\nallow: [a, b, c] # s\nlisten: 1\n");
        let inline = "# head\ntcp = { nodelay = true } # tcp\nlisten = 1\n";
        let saved = save(Language::TOML, Language::YAML, inline, |shown| {
            shown.replace("nodelay: true", "nodelay: true\n  keepalive_secs: 3")
        });
        assert_eq!(
            saved,
            "# head\ntcp = { nodelay = true, keepalive_secs = 3 } # tcp\nlisten = 1\n"
        );
    }

    #[cfg(feature = "schema-validation")]
    #[test]
    fn a_value_in_a_lone_block_read_as_a_list_changes_only_that_value() {
        let schema = super::super::schema::build_schema(include_str!(
            "../../tests/fixtures/tunnel.schema.json"
        ))
        .unwrap();
        let bridge = Bridge::new(
            Language::HCL,
            Language::YAML,
            Some(std::sync::Arc::new(schema)),
        )
        .unwrap();
        let host = "web {\n  # certs\n  certificates {\n    hosts = [\"a\"]\n    cert = \"c\" # c\n  }\n}\n";
        let shown = bridge.to_edit(host).unwrap().text;
        assert!(shown.contains("cert: c"), "{shown}");
        let saved = bridge
            .to_host(&shown.replace("cert: c", "cert: d"), host)
            .unwrap();
        assert_eq!(saved, host.replace("cert = \"c\"", "cert = \"d\""));
    }

    #[test]
    fn a_value_in_multi_line_string_syntax_keeps_what_follows_it() {
        let toml = "public_address = \'\'\'ab\'\'\' # real\nlisten = 1\n";
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("public_address: ab", "public_address: cd")
        });
        assert_eq!(saved, "public_address = '''cd''' # real\nlisten = 1\n");
        let hcl = "public_address = <<EOT\nab\nEOT\n\n# after\nlisten = 1\n";
        let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
            shown.replace("public_address: |\n  ab", "public_address: |\n  cd")
        });
        assert_eq!(
            saved,
            "public_address = <<EOT\ncd\nEOT\n\n# after\nlisten = 1\n"
        );
        let yaml = "public_address: >- # c\n  ab\nlisten: 1\n";
        let saved = save(Language::YAML, Language::JSON, yaml, |shown| {
            shown.replace("\"ab\"", "\"cd\"")
        });
        assert_eq!(saved, "public_address: >- # c\n  cd\nlisten: 1\n");
    }

    #[test]
    fn values_the_view_cannot_show_are_warned_about() {
        let bridge = Bridge::new(Language::YAML, Language::XML, None).unwrap();
        let shown = bridge
            .to_edit("listen: 1\nx:\n  - [1, 2]\n  - [3]\nallow: []\n")
            .unwrap();
        let warnings = worded(bridge.view_problems(&shown));
        let messages: Vec<&str> = warnings
            .iter()
            .map(|warning| warning.message.as_str())
            .collect();
        assert!(
            messages.iter().any(|message| message.starts_with("/x")),
            "{messages:?}\n{}",
            shown.text
        );
        assert!(
            messages.iter().any(|message| message.starts_with("/allow")),
            "{messages:?}\n{}",
            shown.text
        );
        assert!(
            warnings
                .iter()
                .all(|warning| warning.severity == Severity::Warning)
        );
    }

    #[test]
    fn xml_attributes_stay_attributes() {
        let xml = "<!-- head -->\n<services>\n  <web mode=\"balance\" target=\"80\">\n    <!-- c -->\n    <client>acme</client>\n  </web>\n</services>\n<listen>1</listen>\n";
        let saved = save(Language::XML, Language::YAML, xml, |shown| {
            shown.replace("mode: balance", "mode: failover")
        });
        assert_eq!(saved, xml.replace("mode=\"balance\"", "mode=\"failover\""));
        let saved = save(Language::XML, Language::YAML, xml, |shown| {
            shown.replace(
                "    client: acme\n",
                "    client: acme\n    bandwidth: 1m\n",
            )
        });
        assert!(
            saved.contains("<!-- head -->") && saved.contains("<!-- c -->"),
            "{saved}"
        );
        assert!(
            saved.contains("<web mode=\"balance\" target=\"80\">"),
            "{saved}"
        );
        assert!(saved.contains("<bandwidth>1m</bandwidth>"), "{saved}");
    }

    #[test]
    fn toml_dotted_keys_stay_dotted() {
        let toml = "# head\nlisten = 7835 # port\nclients.acme.secret = \"a\" # sa\n\n# limits\n[limits]\nmax_pending = 5\n";
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("    secret: a\n", "    secret: a\n    max_connections: 2\n")
        });
        assert!(
            saved.contains("# head") && saved.contains("# sa") && saved.contains("# limits"),
            "{saved}"
        );
        assert!(
            saved.contains("clients.acme.secret = \"a\" # sa\n"),
            "{saved}"
        );
        assert!(
            saved.contains("clients.acme.max_connections = 2\n"),
            "{saved}"
        );
        assert!(!saved.contains("[clients"), "{saved}");
    }

    #[test]
    fn hcl_labelled_blocks_stay_labelled() {
        let hcl = "# header\nlisten = 7835 // port\n\n# acme\nclients \"acme\" {\n  secret = \"abc\" # s\n}\n\nservices \"web\" {\n  client = \"acme\"\n  target = 8080\n}\n";
        let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
            shown.replace(
                "    target: 8080\n",
                "    target: 8080\n    mode: balance\n",
            )
        });
        assert_eq!(
            saved,
            hcl.replace(
                "  target = 8080\n",
                "  target = 8080\n  mode   = \"balance\"\n"
            )
            .replace("mode   =", "mode ="),
            "{saved}"
        );
    }

    #[test]
    fn a_json_edit_at_the_end_of_an_object_changes_only_its_lines() {
        let json = "{\n    \"listen\": 7835,\n    \"clients\": {\"acme\": {\"secret\": \"abc\"}},\n    \"pool\": {\n        \"min_idle\": 4,\n        \"max_idle\": 8\n    }\n}\n";
        let saved = save(Language::JSON, Language::YAML, json, |shown| {
            shown.replace("  max_idle: 8\n", "")
        });
        assert_eq!(
            saved,
            "{\n    \"listen\": 7835,\n    \"clients\": {\"acme\": {\"secret\": \"abc\"}},\n    \"pool\": {\n        \"min_idle\": 4\n    }\n}\n"
        );
    }

    #[test]
    fn renaming_a_table_keeps_the_comment_above_it() {
        let toml = "listen = 7835\n\n# heartbeat section\n[heartbeat]\ninterval_secs = 15\n\n# acme client\n[clients.acme]\nsecret = \"a\"\n";
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("  acme:", "  acme2:")
        });
        assert_eq!(saved, toml.replace("[clients.acme]", "[clients.acme2]"));
        let saved = save(Language::TOML, Language::YAML, toml, |shown| {
            shown.replace("heartbeat:", "limits:")
        });
        assert_eq!(saved, toml.replace("[heartbeat]", "[limits]"));
    }

    #[test]
    fn a_deleted_nested_entry_takes_its_own_comment() {
        let hcl = "clients {\n  # acme\n  acme {\n    secret = \"a\"\n  }\n\n  # beta\n  beta {\n    secret = \"b\"\n  }\n}\n";
        let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
            shown.replace("  acme:\n    secret: a\n", "")
        });
        assert_eq!(
            saved,
            "clients {\n  # beta\n  beta {\n    secret = \"b\"\n  }\n}\n"
        );
        let yaml =
            "# head\nlisten: 1\n# public comment\npublic_address: x\ninsecure_plaintext: false\n";
        let saved = save(Language::YAML, Language::JSON, yaml, |shown| {
            shown.replace("  \"public_address\": \"x\",\n", "")
        });
        assert_eq!(saved, "# head\nlisten: 1\ninsecure_plaintext: false\n");
    }

    #[test]
    fn deleting_the_first_of_repeated_tables_removes_that_one() {
        let toml = "listen = 1\n\n# first\n[[web.certificates]]\nhosts = [\"a\"] # n\ncert = \"a.pem\"\n\n# second\n[[web.certificates]]\nhosts = [\"b\"]\n";
        let saved = save(Language::TOML, Language::JSON, toml, |shown| {
            let value: serde_json::Value = serde_json::from_str(shown).unwrap();
            let mut value = value;
            value["web"]["certificates"]
                .as_array_mut()
                .unwrap()
                .remove(0);
            serde_json::to_string_pretty(&value).unwrap()
        });
        assert_eq!(
            saved,
            "listen = 1\n\n# second\n[[web.certificates]]\nhosts = [\"b\"]\n"
        );
    }

    #[test]
    fn a_table_inserted_between_two_gets_one_blank_line_each_side() {
        let toml = "[clients.acme]\nsecret = \"abc\"\n\n# beta\n[clients.beta]\nsecret = \"b\"\n";
        let saved = save(Language::TOML, Language::JSON, toml, |shown| {
            let mut value: serde_json::Value = serde_json::from_str(shown).unwrap();
            let clients = value["clients"].as_object_mut().unwrap();
            let beta = clients.remove("beta").unwrap();
            clients.insert("gamma".into(), serde_json::json!({"secret": "g"}));
            clients.insert("beta".into(), beta);
            serde_json::to_string_pretty(&value).unwrap()
        });
        assert_eq!(
            saved,
            "[clients.acme]\nsecret = \"abc\"\n\n[clients.gamma]\nsecret = \"g\"\n\n# beta\n[clients.beta]\nsecret = \"b\"\n"
        );
    }

    #[test]
    fn an_empty_host_filled_through_edit_as_is_written_whole() {
        let saved = save(Language::TOML, Language::YAML, "", |_| {
            "listen: 1\nheartbeat:\n  interval_secs: 3\n".to_string()
        });
        assert_eq!(saved, "listen = 1\n\n[heartbeat]\ninterval_secs = 3\n");
    }

    #[test]
    fn a_warning_for_a_value_the_view_drops_sits_on_its_nearest_shown_ancestor() {
        let bridge = Bridge::new(Language::JSON, Language::XML, None).unwrap();
        let shown = bridge
            .to_edit("{\"listen\": 99999, \"self\": {\"allow\": []}}")
            .unwrap();
        let warnings = worded(bridge.view_problems(&shown));
        let allow = warnings
            .iter()
            .find(|warning| warning.message.starts_with("/self/allow"))
            .expect("a warning for /self/allow");
        let line = shown.text.lines().nth(allow.line.unwrap() - 1).unwrap();
        assert!(line.contains("self"), "{line:?} in\n{}", shown.text);
    }

    #[test]
    fn the_first_key_of_a_comment_only_document_goes_under_its_header() {
        let saved = save(Language::YAML, Language::JSON, "# header\n", |_| {
            "{\"listen\": 1}".to_string()
        });
        assert_eq!(saved, "# header\nlisten: 1\n");
    }
}
