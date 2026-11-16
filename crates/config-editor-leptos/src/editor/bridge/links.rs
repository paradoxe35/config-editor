//! Provenance of an Edit-as view's items: each list item or map entry with
//! lines of its own carries a link to the host item it came from (a host
//! text and a pointer in it). The editor's own operations keep the links
//! as they edit (see `splices`): text before an edit shifts them, an edit
//! inside an item keeps its link, an edit covering an item removes it and
//! keeps it in the clipboard's record of the removed text (see
//! `clipboard`), and text inserted that is a record's text (a paste, a
//! drop, a line moved or copied) takes the record's links. A save reads
//! each item's source from them (`item_sources`): nothing is told by
//! comparing texts or values.

use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::super::work_count::{Work, note};
use std::sync::Arc;

mod clipboard;
mod items;
pub(crate) mod splices;
#[cfg(all(test, feature = "yaml"))]
mod tests;
mod told;
mod untouched;

pub(crate) use clipboard::Clipboard;
use items::items;
pub(crate) use splices::{Inserted, Piece, Splice, pieces};
pub(crate) use told::Told;
pub(crate) use untouched::Untouched;

/// Typing kept as splices before the links are read (see [`Links::Fresh`]).
const MOST_PENDING: usize = 64;

/// A host item: the host text (as a save wrote it, or as the host gave
/// it) and its pointer there.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) host: Arc<str>,
    pub(crate) pointer: String,
    /// Where the editor wrote the item in a view (built from the host
    /// text, or converted by a switch): what tells the scalars the user
    /// left as the editor wrote them (see `untouched`).
    pub(crate) shown: Option<Shown>,
}

/// A view text the editor wrote, and an item's pointer in it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    pub(crate) text: Arc<str>,
    pub(crate) pointer: String,
    /// The view's language (its label).
    pub(crate) language: &'static str,
    /// The pointers (in `text`) of the scalars written there as the
    /// editor first wrote them, when not all are: a switch converting
    /// unsaved edits writes the user's values too.
    pub(crate) kept: Option<Arc<std::collections::HashSet<String>>>,
}

/// An item's link: its text in the view (from the first character of its
/// first line that is not blank to the end of its last one's), and its
/// source.
#[derive(Clone, Debug)]
pub(crate) struct Link {
    start: usize,
    end: usize,
    source: Arc<Source>,
}

/// The links of a view's text.
#[derive(Clone, Debug)]
pub(crate) enum Links {
    /// Each item of `base` (the view's text when the host was `host`) is
    /// the host item at its own pointer; `pending` are the small edits
    /// (typing) made since, which no item's link can have taken: the items
    /// are read only once links are wanted.
    Fresh {
        host: Arc<str>,
        base: Arc<str>,
        pending: Arc<Vec<(usize, usize, usize)>>,
    },
    /// The items' links, by where they start (an outer item before the
    /// ones inside it).
    Kept(Arc<Vec<Link>>),
}

impl Default for Links {
    fn default() -> Self {
        Links::Kept(Arc::default())
    }
}

/// Two link states are the same state (not two that read alike).
impl PartialEq for Links {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Links::Fresh { base, pending, .. },
                Links::Fresh {
                    base: theirs,
                    pending: their_pending,
                    ..
                },
            ) => Arc::ptr_eq(base, theirs) && Arc::ptr_eq(pending, their_pending),
            (Links::Kept(links), Links::Kept(theirs)) => {
                Arc::ptr_eq(links, theirs) || links.is_empty() && theirs.is_empty()
            }
            _ => false,
        }
    }
}

impl Eq for Links {}

impl Links {
    /// The view's text `text`, shown for the host's text `host`.
    pub(crate) fn fresh(host: &str, text: &str) -> Self {
        Links::Fresh {
            host: Arc::from(host),
            base: Arc::from(text),
            pending: Arc::default(),
        }
    }

    /// Bytes the links hold, for the undo history's budget: the texts a
    /// `Fresh` state shares with `newer` (the state of the step after it)
    /// counted there only.
    pub(crate) fn bytes(&self, newer: Option<&Links>) -> usize {
        match self {
            Links::Fresh {
                host,
                base,
                pending,
            } => {
                let shared = matches!(newer, Some(Links::Fresh { base: theirs, .. }) if Arc::ptr_eq(base, theirs));
                pending.len() * 24 + if shared { 0 } else { host.len() + base.len() }
            }
            Links::Kept(links) => links.len() * std::mem::size_of::<Link>(),
        }
    }

