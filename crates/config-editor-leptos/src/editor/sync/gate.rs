//! The one gate between the editor and its host. Every way a text leaves
//! the editor asks it: a change event (an edit typed in the host's
//! language, or a view's edit converted at a typing pause, a flush, a
//! switch or a change of the host's locks), the unmount's save, `save()`,
//! `value()` and `unified_diff()`. P1: a text the guard holds reaches the
//! host by none of them: the change events and the unmount's save send a
//! text they made through [`EditorState::admit`], the others ask for the
//! document through [`EditorState::to_host`], which admits it too.

use super::super::edit_as::SaveError;
use super::super::state::EditorState;

/// A way the host asks for the document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    /// `save()` and `Ctrl+S`: `on_save`.
    Save,
    /// The handle's `value()`.
    Value,
    /// The handle's `unified_diff()`.
    #[cfg(feature = "diff")]
    UnifiedDiff,
}

#[cfg(all(test, feature = "protect", feature = "json"))]
impl Route {
    pub(crate) const ALL: &[Route] = &[
        Route::Save,
        Route::Value,
        #[cfg(feature = "diff")]
        Route::UnifiedDiff,
    ];
}

/// Why a text does not reach the host.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Held {
    /// The text on screen is not written in the host's language: a view's
    /// edit does not convert (it does not parse, or holds a value the host's
    /// language cannot), or, for a save, the host's own reader refuses it.
    Unconverted,
    /// The guard holds it: a locked field reads otherwise than pinned, a
    /// placeholder given whole has text beside it, a field the text no
    /// longer locates is locked. With the message a save shows, when the
    /// guard words one.
    Guarded(Option<String>),
    /// A view's edit converted, but its save would change a read-only
    /// value, or glue or move a placeholder.
    Refused(SaveError),
}

/// How the text a route sends was made, which says what the gate checks
/// besides the guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Made {
    /// Typed in the host's language: each edit was judged against the
    /// locks as it was made (and a reload is the host's own text), so the
    /// guard alone decides.
    Typed,
    /// Converted from a view by a save over the host's text `previous`: the
    /// save is checked as a whole too.
    Converted,
    /// A host text the view's text is known to stand for (an undo back to
    /// a text saved before, a reload): the host's own, restored as it was.
    Restored,
}

impl EditorState {
    /// The guard's word on the text on screen: why it may not reach the
    /// host, if it may not.
    fn guard_holds(&self) -> Option<Held> {
        if let Some(message) = self.save_waits() {
            return Some(Held::Guarded(Some(message)));
        }
        self.changes_held().then_some(Held::Guarded(None))
    }

    /// Whether `text`, made as `made` for the host over its last text
    /// `previous`, may reach it. Every route that sends a text it made
    /// itself asks this (see [`Self::to_host`] for the others); a change
    /// event held is noted, so the host's locks changing sends it.
    pub(crate) fn admit(&self, previous: &str, text: &str, made: Made) -> Result<(), Held> {
        // A keystroke's change event reads no document (P5): the guard
        // judged the edit itself, and its predicate holds what it took but
        // the text reads otherwise. Text beside a placeholder the host wrote
        // that way itself is the host's own; a save and `value()` still
        // wait on it (see `to_host`).
        let held = match made {
            Made::Typed => self.changes_held().then_some(Held::Guarded(None)),
            Made::Converted | Made::Restored => self.guard_holds(),
        };
        self.note_held_back(held.is_some());
        if let Some(held) = held {
            return Err(held);
        }
        match made {
            Made::Typed | Made::Restored => Ok(()),
            Made::Converted => self.check_host_save(previous, text).map_err(Held::Refused),
        }
    }

    /// The document in the host's language, as `route` hands it to the
    /// host: typed input the browser sent in this task taken and a pending
    /// Edit-as edit converted first (it reaches `on_change` first), then
    /// passed through the gate.
    pub(crate) fn to_host(self, route: Route) -> Result<String, Held> {
        let text = self.host_document().ok_or(Held::Unconverted)?;
        if let Some(held) = self.guard_holds() {
            return Err(held);
        }
        // A converted view's host text parses already; the host's own
        // language is read for a save: a save never sends text its reader
        // refuses (a number the editor cannot hold exactly, which the
        // language allows, saves). The other routes carry half-typed text.
        if route == Route::Save
            && self.bridge().is_none()
            && !self.config().host_language.saves(&text)
        {
            return Err(Held::Unconverted);
        }
        Ok(text)
    }
}
