//! Provenance of an Edit-as view's items: each list item or map entry with
//! lines of its own carries a link to the host item it came from (a host
//! text and a pointer in it). The editor's own operations keep the links
//! as they edit (see `splices`): text before an edit shifts them, an edit
//! inside an item keeps its link, an edit covering an item removes it and
//! keeps it in the clipboard's record of the removed text (see
//! `clipboard`), and text inserted that is a record's text (a paste, a
//! drop, a line moved or copied) takes the record's links. A save reads
//! each item's source from them (`item_sources`): nothing is told by
//! comparing texts or values. After a save each item is the new host's,
//! and a link on a line with no item (an item's text pasted inside a
//! scalar or a comment) keeps its source (see [`Links::saved`]).

use super::super::Language;
use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::super::schema::SchemaBundle;
use super::super::text::{TextRange, TextSize, slice};
use super::super::work_count::{Work, note};
use std::cmp::Ordering;
use std::sync::Arc;

mod clipboard;
mod holding;
mod items;
pub(crate) mod splices;
#[cfg(all(test, feature = "yaml"))]
mod tests;
mod told;
mod untouched;

pub(crate) use clipboard::Clipboard;
pub(crate) use holding::{Holding, HostPaths};
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
    /// How the host holds the item, recorded where the view was written
    /// from it (see `holding`): a list of one the view writes as its item
    /// holds what the view writes inside it under its `/0`.
    pub(crate) holding: Holding,
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
    range: TextRange,
    source: Arc<Source>,
}

/// Links by where they start, an outer one before those inside it.
fn outer_first(left: &Link, right: &Link) -> Ordering {
    left.range
        .start()
        .cmp(&right.range.start())
        .then(right.range.end().cmp(&left.range.end()))
}

/// Typing kept before the links are read: the range it replaced (in the
/// text the typing before it left) and how many bytes it wrote.
type Pending = (TextRange, TextSize);

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
        pending: Arc<Vec<Pending>>,
        /// How `host` reads, to name each item by its host path when the
        /// items are read; `None` when the view is in the host's language.
        reading: Option<HostReading>,
        /// Links of `base` on lines where it has no item (an item's text
        /// pasted inside a scalar or a comment), kept from before the save
        /// that made `host`: that text is still that item's when it is cut
        /// and pasted as an item again (see [`Links::saved`]).
        loose: Arc<Vec<Link>>,
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
                Links::Fresh {
                    base,
                    pending,
                    loose,
                    ..
                },
                Links::Fresh {
                    base: theirs,
                    pending: their_pending,
                    loose: their_loose,
                    ..
                },
            ) => {
                Arc::ptr_eq(base, theirs)
                    && Arc::ptr_eq(pending, their_pending)
                    && Arc::ptr_eq(loose, their_loose)
            }
            (Links::Kept(links), Links::Kept(theirs)) => {
                Arc::ptr_eq(links, theirs) || links.is_empty() && theirs.is_empty()
            }
            _ => false,
        }
    }
}

impl Eq for Links {}

