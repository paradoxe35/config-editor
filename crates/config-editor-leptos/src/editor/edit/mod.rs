//! The edit pipeline: every change to the document is an [`Edit`] that
//! passes through [`EditorState::commit`], in the same stages, in the same
//! order, whatever made it.
//!
//! # Where edits come from
//!
//! | Entry point | Built in | Origin |
//! |---|---|---|
//! | Typing, IME, paste, cut, drop, dictation (the browser's own edits) | `input::flush_input` | [`Origin::Input`] |
//! | Enter, a typed pair, closing tag or re-indenting closer, a selection wrapped in a pair (typing, grouped with the typing around them) | `keys::*` | [`Origin::Input`] |
//! | Ctrl+Enter, Tab, Shift+Tab, Backspace in a pair, Ctrl+/, join lines, the line commands (closed folds given) | `keys::*` | [`Origin::Command`] |
//! | A line copied, cut or pasted whole, a cut across a fold | `input` | [`Origin::Command`] |
//! | An accepted suggestion | `completion::live` | [`Origin::Command`] |
//! | Replace, Replace All | `search::actions` | [`Origin::Command`] |
//! | Revert a change | `changes::commands` | [`Origin::Command`] |
//! | Undo, redo | `keys::navigation` | [`Origin::Undo`], [`Origin::Redo`] |
//! | Reload in the baseline notice | `host_editing::conflict` | [`Origin::Reload`] |
//! | A new controlled `value` | `sync::take_host_value` | [`Origin::HostValue`] |
//! | An `edit_as` switch | `sync::switch_to` | [`Origin::Switch`] |
//!
//! Two things happen before an edit exists, because the browser applies
//! typed text to the textarea itself: `beforeinput` cancels input while the
//! editor is read-only (and input reaching into a closed fold, after opening
//! it), and a key that is about to edit opens the folds its edit will touch
//! (`folding::open_folds_for_edit`, remembered so a refusal closes them
//! again). Nothing else writes the document: the pipeline's `write_text` is
//! private to it.
//!
//! # Stages
//!
//! 1. **Prepare** — the edit as it may be made. An edit cutting a secret
//!    placeholder takes it whole (`protect::widen_edit`); an Edit-as switch
//!    drops the folds, which belong to the other language's lines. First,
//!    so the check sees the edit that will really be made.
//! 2. **Check** — whether it may be made: not while read-only, and never
//!    changing a read-only field (`protect::check_edit`). The host's own
//!    texts (a reload, a controlled value, a switch) pass. A refused edit
//!    changes nothing: the folds opened for it close again, the textarea
//!    shows the document and the selection it started from, and the reason
//!    shows at the caret. Before anything is written, so a refusal has
//!    nothing to undo.
//! 3. **Apply** — what is kept by position follows the edit (secret
//!    placeholders, the host's ranges; folds inside `write_text`), the text
//!    is written, what is found by reading the text is found again (the
//!    indent unit, placeholders and ranges after a host text), and the
//!    selection is placed. Positions follow before the text changes, since
//!    they are mapped from the text before.
//! 4. **Record** — the undo history: one step for a command, typing grouped
//!    by kind, a step moved for undo and redo, a step for a switch (its
//!    snapshot in the language it left), nothing for a host value. After the apply stage, which settles the
//!    selection a step returns to.
//! 5. **Notify** — `on_change` with the new document, once the text, the
//!    caret and the history are all in their new state (a host reading the
//!    editor from the callback sees the edit whole); `on_edit_as_change`
//!    after an edit that changed the language on screen. Edit-as conversion
//!    back to the host's language, the dirty flag and the change count
//!    follow reactively from the text (see `sync`, `host_editing`,
//!    `changes`).
//!
//! What a caller does around an edit (closing suggestions, typing aids) is
//! its own; [`EditorState::commit`] returns whether the edit was made.

// Only protection follows positions through a change so far.
#[cfg(feature = "protect")]
mod change;
#[cfg(all(test, feature = "protect"))]
mod change_tests;
mod origin;
mod pipeline;
#[cfg(all(test, feature = "yaml"))]
mod tests;

#[cfg(feature = "protect")]
pub(super) use change::{Assoc, Change, CodeUnits, Moved};
pub(super) use origin::Origin;
#[cfg(all(test, feature = "yaml"))]
pub(super) use pipeline::{Stage, take_trace};

