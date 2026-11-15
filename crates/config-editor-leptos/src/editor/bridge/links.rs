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

pub(crate) use clipboard::Clipboard;
use items::items;
pub(crate) use splices::Splice;
pub(crate) use told::Told;

/// Typing kept as splices before the links are read (see [`Links::Fresh`]).
const MOST_PENDING: usize = 64;

/// A host item: the host text (as a save wrote it, or as the host gave
/// it) and its pointer there.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Source {
    pub(crate) host: Arc<str>,
    pub(crate) pointer: String,
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

    /// The links once `splices` (sorted, apart, in the text before) are
    /// made to `before` (in `language`): the clipboard's records give an
    /// insert its links, and keep those an edit removes.
    pub(crate) fn edited(
        &mut self,
        language: &dyn ConfigLanguage,
        before: &str,
        splices: &[Splice],
        clipboard: &mut Clipboard,
    ) {
        let splices: Vec<&Splice> = splices
            .iter()
            .filter(|splice| {
                !splice.moved.is_empty()
                    || before.get(splice.at..splice.at + splice.removed)
                        != Some(splice.inserted.as_str())
            })
            .collect();
        if splices.is_empty() {
            return;
        }
        // Links of another text than `before` (one written without an
        // edit) tell nothing of it.
        if !self.fits(before.len()) {
            *self = Links::default();
        }
        // Typing changes no item's link: kept until the links are read.
        let typed = splices.iter().all(|splice| {
            splice.removed < 2 && splice.moved.is_empty() && !clipboard.holds(&splice.inserted)
        });
        if let Links::Fresh { pending, .. } = self
            && typed
            && pending.len() < MOST_PENDING
        {
            let pending = Arc::make_mut(pending);
            pending.extend(
                splices
                    .iter()
                    .map(|splice| (splice.at, splice.removed, splice.inserted.len())),
            );
            return;
        }
        let links = self.read(language);
        // Each splice with the links its insert takes: those of the text
        // it moves or copies, of the clipboard's record of its text, or of
        // its own lines written again at another indent; else it is
        // narrowed to what it changes (a whole text pasted with a few
        // changes keeps the items of the lines it writes as they were).
        let (splices, inserted): (Vec<Splice>, Vec<Vec<Link>>) = splices
            .iter()
            .map(|splice| {
                if !splice.moved.is_empty() {
                    let carried = splice
                        .moved
                        .iter()
                        .flat_map(|moved| {
                            within(&links, moved.from, moved.from + moved.len)
                                .into_iter()
                                .map(|link| Link {
                                    start: link.start + moved.to,
                                    end: link.end + moved.to,
                                    source: link.source,
                                })
                        })
                        .collect();
                    return ((*splice).clone(), carried);
                }
                let pasted = clipboard.links_of(&splice.inserted);
                if !pasted.is_empty() {
                    return ((*splice).clone(), pasted);
                }
                let (a, b) = (splice.at, splice.at + splice.removed);
                match clipboard::reindented(&before[a..b], within(&links, a, b), &splice.inserted) {
                    Some(reindented) if !reindented.is_empty() => ((*splice).clone(), reindented),
                    _ => (splice.narrowed(before), Vec::new()),
                }
            })
            .unzip();
        let splices: Vec<&Splice> = splices.iter().collect();
        let (mut kept, removed) = shifted(&links, &splices, Some(&inserted));
        for (splice, links) in splices.iter().zip(removed) {
            if !links.is_empty() {
                clipboard.keep(&before[splice.at..splice.at + splice.removed], links);
            }
        }
        kept.sort_by(|left, right| left.start.cmp(&right.start).then(right.end.cmp(&left.end)));
        // Each link a range of the text after the edit (QA `FZ38-17`).
        let length = splices.iter().fold(before.len(), |length, splice| {
            length + splice.inserted.len() - splice.removed
        });
        kept.retain(|link| {
            let fits = link.start < link.end && link.end <= length;
            debug_assert!(
                fits,
                "a link {}..{} outside a text of {length} after {splices:?}",
                link.start, link.end
            );
            fits
        });
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
                            pointer: item.pointer,
                        }),
                    })
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        // By where they start, an outer item before the ones inside it
        // (the items read in the value's order, which need not be the
        // text's: `within` and the edits find links by start, QA
        // `FZ38-17`).
        links.sort_by(|left, right| left.start.cmp(&right.start).then(right.end.cmp(&left.end)));
        for &(at, removed, inserted) in pending.iter() {
            let splice = Splice {
                at,
                removed,
                inserted: " ".repeat(inserted),
                moved: Vec::new(),
            };
            links = shifted(&links, &[&splice], None).0;
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

/// `links` (sorted by start) after `splices` (sorted, apart, in the text
/// before), with the links each insert takes (`inserted`, from its
/// start), and for each splice the links its removal covers, from its
/// start. One pass over both: a splice is looked at by the links it is
/// inside of only.
fn shifted(
    links: &[Link],
    splices: &[&Splice],
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
