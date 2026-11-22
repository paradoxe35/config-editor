//! The names a documents view knows for a field whose schema says its value
//! names a key defined elsewhere (`x-config-editor-keys`): per open
//! document, the keys at each pointer a field asked for, read off its
//! current text; the host's names for the files that are not open; and the
//! lookup that puts them together for one field.
//!
//! Which documents count is never guessed: a field of a document with
//! schema S, asking pointer P from the schemas F, reads the documents whose
//! [`SchemaIdentity`] is S or one of F.
//!
//! The index is lazy: an edit only marks its document as changed, and the
//! next lookup reads that one document's keys again (from the parse its
//! editor keeps when there is one), every other document answering from
//! what it read before. A text that does not parse keeps the names of its
//! last text that did.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use serde_json::Value;

use crate::editor::key_names::{KeyNameSource, SchemaIdentity, keys_in};
use crate::editor::work_count::{Work, note};
use crate::{KeyName, KeyNames, Language, ParseError, SchemaSource};

/// The identity of a document's schema as the view resolved it: none
/// without a schema, or for one that does not compile (it supplies no
/// names).
pub(crate) fn identity_of(source: Option<&SchemaSource>) -> Option<SchemaIdentity> {
    match source? {
        SchemaSource::Compiled(schema) => Some(schema.bundle.identity.clone()),
        _ => None,
    }
}

/// One open document as the index knows it.
struct Indexed {
    file_name: String,
    identity: Option<SchemaIdentity>,
    language: Language,
    /// Its text changed since its keys were read.
    changed: bool,
    /// The keys at each pointer asked, from its last text that parsed.
    keys: Vec<(String, Rc<[String]>)>,
}

/// The parse of an open document's current text (by id), from the editor's
/// when it keeps one; `None` when the document is not open.
pub(crate) type ParseOf<'a> =
    dyn FnMut(&str, Language) -> Option<Rc<Result<Value, ParseError>>> + 'a;

/// A document as the view lists it, for [`NamesIndex::list`].
pub(crate) struct Listed<'a> {
    pub(crate) id: &'a str,
    pub(crate) file_name: String,
    pub(crate) identity: Option<SchemaIdentity>,
    pub(crate) language: Language,
}

/// The keys of every open document at the pointers asked (see the module
/// docs).
#[derive(Default)]
pub(crate) struct NamesIndex {
    /// The open documents, in tab order.
    order: Vec<String>,
    documents: HashMap<String, Indexed>,
}

impl NamesIndex {
    /// Takes the view's list: a document no longer listed goes with its
    /// names; one whose schema or language changed is read again.
    pub(crate) fn list(&mut self, listed: Vec<Listed<'_>>) {
        let mut documents = HashMap::with_capacity(listed.len());
        let mut order = Vec::with_capacity(listed.len());
        for document in listed {
            let indexed = match self.documents.remove(document.id) {
                Some(mut known)
                    if known.identity == document.identity
                        && known.language == document.language =>
                {
                    known.file_name = document.file_name;
                    known
                }
                _ => Indexed {
                    file_name: document.file_name,
                    identity: document.identity,
                    language: document.language,
                    changed: true,
                    keys: Vec::new(),
                },
            };
            order.push(document.id.to_string());
            documents.insert(document.id.to_string(), indexed);
        }
        self.order = order;
        self.documents = documents;
    }

    /// The text of document `id` changed: its keys are read again when
    /// next asked.
    pub(crate) fn edited(&mut self, id: &str) {
        if let Some(document) = self.documents.get_mut(id) {
            document.changed = true;
        }
    }

    /// The names at `pointer` of the documents of schema `schema`: the
    /// open documents' (but `except`, the asking document, whose editor
    /// reads its own keys), in tab order, then the host's `given` entries,
    /// each name once. A host's name whose `source` is an open document is
    /// left out: that document's live names stand for it. An open
    /// document's names give its file name as their source, as its tab
    /// shows it (see [`NamesIndex::defining`] for the way back).
    pub(crate) fn names(
        &mut self,
        schema: &SchemaIdentity,
        pointer: &str,
        except: Option<&str>,
        given: &[&[KeyNames]],
        parse: &mut ParseOf<'_>,
    ) -> Vec<KeyName> {
        // Each document's keys first (read again only where its text
        // changed), then each name once, told apart without a copy.
        let mut read: Vec<(String, Rc<[String]>)> = Vec::new();
        for id in &self.order {
            if Some(id.as_str()) == except {
                continue;
            }
            let Some(document) = self.documents.get_mut(id) else {
                continue;
            };
            if document.identity.as_ref() == Some(schema) {
                let keys = document.keys_at(id, pointer, parse);
                read.push((document.file_name.clone(), keys));
            }
        }
        let mut seen: HashSet<&str> = HashSet::new();
        let mut found = Vec::new();
        for (file_name, keys) in &read {
            for key in keys.iter() {
                if seen.insert(key) {
                    found.push(KeyName::new(key.as_str()).with_source(file_name.as_str()));
                }
            }
        }
        let mut host: Vec<KeyName> = Vec::new();
        for entries in given {
            for name in entries.names(schema, pointer) {
                let live = name
                    .source
                    .as_deref()
                    .is_some_and(|source| self.documents.contains_key(source));
                if !live && !seen.contains(name.name.as_str()) {
                    host.push(name);
                }
            }
        }
        let mut shown: HashSet<String> = HashSet::new();
        found.extend(
            host.into_iter()
                .filter(|name| shown.insert(name.name.clone())),
        );
        found
    }

    /// The open document defining `name` at `pointer` that a name's
    /// `source` stands for: the document with that id (a host's source),
    /// else one with that file name (an open document's live names) whose
    /// keys there, as last read, hold `name`, the first in tab order.
    /// `None` when no open document does.
    pub(crate) fn defining(&self, source: &str, pointer: &str, name: &str) -> Option<String> {
        if self.documents.contains_key(source) {
            return Some(source.to_string());
        }
        self.order
            .iter()
            .find(|id| {
                self.documents.get(*id).is_some_and(|document| {
                    document.file_name == source
                        && document
                            .keys
                            .iter()
                            .any(|(at, keys)| at == pointer && keys.iter().any(|key| key == name))
                })
            })
            .cloned()
    }
}

impl Indexed {
    /// Its keys at `pointer`: as read before, unless its text changed or
    /// the pointer is new; a text that does not parse keeps the last names.
    fn keys_at(&mut self, id: &str, pointer: &str, parse: &mut ParseOf<'_>) -> Rc<[String]> {
        let known = self.keys.iter().position(|(at, _)| at == pointer);
        if !self.changed
            && let Some(at) = known
        {
            return Rc::clone(&self.keys[at].1);
        }
        note(Work::KeyNamesRead);
        let parsed = parse(id, self.language);
        self.changed = false;
        if let Some(Ok(value)) = parsed.as_deref() {
            for (at, keys) in &mut self.keys {
                *keys = keys_at_pointer(value, at);
            }
            if known.is_none() {
                self.keys
                    .push((pointer.to_string(), keys_at_pointer(value, pointer)));
            }
        } else if known.is_none() {
            self.keys.push((pointer.to_string(), Rc::from([])));
        }
        let at = self
            .keys
            .iter()
            .position(|(at, _)| at == pointer)
            .expect("the pointer was just read");
        Rc::clone(&self.keys[at].1)
    }
}

/// The keys of the map at `pointer` of `value`, in order; none where no
/// map is there.
fn keys_at_pointer(value: &Value, pointer: &str) -> Rc<[String]> {
    keys_in(value, pointer).map_or_else(|| Rc::from([]), Rc::from)
}