use leptos::prelude::*;

use super::bridge::links::Splice;
use super::fold_ranges::FoldRange;
use super::history::{Selection, Snapshot};
use super::input::{UNKNOWN_SELECTION, edit_range};
#[cfg(feature = "protect")]
use super::keyboard::utf16_to_byte_index;
use super::languages::Language;
use super::state::{EditorState, textarea_selection};
#[cfg(feature = "protect")]
use super::text::{TextRange, TextSize};

/// A change to the document: the text before and after, the selection it
/// was made on, where the selection goes, and what made it.
pub(super) struct Edit {
    /// The document before.
    pub(super) before: String,
    /// The selection the edit replaced, in `before`;
    /// [`UNKNOWN_SELECTION`] when the browser did not say (read off the
    /// change instead).
    pub(super) selection: Selection,
    /// The document after.
    pub(super) text: String,
    pub(super) after: After,
    pub(super) origin: Origin,
    /// The language `text` is in, when the edit changes the language on
    /// screen: an Edit-as switch, or an undo or redo back across one.
    pub(super) language: Option<Language>,
    /// The closed folds after the edit, when it says (a line command
    /// moving folded lines whole); else they follow the change.
    pub(super) folds: Option<Vec<FoldRange>>,
    /// What the edit replaced, when it says (a line command, an indent, a
    /// Replace all): an Edit-as view's items follow it (see
    /// `bridge::links`); else it is read off the selection or the caret.
    pub(super) splices: Option<Vec<Splice>>,
    /// What an undo or redo does, as its step recorded it (see
    /// `history::Record`); `None` for every other edit.
    #[cfg(feature = "protect")]
    pub(super) stepped: Option<Change>,
}

/// Where the selection goes after an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum After {
    /// `anchor..caret` selected, the caret scrolled into view with
    /// `reveal`.
    Select {
        anchor: usize,
        caret: usize,
        reveal: bool,
    },
    /// Where the browser left it after its own edit; set again only when
    /// the textarea had to be rewritten, or with `forced` (the edit was
    /// widened).
    Typed { selection: Selection, forced: bool },
    /// On the same text as before, mapped through the change, the view
    /// scrolled where it was (a host value).
    Mapped,
    /// Left as the textarea has it (an Edit-as switch).
    Kept,
}

impl Edit {
    /// A command replacing `selection` of `before` with `text`, the caret
    /// left at `caret`, in view.
    pub(super) fn new(before: String, selection: Selection, text: String, caret: usize) -> Self {
        Self {
            before,
            selection,
            text,
            after: After::Select {
                anchor: caret,
                caret,
                reveal: true,
            },
            origin: Origin::Command,
            language: None,
            folds: None,
            splices: None,
            #[cfg(feature = "protect")]
            stepped: None,
        }
    }

    /// A key typed that the editor applies itself where the browser would
    /// have typed it (a pair opened or a selection wrapped in it, a closer
    /// typed over, an empty pair deleted): grouped in the undo history
    /// with the typing around it, as `kind` input at `time`, so one undo
    /// never stops half-way through what was typed.
    pub(super) fn typed(mut self, kind: &str, time: f64) -> Self {
        self.origin = Origin::Input {
            kind: kind.to_string(),
            time,
        };
        self
    }

    /// Leaves `anchor..caret` selected.
    pub(super) fn selecting_from(mut self, from: usize) -> Self {
        if let After::Select { anchor, .. } = &mut self.after {
            *anchor = from;
        }
        self
    }

    /// Leaves exactly `folds` closed after it (those still folding in the
    /// new text), instead of following the change.
    pub(super) fn folding(mut self, folds: Vec<FoldRange>) -> Self {
        self.folds = Some(folds);
        self
    }

    /// Says what the edit replaced (see [`Self::splices`]).
    pub(super) fn splicing(mut self, splices: Option<Vec<Splice>>) -> Self {
        self.splices = splices;
        self
    }

    /// What the edit replaced, in bytes of the text before (see
    /// [`Self::splices`]).
    pub(super) fn replaced_splices(&self) -> Vec<Splice> {
        match &self.splices {
            // As said, unless the edit was made otherwise (widened).
            Some(splices)
                if Splice::make(&self.before, splices).as_deref() == Some(self.text.as_str()) =>
            {
                splices.clone()
            }
            _ => Splice::of(&self.before, &self.text, self.selection, self.caret()),
        }
    }