    /// The links once the edit's `pieces` (sorted, apart, in the text
    /// before) are made to `before` (in `language`): each piece's insert
    /// takes the links its source gives (see [`Inserted`]), and the
    /// clipboard keeps those an edit removes.
    pub(crate) fn edited(
        &mut self,
        language: &dyn ConfigLanguage,
        before: &str,
        pieces: &[Piece],
        clipboard: &mut Clipboard,
    ) {
        if pieces.is_empty() {
            return;
        }
        // The links are of the text the edit is made on: every write of
        // the text on screen is an edit they follow.
        debug_assert!(
            self.fits(before.len()),
            "links of another text than the one edited"
        );
        if !self.fits(before.len()) {
            *self = Links::default();
        }
        // Typing changes no item's link: kept until the links are read.
        let typed = pieces.iter().all(|piece| {
            piece.removed < 2
                && matches!(
                    piece.source,
                    Inserted::Typed | Inserted::Generated | Inserted::External
                )
        });
        if let Links::Fresh { pending, .. } = self
            && typed
            && pending.len() < MOST_PENDING
        {
            let pending = Arc::make_mut(pending);
            pending.extend(
                pieces
                    .iter()
                    .map(|piece| (piece.at, piece.removed, piece.inserted.len())),
            );
            return;
        }
        let links = self.read(language);
        let inserted: Vec<Vec<Link>> = pieces
            .iter()
            .map(|piece| match &piece.source {
                Inserted::Moved(moved) => moved
                    .iter()
                    .flat_map(|moved| {
                        within(&links, moved.from, moved.from + moved.len)
                            .into_iter()
                            .chain(heads(&links, before, moved.from, moved.from + moved.len))
                            .map(|link| Link {
                                start: link.start + moved.to,
                                end: link.end + moved.to,
                                source: link.source,
                            })
                    })
                    .collect(),
                Inserted::Pasted => clipboard.links_of(&piece.inserted),
                Inserted::Reindented => clipboard::reindented(
                    &before[piece.at..piece.end()],
                    within(&links, piece.at, piece.end()),
                    &piece.inserted,
                ),
                Inserted::Typed | Inserted::Generated | Inserted::External => Vec::new(),
            })
            .collect();
        let (mut kept, removed) = shifted(&links, pieces, Some(&inserted));
        for (piece, links) in pieces.iter().zip(removed) {
            if !links.is_empty() {
                clipboard.keep(&before[piece.at..piece.end()], links);
            }
        }
        kept.sort_by(|left, right| left.start.cmp(&right.start).then(right.end.cmp(&left.end)));
        // Each link a range of the text after the edit.
        let length = pieces.iter().fold(before.len(), |length, piece| {
            length + piece.inserted.len() - piece.removed
        });
        debug_assert!(
            kept.iter()
                .all(|link| link.start < link.end && link.end <= length),
            "a link outside the text of {length} bytes after {pieces:?}"
        );
        kept.retain(|link| link.start < link.end && link.end <= length);
        *self = Links::Kept(Arc::new(kept));
    }

    /// Whether the links can be of a text of `length` bytes.
    fn fits(&self, length: usize) -> bool {
        match self {
            Links::Fresh { base, pending, .. } => {
                pending
                    .iter()
                    .try_fold(base.len(), |at, &(_, removed, inserted)| {
                        (at + inserted).checked_sub(removed)
                    })
                    == Some(length)
            }
            Links::Kept(links) => links.iter().all(|link| link.end <= length),
        }
    }

    /// The links of `text[range]` (bytes), from its start: what a copy of
    /// it takes.
    pub(crate) fn copied(
        &mut self,
        (language, text): (&dyn ConfigLanguage, &str),
        start: usize,
        end: usize,
    ) -> Vec<(usize, usize, Arc<Source>)> {
        if !self.fits(text.len()) {
            return Vec::new();
        }
        within(&self.read(language), start, end)
            .into_iter()
            .map(|link| (link.start, link.end, link.source))
            .collect()
    }

