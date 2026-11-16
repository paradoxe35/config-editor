//! Which item of the host each item of an Edit-as save is, as the edit
//! made it (`bridge::links`: each view item's link, kept by the edits that
//! moved, cut, copied and pasted it), not values that read alike. The save
//! passes it ([`SaveTold`]) to each write and check of its text, so two
//! items with the same data keep their own markup (XML's `xsi:type` and
//! namespace declarations), and an item cut and pasted is moved, not
//! removed and added.
#![cfg_attr(
    not(feature = "xml"),
    allow(dead_code, reason = "only XML hosts read the sources")
)]

use std::collections::HashMap;

mod pairing;

pub(crate) use pairing::Pairing;
#[cfg(feature = "xml")]
pub(crate) use pairing::{Side, Step, renamed_from};

/// Where an item of the saved value comes from, by its pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ItemSource {
    /// The item at this pointer of the host's last text.
    Previous(String),
    /// The item at this pointer of an earlier host text
    /// ([`ItemSources::origin`]): cut before an earlier save, pasted back
    /// in this one.
    Origin(String),
}

/// The items of one save's value, as the edit made them.
#[derive(Debug)]
pub(crate) struct ItemSources {
    /// The host's last text, which the save patches.
    pub(crate) previous: String,
    /// The earlier host text the [`ItemSource::Origin`] items are in.
    pub(crate) origin: String,
    /// Each item's (and map entry's) source, by its pointer in the view's
    /// value (an item of a list at `/l/1` is at `/l/1`, and its list's
    /// lone item at `/l/0` or `/l`, as the view writes it), and its path in
    /// the host text it is in, as that text writes it. Items not listed
    /// are not told by the edit.
    pub(crate) items: HashMap<String, ItemSource>,
}

impl ItemSource {
    /// Its pointer in the host text it is in.
    pub(crate) fn pointer(&self) -> &str {
        match self {
            ItemSource::Previous(pointer) | ItemSource::Origin(pointer) => pointer,
        }
    }
}

impl ItemSources {
    /// The source of the item at `pointer` of a value written (`lone`
    /// when its list holds it alone there, which the view may write
    /// either way: the view's item at `{pointer}/0` first, else the key or
    /// item at `pointer`).
    pub(crate) fn of(&self, pointer: &str, lone: bool) -> Option<&ItemSource> {
        lone.then(|| self.items.get(&format!("{pointer}/0")))
            .flatten()
            .or_else(|| self.items.get(pointer))
    }

    /// Whether any item comes from an earlier host text.
    pub(crate) fn holds_origin_items(&self) -> bool {
        self.items
            .values()
            .any(|source| matches!(source, ItemSource::Origin(_)))
    }
}

/// What an Edit-as save tells a write or a check of one of its steps:
/// which host item each item of the saved value is (when the edit's links
/// tell it), and whether the step writes over the host's last text, of
/// which the sources tell, or over a text an earlier step of the save
/// wrote. Outside a save (a conversion) it tells nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaveTold<'a> {
    sources: Option<&'a ItemSources>,
    over_last: bool,
}

impl<'a> SaveTold<'a> {
    /// A save told `sources`, writing over the host's last text.
    pub(crate) fn new(sources: Option<&'a ItemSources>) -> Self {
        SaveTold {
            sources,
            over_last: true,
        }
    }

    /// The same save, at a step written over a text an earlier step (or
    /// the save's own preparation) wrote.
    #[must_use]
    pub(crate) fn later(self) -> Self {
        SaveTold {
            over_last: false,
            ..self
        }
    }

    /// The sources, at whichever step: for a caller that reads each item's
    /// source text itself ([`ItemSources::previous`],
    /// [`ItemSources::origin`]).
    pub(crate) fn sources(self) -> Option<&'a ItemSources> {
        self.sources
    }

    /// The sources, at a step written over the host's last text: for a
    /// write of the value, and for the first step of a save made in
    /// several (an item the edit brought is added there while its source
    /// is still in it).
    pub(crate) fn over_last(self) -> Option<&'a ItemSources> {
        self.sources.filter(|_| self.over_last)
    }
}