    /// What the edit did, exactly, as the code making it recorded it: the
    /// splices a command gave (a line command's lines moved with them),
    /// else the selection the edit replaced, when the texts agree it did
    /// (typing, paste, cut, drop, a command replacing its selection), or
    /// for an undo or redo the change its step recorded (`stepped`, when it
    /// still makes this edit's text); each replacement without the text it
    /// writes back as it was at its ends.
    /// `None` for an edit with no record, whose followers read the change
    /// off the two texts: an undo or redo of a step with no record (made by
    /// such an edit, or rebased over a host value), input whose selection
    /// the browser did not give
    /// (`UNKNOWN_SELECTION`), a command whose new text is not its selection
    /// replaced and that gave no splices (a comment toggle, a suggestion
    /// rewriting its line, Revert), and the host's own texts (a value, a
    /// reload, an Edit-as switch), which no check or follower judges.
    #[cfg(feature = "protect")]
    pub(super) fn change(&self) -> Option<Change> {
        self.recorded()?.narrowed(&self.before)
    }

    /// [`Self::change`] as recorded, before it is narrowed.
    #[cfg(feature = "protect")]
    fn recorded(&self) -> Option<Change> {
        if matches!(self.origin, Origin::Undo | Origin::Redo) {
            return self.stepped.clone().filter(|change| {
                change.apply(&self.before).as_deref() == Some(self.text.as_str())
            });
        }
        if !matches!(self.origin, Origin::Command | Origin::Input { .. }) {
            return None;
        }
        if let Some(splices) = self.splices.as_ref().filter(|splices| {
            Splice::make(&self.before, splices).as_deref() == Some(self.text.as_str())
        }) {
            return change_of_splices(&self.before, splices);
        }
        if self.selection == UNKNOWN_SELECTION {
            return None;
        }
        let (start, end) = self.selection;
        let byte = |at: usize| utf16_to_byte_index(&self.before, at);
        (start <= end)
            .then(|| Change::replacing_selection(&self.before, &self.text, byte(start)..byte(end)))
            .flatten()
    }

    /// Leaves the scroll position alone (an edit made with the mouse, where
    /// the caret is already in view).
    pub(super) fn quiet(mut self) -> Self {
        if let After::Select { reveal, .. } = &mut self.after {
            *reveal = false;
        }
        self
    }

    /// Makes it the host's text `host` the user chose to reload (see
    /// [`Origin::Reload`]).
    pub(super) fn reloading(mut self, host: String) -> Self {
        self.origin = Origin::Reload { host };
        self
    }

    /// Input the browser applied: `before` became `text`, the selection
    /// left at `typed`.
    pub(super) fn input(
        before: String,
        selection: Selection,
        text: String,
        typed: Selection,
        kind: String,
        time: f64,
    ) -> Self {
        Self {
            before,
            selection,
            text,
            after: After::Typed {
                selection: typed,
                forced: false,
            },
            origin: Origin::Input { kind, time },
            language: None,
            folds: None,
            splices: None,
            #[cfg(feature = "protect")]
            stepped: None,
        }
    }

    /// Undo (or redo, with `redo`) from `current` (`selection` selected)
    /// back to `snapshot`.
    pub(super) fn step(redo: bool, current: String, selection: Selection, to: Snapshot) -> Self {
        let (anchor, caret) = to.selection;
        Self {
            before: current,
            selection,
            text: to.value,
            after: After::Select {
                anchor,
                caret,
                reveal: true,
            },
            origin: if redo { Origin::Redo } else { Origin::Undo },
            language: to.language,
            folds: to.folds,
            splices: None,
            #[cfg(feature = "protect")]
            stepped: None,
        }
    }

    /// A text the host replaced the document with: a controlled `value`
    /// (`Origin::HostValue`) or the document converted by an Edit-as switch
    /// (`Origin::Switch`).
    pub(super) fn replacing(before: String, text: String, origin: Origin) -> Self {
        let after = if origin == Origin::Switch {
            After::Kept
        } else {
            After::Mapped
        };
        Self {
            before,
            selection: UNKNOWN_SELECTION,
            text,
            after,
            origin,
            language: None,
            folds: None,
            splices: None,
            #[cfg(feature = "protect")]
            stepped: None,
        }
    }