    /// The links, read from the items now if not yet (`Fresh`).
    fn read(&mut self, language: &dyn ConfigLanguage) -> Arc<Vec<Link>> {
        let Links::Fresh {
            host,
            base,
            pending,
        } = self
        else {
            let Links::Kept(links) = self else {
                unreachable!()
            };
            return links.clone();
        };
        let parsed = parse_cache::parsed(language, base);
        let mut links: Vec<Link> = match parsed.as_ref() {
            Ok(value) => items(language, base, value)
                .into_iter()
                .filter_map(|item| {
                    let (start, end) = content(base, item.lines.start, item.lines.end)?;
                    Some(Link {
                        start,
                        end,
                        source: Arc::new(Source {
                            host: host.clone(),
                            shown: Some(Shown {
                                text: base.clone(),
                                pointer: item.pointer.clone(),
                                language: language.info().label,
                                kept: None,
                            }),
                            pointer: item.pointer,
                        }),
                    })
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        // By where they start, an outer item before the ones inside it
        // (the items read in the value's order, which need not be the
        // text's: `within` and the edits find links by start).
        links.sort_by(|left, right| left.start.cmp(&right.start).then(right.end.cmp(&left.end)));
        for &(at, removed, inserted) in pending.iter() {
            let typed = Piece {
                at,
                removed,
                inserted: " ".repeat(inserted),
                source: Inserted::Typed,
            };
            links = shifted(&links, std::slice::from_ref(&typed), None).0;
        }
        let links = Arc::new(links);
        *self = Links::Kept(links.clone());
        links
    }
}

/// The text of the lines `start..end` of `text` from its first character
/// that is not blank to the end of its last one; `None` when blank.
fn content(text: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let lines = &text[start..end];
    let trimmed = lines.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let from = start + lines.len() - trimmed.len();
    Some((from, from + trimmed.trim_end().len()))
}

/// The links wholly inside `start..end`, from `start`.
fn within(links: &[Link], start: usize, end: usize) -> Vec<Link> {
    let first = links.partition_point(|link| link.start < start);
    links[first..]
        .iter()
        .take_while(|link| link.start < end)
        .filter(|link| link.end <= end)
        .map(|link| Link {
            start: link.start - start,
            end: link.end - start,
            source: link.source.clone(),
        })
        .collect()
}

/// The links starting inside `start..end` of `text` that run past it,
/// from `start`, cut at its last character that is not blank: the head
/// of an item (its key's line) moved away from the rest of its lines
/// stays that item's.
fn heads(links: &[Link], text: &str, start: usize, end: usize) -> Vec<Link> {
    let last = start + text[start..end].trim_end().len();
    let first = links.partition_point(|link| link.start < start);
    links[first..]
        .iter()
        .take_while(|link| link.start < end)
        .filter(|link| link.end > end && link.start < last)
        .map(|link| Link {
            start: link.start - start,
            end: last - start,
            source: link.source.clone(),
        })
        .collect()
}

/// `links` (sorted by start) after `splices` (sorted, apart, in the text
/// before), with the links each insert takes (`inserted`, from its
/// start), and for each splice the links its removal covers, from its
/// start. One pass over both: a splice is looked at by the links it is
/// inside of only.
fn shifted(
    links: &[Link],
    splices: &[Piece],
    inserted: Option<&[Vec<Link>]>,
) -> (Vec<Link>, Vec<Vec<Link>>) {
    // How far each splice's start moves: the growth of those before it.
    let mut moved = Vec::with_capacity(splices.len() + 1);
    let mut growth = 0isize;
    for splice in splices {
        moved.push(growth);
        growth += splice.inserted.len() as isize - splice.removed as isize;
    }
    moved.push(growth);
    let at = |offset: usize, by: isize| (offset as isize + by) as usize;
    let mut out = Vec::with_capacity(links.len());
    let mut removed = vec![Vec::new(); splices.len()];
    // The first splice not wholly before the link's start.
    let mut first = 0;
    for link in links {
        let (s, e) = (link.start, link.end);
        while first < splices.len() && splices[first].at + splices[first].removed <= s {
            first += 1;
        }
        let start = at(s, moved[first]);
        let mut end = e as isize + moved[first];
        let mut alive = true;
        for (index, splice) in splices.iter().enumerate().skip(first) {
            note(Work::LinkStep);
            let (a, b) = (splice.at, splice.at + splice.removed);
            if a >= e {
                break;
            }
            if a <= s && b >= e {
                removed[index].push(Link {
                    start: s - a,
                    end: e - a,
                    source: link.source.clone(),
                });
                alive = false;
                break;
            }
            // Its first character removed (its key, its header, its
            // opening tag): no longer the item it was. A removal from its
            // start keeps its head with the removed text (a header cut
            // and pasted back heads the item again).
            if a < s || a == s && b > s {
                if a == s && b - a > 1 {
                    removed[index].push(Link {
                        start: 0,
                        end: b - a,
                        source: link.source.clone(),
                    });
                }
                alive = false;
                break;
            }
            if b > e {
                end = (at(a, moved[index]) + splice.inserted.len()) as isize;
                break;
            }
            end += splice.inserted.len() as isize - splice.removed as isize;
        }
        if alive && end as usize > start {
            out.push(Link {
                start,
                end: end as usize,
                source: link.source.clone(),
            });
        }
    }
    if let Some(inserted) = inserted {
        for (index, (splice, links)) in splices.iter().zip(inserted).enumerate() {
            let place = at(splice.at, moved[index]);
            out.extend(links.iter().map(|link| Link {
                start: place + link.start,
                end: place + link.end,
                source: link.source.clone(),
            }));
        }
    }
    (out, removed)
}
