//! The edits made to an Edit-as view since its last save, which tell the
//! save which item of the host each item of the view is (see
//! `item_sources`): an item whose lines the edits kept is the item it was
//! (the first of two equal items deleted leaves the second, QA `E32-3`),
//! and an item whose lines were removed and pasted back, in this save or
//! an earlier one of the session, is the item moved (QA `E32-2`), and so
//! is an item pasted where its lines read otherwise (another indent, a
//! JSON item's comma) or after a switch, by its data (QA `E34-1`). A map
//! entry is followed the same way: one cut and pasted back holding the
//! same data under the same key is the entry moved (QA `E35-1`). Values
//! tell the rest.

use super::super::item_sources::ItemSource;
use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::super::work_count::{Work, note};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

mod by_data;
mod copies;
mod follow;
mod items;
mod pasted;
mod places;
mod splice;
#[cfg(all(test, feature = "yaml", feature = "json"))]
mod tests;

use follow::follow;
use items::{Item, items};
pub(crate) use splice::Splice;

/// Edits kept between two saves: past this many the trail is dropped, and
/// values tell the items until the next save.
const MOST_SPLICES: usize = 4096;

/// Saves in a row a trail is followed through untraced (see
/// [`Trail::wants_trace`]): the next one traces it, so its edits stay few
/// and its start's host text known.
const MOST_UNTRACED: usize = 32;

/// An item of the session's first view no later view holds: what a paste
/// may bring back.
#[derive(Clone, Debug, PartialEq)]
struct Removed {
    /// Its pointer in the session's first view.
    origin: String,
    /// Its lines.
    lines: String,
    /// Its data, as a view holds it: an item pasted in any view, its lines
    /// written otherwise, is it when it holds the same.
    value: Value,
    /// A map entry's key; `None` for a list item.
    key: Option<String>,
}

/// Lines removed, which a later insert may bring back: what they held,
/// and its key when it is a map entry.
struct Cut {
    lines: String,
    source: Source,
    key: Option<String>,
    /// Where they were in the removed text of the edit cutting them, while
    /// it is followed.
    within: Option<std::ops::Range<usize>>,
}

/// The edits made to a view's text since it was last saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Trail {
    /// The view's text at its last save (or the session's start); `None`
    /// while the edits since are not known.
    synced: Option<String>,
    /// The view's text at its last save, when saves since `synced` went
    /// untraced (see [`Trail::wants_trace`]), and how many did.
    reached: Option<String>,
    untraced: usize,
    splices: Vec<Splice>,
    /// Each item of `synced` that is an item of the session's first view,
    /// by its pointer there; `None` at the session's start, where each is.
    origin_of: Option<HashMap<String, String>>,
    removed: Vec<Removed>,
}

/// What a trail tells of the view's text: each item's source, and the
/// trail once the text is saved.
pub(crate) struct Trace {
    pub(crate) items: HashMap<String, ItemSource>,
    pub(crate) saved: Trail,
}

/// An item followed through the edits: its lines, and what it is.
struct Followed {
    lines: std::ops::Range<usize>,
    source: Source,
    key: Option<String>,
    alive: bool,
}

#[derive(Clone)]
enum Source {
    /// The item at this pointer of the last saved view.
    Synced(String),
    /// The item at this pointer of the session's first view.
    Origin(String),
}

impl Trail {
    /// A session beginning on the view's text `text`.
    pub(crate) fn starting(text: &str) -> Self {
        Trail {
            synced: Some(text.to_string()),
            ..Trail::default()
        }
    }

    /// The view's text `text` saved with no edits known before it: its
    /// items are no item of the session's first view the trail knows.
    pub(crate) fn unknown_since(text: &str) -> Self {
        Trail {
            synced: Some(text.to_string()),
            origin_of: Some(HashMap::new()),
            ..Trail::default()
        }
    }

    /// The trail on `text`, the text a switch (to another view, or the
    /// host's language and back) or a save put on screen for the host's
    /// text the trail last followed: no edits since, and what it knows of
    /// the session's items kept, the items cut and not pasted back with
    /// them (QA `E34-1`: a cut item pasted after a switch lost its markup).
    pub(crate) fn restart(&mut self, text: &str) {
        self.synced = Some(text.to_string());
        self.reached = None;
        self.untraced = 0;
        self.splices = Vec::new();
    }

