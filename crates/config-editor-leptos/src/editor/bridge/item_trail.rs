//! The edits made to an Edit-as view since its last save, which tell the
//! save which item of the host each item of the view is (see
//! `item_sources`): an item whose lines the edits kept is the item it was
//! (the first of two equal items deleted leaves the second, QA `E32-3`),
//! and an item whose lines were removed and pasted back, in this save or
//! an earlier one of the session, is the item moved (QA `E32-2`). Values
//! tell the rest.

use super::super::item_sources::ItemSource;
use super::super::language::ConfigLanguage;
use std::collections::HashMap;

mod items;
mod splice;
#[cfg(all(test, feature = "yaml", feature = "json"))]
mod tests;

use items::{Item, items};
pub(crate) use splice::Splice;

/// Edits kept between two saves: past this many the trail is dropped, and
/// values tell the items until the next save.
const MOST_SPLICES: usize = 4096;

/// An item of the session's first view no later view holds: what a paste
/// may bring back.
#[derive(Clone, Debug, PartialEq)]
struct Removed {
    /// Its pointer in the session's first view.
    origin: String,
    /// Its lines.
    lines: String,
}

/// The edits made to a view's text since it was last saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Trail {
    /// The view's text at its last save (or the session's start); `None`
    /// while the edits since are not known.
    synced: Option<String>,
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
        let before = items(language, synced)?;
        let after = items(language, text)?;
        let origin_of = |pointer: &str| match &self.origin_of {
            None => Some(pointer.to_string()),
            Some(known) => known.get(pointer).cloned(),
        };
        let mut followed: Vec<Followed> = before
            .iter()
            .map(|Item { pointer, lines }| Followed {
                lines: lines.clone(),
                source: Source::Synced(pointer.clone()),
                alive: true,
            })
            .collect();
        // Lines removed, which a later insert may bring back.
        let mut cut: Vec<(String, Source)> = self
            .removed
            .iter()
            .map(|removed| {
                (
                    removed.lines.clone(),
                    Source::Origin(removed.origin.clone()),
                )
            })
            .collect();
        for splice in &self.splices {
            follow(&mut followed, &mut cut, splice);
        }
        let mut items = HashMap::new();
        let mut saved_origin = HashMap::new();
        for item in &after {
            let Some(found) = followed
                .iter()
                .find(|found| found.alive && found.lines.start == item.lines.start)
            else {
                continue;
            };
            let (source, origin) = match &found.source {
                Source::Synced(pointer) => {
                    (ItemSource::Previous(pointer.clone()), origin_of(pointer))
                }
                Source::Origin(pointer) => {
                    (ItemSource::Origin(pointer.clone()), Some(pointer.clone()))
                }
            };
            items.insert(item.pointer.clone(), source);
            if let Some(origin) = origin {
                saved_origin.insert(item.pointer.clone(), origin);
            }
        }
        // Still out of the view: the first view's items removed whole, now
        // or before.
        let removed = cut
            .into_iter()
            .filter_map(|(lines, source)| {
                let origin = match source {
                    Source::Synced(pointer) => origin_of(&pointer)?,
                    Source::Origin(pointer) => pointer,
                };
                (!saved_origin.values().any(|kept| *kept == origin))
                    .then_some(Removed { origin, lines })
            })
            .collect();
        Some(Trace {
            items,
            saved: Trail {
                synced: Some(text.to_string()),
                splices: Vec::new(),
                origin_of: Some(saved_origin),
                removed,
            },
        })
    }
}

/// `followed` (and the lines `cut` so far) through `splice`: an item
/// whose first line it removes is removed, kept in `cut` when it removes
/// all its lines; an item whose lines it inserts again, from `cut`, is
/// that item, where it is inserted.
fn follow(followed: &mut Vec<Followed>, cut: &mut Vec<(String, Source)>, splice: &Splice) {
    let (at, end) = (splice.at, splice.at + splice.removed.len());
    let grown = splice.inserted.len() as isize - splice.removed.len() as isize;
    let shift = |offset: usize| (offset as isize + grown) as usize;
    for item in followed.iter_mut().filter(|item| item.alive) {
        let lines = item.lines.clone();
        if at <= lines.start && lines.start < end {
            item.alive = false;
            if lines.end <= end {
                cut.push((
                    splice.removed[lines.start - at..lines.end - at].to_string(),
                    item.source.clone(),
                ));
            }
        } else if lines.start >= end {
            item.lines = shift(lines.start)..shift(lines.end);
        } else if lines.end > end {
            item.lines.end = shift(lines.end);
        } else if lines.end > at {
            item.lines.end = at + splice.inserted.len();
        }
    }
    // Lines inserted again: each cut item at the first place its lines
    // are, not taken by another.
    let mut taken: Vec<std::ops::Range<usize>> = Vec::new();
    let mut index = 0;
    while index < cut.len() {
        let lines = &cut[index].0;
        let place = splice
            .inserted
            .match_indices(lines.as_str())
            .map(|(offset, _)| offset)
            .find(|offset| {
                let range = *offset..offset + lines.len();
                let line_start = *offset == 0 || splice.inserted[..*offset].ends_with('\n');
                line_start
                    && !taken
                        .iter()
                        .any(|known| known.start < range.end && range.start < known.end)
            });
        match place {
            Some(offset) => {
                let (lines, source) = cut.remove(index);
                let range = at + offset..at + offset + lines.len();
                taken.push(offset..offset + lines.len());
                followed.push(Followed {
                    lines: range,
                    source,
                    alive: true,
                });
            }
            None => index += 1,
        }
    }
}
