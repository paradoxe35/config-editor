//! The edit pipeline: every change to the document is an [`Edit`] that
//! passes through [`EditorState::commit`], in the same stages, in the same
//! order, whatever made it.
//!
//! # Where edits come from
//!
//! | Entry point | Built in | Origin |
//! |---|---|---|
//! | Typing, IME, paste, cut, drop, dictation (the browser's own edits) | `input::flush_input` | [`Origin::Input`] |
//! | Enter, Tab, Shift+Tab, pairs and tags, Backspace in a pair, Ctrl+/, join lines, the line commands (closed folds given) | `keys::*` | [`Origin::Command`] |
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

mod origin;
mod pipeline;
#[cfg(all(test, feature = "yaml"))]
mod tests;

pub(super) use origin::Origin;
#[cfg(all(test, feature = "yaml"))]
pub(super) use pipeline::{Stage, take_trace};

use leptos::prelude::*;

use super::fold_ranges::FoldRange;
use super::history::{Selection, Snapshot};
use super::input::{UNKNOWN_SELECTION, edit_range};
use super::languages::Language;
use super::state::{EditorState, textarea_selection};

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
        }
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
            folds: None,
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
        Some(Edit::step(
            redo,
            self.doc().text.get_untracked(),
            selection,
            to,
        ))
    }
}
