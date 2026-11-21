//! The echo records of destroyed editors, kept page-wide by the name the
//! host gave their slot (`echo_key`, the JavaScript `echoKey`): a
//! conditional render makes a new element for the editor it shows again,
//! so only the host's name says it shows the same document. The next
//! editor made with that name takes the record over (see
//! [`Echoes::handed`](super::Echoes::handed)).
//!
//! A record is the prints of the texts on their way, never a text. One is
//! kept per name, the newest one left, and it goes when it is taken, when
//! an editor with that name starts with none to take, when its editor
//! left none on its way, and when the channel that left it goes on in a
//! new editor (a binding re-mounting it took the channel over: its texts
//! come back there). Nothing else tells that a record's texts can no
//! longer come back: a host that never shows the slot again keeps its
//! record, one per name, for the page's life.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use super::Link;
use super::echoes::Handoff;

/// A record kept, and the channel that left it.
struct Kept {
    from: Weak<RefCell<Link>>,
    handoff: Handoff,
}

thread_local! {
    static KEPT: RefCell<HashMap<String, Kept>> = RefCell::new(HashMap::new());
}

/// Keeps `handoff` (left by the channel `from`) under `key`, in place of
/// the one kept there; `None` keeps none.
pub(super) fn leave(key: &str, from: &Rc<RefCell<Link>>, handoff: Option<Handoff>) {
    KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        match handoff {
            Some(handoff) => {
                kept.insert(
                    key.to_string(),
                    Kept {
                        from: Rc::downgrade(from),
                        handoff,
                    },
                );
            }
            None => {
                kept.remove(key);
            }
        }
    });
}

/// Takes the record kept under `key`, if any.
pub(super) fn take(key: &str) -> Option<Handoff> {
    KEPT.with(|kept| kept.borrow_mut().remove(key))
        .map(|kept| kept.handoff)
}

/// The channel `from` goes on in a new editor: what it left is no record
/// to hand on.
pub(super) fn going_on(from: &Rc<RefCell<Link>>) {
    let from = Rc::downgrade(from);
    KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        if !kept.is_empty() {
            kept.retain(|_, kept| !kept.from.ptr_eq(&from));
        }
    });
}

/// How many records are kept under `key`: none or one (for the tests).
#[cfg(all(test, feature = "yaml"))]
pub(super) fn count_of(key: &str) -> usize {
    KEPT.with(|kept| usize::from(kept.borrow().contains_key(key)))
}