    /// The edits made to the view no longer known (a text not saved
    /// replaced it): no item of it is known to be one of the session's
    /// first view, and the items cut from that view are still out of it.
    pub(crate) fn forget_edits(&mut self) {
        self.synced = None;
        self.reached = None;
        self.untraced = 0;
        self.splices = Vec::new();
        self.origin_of = Some(HashMap::new());
    }

    /// The view's text `text` saved without this trail's edits telling
    /// its items (see [`Self::unknown_since`]): the items cut from the
    /// session's first view are still out of it.
    pub(crate) fn unknown_from(&self, text: &str) -> Self {
        Trail {
            removed: self.removed.clone(),
            ..Trail::unknown_since(text)
        }
    }

    /// The view's text `text` saved as the session's first host text, by
    /// its own: each of its items is the item of the session's first view
    /// at its pointer, and the items cut from it before are still out.
    pub(crate) fn at_origin(&self, text: &str) -> Self {
        Trail {
            removed: self.removed.clone(),
            ..Trail::starting(text)
        }
    }

    /// Whether the trail follows the edits since `text`, the view's text at
    /// its last save: a trail begun on another text (one a view switch
    /// replaced unsaved edits with) does not tell the items of a save over
    /// the host `text` was saved as.
    pub(crate) fn begins_on(&self, text: &str) -> bool {
        self.reached.as_deref().or(self.synced.as_deref()) == Some(text)
    }

    /// The view's text the trail follows the edits from, a save's (its
    /// items are the items of the host text saved then).
    pub(crate) fn start(&self) -> Option<&str> {
        self.synced.as_deref()
    }

    /// Whether a save must read the items from the edits: when an item may
    /// have been moved, removed or copied (a line removed, an item cut and
    /// not pasted back yet, lines pasted), or after many saves untraced. Otherwise (values
    /// typed, lines added) the save is told nothing, and the trail goes on
    /// from where it began ([`Self::untraced_to`]): reading every item of
    /// the view at every typing pause made a large document's save a
    /// third slower.
    pub(crate) fn wants_trace(&self) -> bool {
        !self.removed.is_empty()
            || self.untraced >= MOST_UNTRACED
            || self.splices.iter().any(pasted::pastes)
            || self
                .splices
                .iter()
                .any(|splice| splice.removed.contains('\n'))
    }

    /// The trail once the view's text `text` is saved untraced: its edits
    /// still followed from its start.
    pub(crate) fn untraced_to(&self, text: &str) -> Self {
        Trail {
            reached: Some(text.to_string()),
            untraced: self.untraced + 1,
            ..self.clone()
        }
    }

    /// The edit `before` → `after` (the caret left at UTF-16 `caret`).
    pub(crate) fn note(&mut self, before: &str, after: &str, caret: usize) {
        if self.synced.is_none() {
            return;
        }
        match Splice::of(before, after, caret) {
            Some(splice) if self.splices.len() < MOST_SPLICES => self.splices.push(splice),
            _ => {
                self.synced = None;
                self.splices = Vec::new();
            }
        }
    }