impl Links {
    /// The view's text `text`, shown for the host's text `host` (read as
    /// `reading` tells; `None` in the host's own language).
    pub(crate) fn fresh(host: &str, text: &str, reading: Option<HostReading>) -> Self {
        Links::Fresh {
            host: Arc::from(host),
            base: Arc::from(text),
            pending: Arc::default(),
            reading,
            loose: Arc::default(),
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
                loose,
                ..
            } => {
                let shared = matches!(newer, Some(Links::Fresh { base: theirs, .. }) if Arc::ptr_eq(base, theirs));
                pending.len() * std::mem::size_of::<Pending>()
                    + loose.len() * std::mem::size_of::<Link>()
                    + if shared { 0 } else { host.len() + base.len() }
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
        let length = TextSize::of(before);
        debug_assert!(
            self.fits(length),
            "links of another text than the one edited"
        );
        if !self.fits(length) {
            *self = Links::default();
        }
        // Typing changes no item's link: kept until the links are read.
        // A line break removed is not kept so: it can join an item onto
        // another's line, where the items of the text no longer tell it
        // apart and only its link does.
        let typed = pieces.iter().all(|piece| {
            piece.range.len() < TextSize::from(2)
                && matches!(
                    piece.source,
                    Inserted::Typed | Inserted::Generated | Inserted::External
                )
                && slice::slice(before, piece.range).is_none_or(|removed| !removed.contains('\n'))
        });
        if let Links::Fresh { pending, .. } = self
            && typed
            && pending.len() + pieces.len() <= MOST_PENDING
        {
            // Kept one after another: each piece at its place in the text
            // the ones before it left (the pieces name the text before the
            // whole edit).
            let pending = Arc::make_mut(pending);
            let mut shift = Shift::default();
            pending.extend(pieces.iter().map(|piece| {
                let at = shift.of(piece.range.start());
                shift.past(piece);
                (TextRange::at(at, piece.range.len()), piece.inserted_len())
            }));
            return;
        }
        let links = self.read(language);
        let inserted: Vec<Vec<Link>> = pieces
            .iter()
            .map(|piece| match &piece.source {
                Inserted::Moved(moved) => moved
                    .iter()
                    .flat_map(|moved| {
                        let range = TextRange::at(moved.from, moved.len);
                        within(&links, range)
                            .into_iter()
                            .chain(heads(&links, before, range))
                            .map(|link| Link {
                                range: link.range + moved.to,
                                source: link.source,
                            })
                    })
                    .collect(),
                Inserted::Pasted => clipboard.links_of(&piece.inserted),
                Inserted::Reindented => clipboard::reindented(
                    slice::slice(before, piece.range).unwrap_or_default(),
                    within(&links, piece.range),
                    &piece.inserted,
                ),
                Inserted::Typed | Inserted::Generated | Inserted::External => Vec::new(),
            })
            .collect();
        let (mut kept, removed) = shifted(&links, pieces, Some(&inserted));
        for (piece, links) in pieces.iter().zip(removed) {
            if !links.is_empty()
                && let Some(removed) = slice::slice(before, piece.range)
            {
                clipboard.keep(removed, links);
            }
        }
        kept.sort_by(outer_first);
        // Each link a range of the text after the edit.
        let length = pieces.iter().fold(length, |length, piece| {
            length + piece.inserted_len() - piece.range.len()
        });
        let inside = |link: &Link| !link.range.is_empty() && link.range.end() <= length;
        debug_assert!(
            kept.iter().all(inside),
            "a link outside the text of {length:?} bytes after {pieces:?}"
        );
        kept.retain(inside);
        *self = Links::Kept(Arc::new(kept));
    }

    /// Whether the links can be of a text of `length` bytes.
    fn fits(&self, length: TextSize) -> bool {
        match self {
            Links::Fresh { base, pending, .. } => {
                pending
                    .iter()
                    .try_fold(TextSize::of(&**base), |at, &(removed, inserted)| {
                        (at + inserted).checked_sub(removed.len())
                    })
                    == Some(length)
            }
            Links::Kept(links) => links.iter().all(|link| link.range.end() <= length),
        }
    }

    /// The links of `range` of `text`, from its start: what a copy of it
    /// takes.
    pub(crate) fn copied(
        &mut self,
        (language, text): (&dyn ConfigLanguage, &str),
        range: TextRange,
    ) -> Vec<(TextRange, Arc<Source>)> {
        if !self.fits(TextSize::of(text)) {
            return Vec::new();
        }
        within(&self.read(language), range)
            .into_iter()
            .map(|link| (link.range, link.source))
            .collect()
    }

    /// The links, read from the items now if not yet (`Fresh`).
    fn read(&mut self, language: &dyn ConfigLanguage) -> Arc<Vec<Link>> {
        let (host, base, pending, reading, loose) = match self {
            Links::Kept(links) => return links.clone(),
            Links::Fresh {
                host,
                base,
                pending,
                reading,
                loose,
            } => (host, base, pending, reading, loose),
        };
        let parsed = parse_cache::parsed(language, base);
        let mut links: Vec<Link> = match parsed.as_ref() {
            Ok(value) => {
                let paths = reading
                    .as_ref()
                    .and_then(|reading| reading.paths(host, value))
                    .unwrap_or_default();
                items(language, base, value)
                    .into_iter()
                    .filter_map(|item| {
                        let range = content(base, item.lines)?;
                        let (pointer, holding) = paths.item(&item.pointer);
                        Some(Link {
                            range,
                            source: Arc::new(Source {
                                host: host.clone(),
                                shown: Some(Shown {
                                    text: base.clone(),
                                    pointer: item.pointer,
                                    language: language.info().label,
                                    kept: None,
                                }),
                                pointer,
                                holding,
                            }),
                        })
                    })
                    .collect()
            }
            Err(_) => Vec::new(),
        };
        links.extend(loose.iter().cloned());
        // By where they start, an outer item before the ones inside it
        // (the items read in the value's order, which need not be the
        // text's: `within` and the edits find links by start).
        links.sort_by(outer_first);
        for &(range, inserted) in pending.iter() {
            let typed = Piece {
                range,
                inserted: " ".repeat(usize::from(inserted)),
                source: Inserted::Typed,
            };
            links = shifted(&links, std::slice::from_ref(&typed), None).0;
        }
        let links = Arc::new(links);
        *self = Links::Kept(links.clone());
        links
    }
}

/// How a host text reads (its language and schema): what names a view
/// item by its host path, where the host writes a list of one otherwise
/// than the view does.
#[derive(Clone)]
pub(crate) struct HostReading {
    pub(crate) language: Language,
    pub(crate) schema: Option<Arc<SchemaBundle>>,
}

impl std::fmt::Debug for HostReading {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.language.behavior().info().label)
    }
}

impl HostReading {
    /// `host` read (after a byte order mark), `None` when it does not read.
    fn read(&self, host: &str) -> Option<serde_json::Value> {
        let (host, _) = super::super::byte_order_mark::split(host);
        let schema = self.schema.as_deref().map(|bundle| &bundle.schema);
        super::super::codec::read(self.language.behavior(), host, schema).ok()
    }