    /// The document converted by an Edit-as switch to `language`, made with
    /// `selection` (in `before`, where undo puts it back), the caret at
    /// `caret` (on the field it was on), else where the textarea has it.
    pub(super) fn switching(
        before: String,
        text: String,
        language: Language,
        selection: Selection,
        caret: Option<usize>,
    ) -> Self {
        let mut edit = Self::replacing(before, text, Origin::Switch);
        edit.language = Some(language);
        edit.selection = selection;
        if let Some(caret) = caret {
            edit.after = After::Select {
                anchor: caret,
                caret,
                reveal: true,
            };
        }
        edit
    }

    /// The caret the edit leaves (where the browser or the command put it).
    pub(super) fn caret(&self) -> usize {
        match self.after {
            After::Select { caret, .. } => caret,
            After::Typed { selection, .. } => selection.0,
            After::Mapped | After::Kept => 0,
        }
    }

    /// Where the text the edit wrote ends, which places the region it
    /// replaced among alike text: the caret it leaves, but for an undo or
    /// redo, whose caret is the selection it puts back (a copied line
    /// undone leaves the caret on the line it was copied from).
    #[cfg(feature = "protect")]
    pub(super) fn written_end(&self) -> Option<usize> {
        (!matches!(self.origin, Origin::Undo | Origin::Redo)).then(|| self.caret())
    }

    /// Puts the caret at `caret` with nothing selected (an edit widened
    /// over a placeholder).
    #[cfg(feature = "protect")]
    pub(super) fn collapse_to(&mut self, caret: usize) {
        match &mut self.after {
            After::Select {
                anchor, caret: at, ..
            } => {
                *anchor = caret;
                *at = caret;
            }
            After::Typed { selection, forced } => {
                *selection = (caret, caret);
                *forced = true;
            }
            After::Mapped | After::Kept => {}
        }
    }

    /// The selection the edit replaced, given the caret it left.
    pub(in crate::editor) fn replaced(&self, caret: usize) -> Selection {
        if self.selection == UNKNOWN_SELECTION {
            edit_range(&self.before, &self.text, caret)
        } else {
            self.selection
        }
    }
}

/// The change `splices` (sorted, apart) make of `before`, each one's
/// moved stretches carried to where its inserted text writes them.
#[cfg(feature = "protect")]
fn change_of_splices(before: &str, splices: &[Splice]) -> Option<Change> {
    let mut shift: isize = 0;
    let mut moved = Vec::new();
    for splice in splices {
        let at = splice.at.checked_add_signed(shift)?;
        moved.extend(splice.moved.iter().map(|piece| Moved {
            from: piece.from,
            len: piece.len,
            to: at + piece.to,
        }));
        shift += splice.inserted.len() as isize - splice.removed as isize;
    }
    let edits = splices.iter().map(|splice| {
        let start = TextSize::try_from(splice.at).ok()?;
        let len = TextSize::try_from(splice.removed).ok()?;
        Some((TextRange::at(start, len), splice.inserted.clone()))
    });
    let edits: Option<Vec<_>> = edits.collect();
    Some(Change::of_ranges(TextSize::of(before), edits?)?.with_moved(moved))
}

impl EditorState {
    /// Undo (or redo, with `redo`) as an edit from the document as it is;
    /// `None` with nothing to undo (redo).
    pub(super) fn history_edit(&self, redo: bool) -> Option<Edit> {
        let to = self.doc().history.with_untracked(|history| {
            if redo {
                history.next_redo().cloned()
            } else {
                history.next_undo().cloned()
            }
        })?;
        let selection = self.textarea().map_or_else(
            || {
                let at = self.doc().caret.get_untracked();
                (at, at)
            },
            |area| textarea_selection(&area),
        );
        let edit = Edit::step(redo, self.doc().text.get_untracked(), selection, to);
        #[cfg(feature = "protect")]
        let edit = Edit {
            stepped: self
                .doc()
                .history
                .with_untracked(|history| history.next_change(redo).cloned()),
            ..edit
        };
        Some(edit)
    }
}