    /// What the edits tell of `text`, the view's text in `language` now;
    /// `None` when they are not known or do not lead to it.
    pub(crate) fn trace(&self, language: &dyn ConfigLanguage, text: &str) -> Option<Trace> {
        let synced = self.synced.as_deref()?;
        let grown: isize = self
            .splices
            .iter()
            .map(|splice| splice.inserted.len() as isize - splice.removed.len() as isize)
            .sum();
        if synced.len() as isize + grown != text.len() as isize {
            return None;
        }
        let (synced_parsed, parsed) = (
            parse_cache::parsed(language, synced),
            parse_cache::parsed(language, text),
        );
        let (synced_value, now) = (
            synced_parsed.as_ref().as_ref().ok()?,
            parsed.as_ref().as_ref().ok()?,
        );
        let before = items(language, synced, synced_value);
        let after = items(language, text, now);
        let origin_of = |pointer: &str| match &self.origin_of {
            None => Some(pointer.to_string()),
            Some(known) => known.get(pointer).cloned(),
        };
        let mut followed: Vec<Followed> = before
            .iter()
            .map(
                |Item {
                     pointer,
                     key,
                     lines,
                 }| Followed {
                    lines: lines.clone(),
                    source: Source::Synced(pointer.clone()),
                    key: key.clone(),
                    alive: true,
                },
            )
            .collect();
        // Lines removed, which a later insert may bring back.
        let mut cut: Vec<Cut> = self
            .removed
            .iter()
            .map(|removed| Cut {
                lines: removed.lines.clone(),
                source: Source::Origin(removed.origin.clone()),
                key: removed.key.clone(),
                within: None,
            })
            .collect();
        for splice in &self.splices {
            follow(&mut followed, &mut cut, splice);
        }
        let mut items = HashMap::new();
        let mut saved_origin = HashMap::new();
        let mut record = |pointer: &str, source: &Source| {
            let (source, origin) = match source {
                Source::Synced(from) => (ItemSource::Previous(from.clone()), origin_of(from)),
                Source::Origin(from) => (ItemSource::Origin(from.clone()), Some(from.clone())),
            };
            items.insert(pointer.to_string(), source);
            if let Some(origin) = origin {
                saved_origin.insert(pointer.to_string(), origin);
            }
        };
        // Each item of the view at the start of the lines of one followed
        // (the first followed there).
        let mut at_start: HashMap<usize, &Source> = HashMap::new();
        for found in followed.iter().filter(|found| found.alive) {
            at_start.entry(found.lines.start).or_insert(&found.source);
        }
        let mut unmatched = Vec::new();
        for item in &after {
            note(Work::TrailStep);
            match at_start.get(&item.lines.start) {
                Some(source) => record(&item.pointer, source),
                None => unmatched.push(item),
            }
        }
        // The data of the items cut: an item of the last saved view's, or
        // one cut from the session's first view before.
        let cut_before: HashMap<&str, &Value> = self
            .removed
            .iter()
            .map(|removed| (removed.origin.as_str(), &removed.value))
            .collect();
        let value_of = |source: &Source| match source {
            Source::Synced(pointer) => synced_value.pointer(pointer),
            Source::Origin(pointer) => cut_before.get(pointer.as_str()).copied(),
        };
        // Pasted where its lines read otherwise, or after a switch: the
        // item cut holding the same data (a map entry: under the same key).
        let mut cut: Vec<Option<Cut>> = cut.into_iter().map(Some).collect();
        let mut pasted_back = Vec::new();
        if !cut.is_empty() && !unmatched.is_empty() {
            let pasted: Vec<(&Item, &Value)> = unmatched
                .iter()
                .filter_map(|item| Some((*item, now.pointer(&item.pointer)?)))
                .collect();
            let held: Vec<(Option<&str>, Option<&Value>)> = cut
                .iter()
                .flatten()
                .map(|cut| (cut.key.as_deref(), value_of(&cut.source)))
                .collect();
            let wanted: Vec<(Option<&str>, &Value)> = pasted
                .iter()
                .map(|(item, value)| (item.key.as_deref(), *value))
                .collect();
            let mut told_items = HashSet::new();
            for ((item, _), told) in pasted.iter().zip(by_data::told(&held, &wanted)) {
                if let Some(cut) = told.and_then(|at| cut[at].take()) {
                    record(&item.pointer, &cut.source);
                    told_items.insert(item.pointer.as_str());
                    pasted_back.push(cut);
                }
            }
            unmatched.retain(|item| !told_items.contains(item.pointer.as_str()));
        }
        // Copied, or pasted again: the source of the item whose data it
        // holds (see `copies`).
        let pasted = if unmatched.is_empty() {
            Vec::new()
        } else {
            pasted::pasted(&self.splices)
        };
        if !pasted.is_empty() {
            let known: Vec<(&str, &Source)> = followed
                .iter()
                .filter(|item| item.alive)
                .map(|item| (item.key.as_deref(), &item.source))
                .chain(
                    cut.iter()
                        .flatten()
                        .chain(&pasted_back)
                        .map(|cut| (cut.key.as_deref(), &cut.source)),
                )
                .filter_map(|(key, source)| Some((key?, source)))
                .collect();
            for (pointer, source) in copies::of(&unmatched, &pasted, &known, now, value_of) {
                record(pointer, &source);
            }
        }
        // Still out of the view: the first view's items removed whole, now
        // or before.
        let kept: HashSet<&String> = saved_origin.values().collect();
        let removed = cut
            .into_iter()
            .flatten()
            .filter_map(
                |Cut {
                     lines, source, key, ..
                 }| {
                    let value = value_of(&source)?.clone();
                    let origin = match source {
                        Source::Synced(pointer) => origin_of(&pointer)?,
                        Source::Origin(pointer) => pointer,
                    };
                    (!kept.contains(&origin)).then_some(Removed {
                        origin,
                        lines,
                        value,
                        key,
                    })
                },
            )
            .collect();
        Some(Trace {
            items,
            saved: Trail {
                synced: Some(text.to_string()),
                origin_of: Some(saved_origin),
                removed,
                ..Trail::default()
            },
        })
    }
}