    /// Each node of `view` (a view's value, written from `host`) paired
    /// with the host's (see [`HostPaths`]); `None` when `host` does not
    /// read.
    pub(crate) fn paths(&self, host: &str, view: &serde_json::Value) -> Option<HostPaths> {
        self.read(host).map(|host| HostPaths::between(&host, view))
    }
}

/// The text of the lines `lines` (bytes) of `text` from its first
/// character that is not blank to the end of its last one; `None` when
/// blank.
fn content(text: &str, lines: TextRange) -> Option<TextRange> {
    let written = slice::slice(text, lines)?;
    let trimmed = written.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let from = lines.start() + TextSize::of(written) - TextSize::of(trimmed);
    Some(TextRange::at(from, TextSize::of(trimmed.trim_end())))
}

/// The links wholly inside `range`, from its start.
fn within(links: &[Link], range: TextRange) -> Vec<Link> {
    let first = links.partition_point(|link| link.range.start() < range.start());
    links
        .get(first..)
        .unwrap_or_default()
        .iter()
        .take_while(|link| link.range.start() < range.end())
        .filter(|link| link.range.end() <= range.end())
        .map(|link| Link {
            range: link.range - range.start(),
            source: link.source.clone(),
        })
        .collect()
}

/// The links starting inside `range` of `text` that run past it, from
/// its start, cut at its last character that is not blank: the head of an
/// item (its key's line) moved away from the rest of its lines stays that
/// item's.
fn heads(links: &[Link], text: &str, range: TextRange) -> Vec<Link> {
    let Some(written) = slice::slice(text, range) else {
        return Vec::new();
    };
    let last = range.start() + TextSize::of(written.trim_end());
    let first = links.partition_point(|link| link.range.start() < range.start());
    links
        .get(first..)
        .unwrap_or_default()
        .iter()
        .take_while(|link| link.range.start() < range.end())
        .filter(|link| link.range.end() > range.end() && link.range.start() < last)
        .map(|link| Link {
            range: TextRange::new(link.range.start(), last) - range.start(),
            source: link.source.clone(),
        })
        .collect()
}

/// How far a position moves past some splices: the bytes they wrote, less
/// those they removed (kept as two sums, so no step goes below zero).
#[derive(Clone, Copy, Debug, Default)]
struct Shift {
    grown: TextSize,
    shrunk: TextSize,
}

impl Shift {
    /// The shift once `piece` is made too.
    fn past(&mut self, piece: &Piece) {
        self.grown += piece.inserted_len();
        self.shrunk += piece.range.len();
    }

    /// Where `at` (a position of the text before, past the splices'
    /// removals) lands.
    fn of(self, at: TextSize) -> TextSize {
        (at + self.grown)
            .checked_sub(self.shrunk)
            .unwrap_or_default()
    }
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
    // How far each splice's start moves: the shift of those before it.
    let mut moved = Vec::with_capacity(splices.len() + 1);
    let mut shift = Shift::default();
    for splice in splices {
        moved.push(shift);
        shift.past(splice);
    }
    moved.push(shift);
    let mut out = Vec::with_capacity(links.len());
    let mut removed = vec![Vec::new(); splices.len()];
    // The first splice not wholly before the link's start.
    let mut first = 0;
    for link in links {
        let (s, e) = (link.range.start(), link.range.end());
        while splices
            .get(first)
            .is_some_and(|splice| splice.range.end() <= s)
        {
            first += 1;
        }
        let start = moved[first].of(s);
        // Where its end lands, as the shift of the splices up to it.
        let mut end = Shift {
            grown: e + moved[first].grown,
            shrunk: moved[first].shrunk,
        };
        let mut alive = true;
        for (index, splice) in splices.iter().enumerate().skip(first) {
            note(Work::LinkStep);
            let (a, b) = (splice.range.start(), splice.range.end());
            if a >= e {
                break;
            }
            if a <= s && b >= e {
                removed[index].push(Link {
                    range: link.range - a,
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
                if a == s && splice.range.len() > TextSize::from(1) {
                    removed[index].push(Link {
                        range: TextRange::up_to(splice.range.len()),
                        source: link.source.clone(),
                    });
                }
                alive = false;
                break;
            }
            if b > e {
                end = Shift {
                    grown: moved[index].of(a) + splice.inserted_len(),
                    shrunk: TextSize::default(),
                };
                break;
            }
            end.past(splice);
        }
        let end = end.grown.checked_sub(end.shrunk);
        if alive && let Some(end) = end.filter(|end| *end > start) {
            out.push(Link {
                range: TextRange::new(start, end),
                source: link.source.clone(),
            });
        }
    }
    if let Some(inserted) = inserted {
        for (index, (splice, links)) in splices.iter().zip(inserted).enumerate() {
            let place = moved[index].of(splice.range.start());
            out.extend(links.iter().map(|link| Link {
                range: link.range + place,
                source: link.source.clone(),
            }));
        }
    }
    (out, removed)
}
