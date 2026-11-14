//! Which item of the host each item of an Edit-as save is, as the edit
//! made it (`bridge::item_trail`): the lines the user removed, moved, cut
//! and pasted in the view, not values that read alike. A host whose items
//! hold what no view shows (XML's `xsi:type` and namespace declarations)
//! reads it while the save writes and checks its text, so two items with
//! the same data keep their own markup (QA `E32-3`), and an item cut and
//! pasted is moved, not removed and added (QA `E32-2`).
#![cfg_attr(
    not(feature = "xml"),
    allow(dead_code, reason = "only XML hosts read the sources")
)]

use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Where an item of the saved value comes from, by its pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ItemSource {
    /// The item at this pointer of the host's last text.
    Previous(String),
    /// The item at this pointer of the host text the edit session began
    /// on: cut in an earlier save, pasted back in this one.
    Origin(String),
}

/// The items of one save's value, as the edit made them.
pub(crate) struct ItemSources {
    /// The host's last text, which the save patches.
    pub(crate) previous: String,
    /// The host text the edit session began on.
    pub(crate) origin: String,
    /// The values the save writes for the view's (as edited, and as the
    /// host reads it back): a write of another value (a step of a save
    /// made in several) is told nothing.
    pub(crate) values: Vec<Value>,
    /// Each item's source, by its pointer in the view's value: an item of
    /// a list at `/l/1` is at `/l/1`, and its list's lone item at `/l/0`
    /// or `/l`. Items not listed are not told by the edit.
    pub(crate) items: HashMap<String, ItemSource>,
}

impl ItemSources {
    /// The source of the item at `pointer` (`lone` when its list holds it
    /// alone, which the view may write either way).
    pub(crate) fn of(&self, pointer: &str, lone: bool) -> Option<&ItemSource> {
        self.items.get(pointer).or_else(|| {
            lone.then(|| self.items.get(&format!("{pointer}/0")))
                .flatten()
        })
    }

    /// Whether any item comes from the session's first host text.
    pub(crate) fn holds_origin_items(&self) -> bool {
        self.items
            .values()
            .any(|source| matches!(source, ItemSource::Origin(_)))
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<ItemSources>>> = const { RefCell::new(None) };
}

/// Told until dropped (see [`tell`]).
pub(crate) struct Told(Option<Rc<ItemSources>>);

impl Drop for Told {
    fn drop(&mut self) {
        let outer = self.0.take();
        let _ = CURRENT.try_with(|current| current.replace(outer));
    }
}

/// Tells `sources` to the writes and checks of a save until the guard
/// returned is dropped.
pub(crate) fn tell(sources: Option<ItemSources>) -> Told {
    let outer = CURRENT.with(|current| current.replace(sources.map(Rc::new)));
    Told(outer)
}

/// Adds `value` to the values the told sources hold for.
pub(crate) fn also_for(value: &Value) {
    CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        if let Some(sources) = current.as_mut()
            && !sources.values.contains(value)
        {
            let mut changed = ItemSources {
                previous: sources.previous.clone(),
                origin: sources.origin.clone(),
                values: sources.values.clone(),
                items: sources.items.clone(),
            };
            changed.values.push(value.clone());
            *sources = Rc::new(changed);
        }
    });
}

/// The sources told for a save over `previous` of a value `holds` says is
/// one of theirs.
pub(crate) fn told(previous: &str, holds: impl Fn(&Value) -> bool) -> Option<Rc<ItemSources>> {
    CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .filter(|sources| sources.previous == previous && sources.values.iter().any(&holds))
            .cloned()
    })
}
