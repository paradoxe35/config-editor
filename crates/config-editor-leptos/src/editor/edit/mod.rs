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

mod browser;
mod change;
#[cfg(test)]
mod change_tests;
mod origin;
mod pipeline;
#[cfg(all(test, feature = "yaml"))]
mod tests;

#[cfg(feature = "protect")]
pub(super) use change::CodeUnits;
pub(super) use change::{Assoc, Change, Moved};
pub(super) use origin::Origin;
#[cfg(all(test, feature = "yaml"))]
pub(super) use pipeline::{Stage, take_trace};

use leptos::prelude::*;

#[cfg(test)]
use super::bridge::links::Splice;
use super::fold_ranges::FoldRange;
use super::history::Selection;
use super::input::UNKNOWN_SELECTION;
use super::keyboard::utf16_to_byte_index;
use super::languages::Language;
use super::paths::utf16_len;
use super::state::{EditorState, directed_selection};
use super::text::{TextRange, TextSize};

/// A change to the document: the text before, what the edit did to it
/// (the text after is made from that), the selection it was made on,
/// where the selection goes, and what made it.
pub(super) struct Edit {
    /// The document before.
    pub(super) before: String,
    /// What the edit does to `before`.
    made: Change,
    /// Whether the code making the edit said `made` (a command, browser
    /// input whose facts the texts confirm, an undo of such edits); else it
    /// was read off the two texts (see [`Self::change`]).
    told: bool,
    /// The document after: `made` made of `before`.
    pub(super) text: String,
    /// The selection the edit was made on, as (anchor, caret) UTF-16
    /// offsets of `before`: undo and a refusal put it back.
    pub(super) restore: Selection,
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

/// What an undo or redo does, as its step recorded it (see
/// `history::Next`), made on the text as it is.
pub(super) struct Stepped {
    pub(super) change: Change,
    pub(super) told: bool,
    /// The selection it puts back, (anchor, caret) bytes of the text after.
    pub(super) selection: Selection,
    pub(super) language: Option<Language>,
    pub(super) folds: Option<Vec<FoldRange>>,
}

impl Edit {
    /// `made` of `before`, as a command makes it.
    fn making(before: String, made: Change, told: bool, restore: Selection, after: After) -> Self {
        let (made, text) = match made.apply(&before) {
            Some(text) => (made, text),
            // Made on another text: a bug in the code making it, which
            // changes nothing rather than something else.
            None => {
                debug_assert!(false, "a change made on another text");
                (Change::unchanged(&before), before.clone())
            }
        };
        Self {
            before,
            made,
            told,
            text,
            restore,
            after,
            origin: Origin::Command,
            language: None,
            folds: None,
        }
    }

    /// A command making `change` of `before` from the selection `restore`
    /// ((anchor, caret), UTF-16, where undo puts it back), the caret left
    /// where the text it wrote ends, in view.
    pub(super) fn command(before: String, change: Change, restore: Selection) -> Self {
        let mut edit = Self::making(before, change, true, restore, After::Kept);
        let caret = edit.written_to();
        edit.after = After::Select {
            anchor: caret,
            caret,
            reveal: true,
        };
        edit
    }

    /// A command giving `text` from `before`, whose change is the UTF-16
    /// `selection` replaced when the texts agree it is, else read off the
    /// texts by the caret `caret` (UTF-16 of `text`) it leaves: the host's
    /// text a reload writes, and the tests'.
    pub(super) fn new(before: String, selection: Selection, text: String, caret: usize) -> Self {
        let (start, end) = (selection.0.min(selection.1), selection.0.max(selection.1));
        let byte = |at: usize| utf16_to_byte_index(&before, at);
        let said = (selection != UNKNOWN_SELECTION)
            .then(|| Change::replacing_selection(&before, &text, byte(start)..byte(end)))
            .flatten();
        let (made, told) = match said {
            Some(change) => (change, true),
            None => (Change::diffed_at(&before, &text, caret), false),
        };
        let after = After::Select {
            anchor: caret,
            caret,
            reveal: true,
        };
        Self::making(before, made, told, selection, after)
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

    /// Leaves `anchor..caret` selected (UTF-16 offsets of the text after).
    pub(super) fn selecting(mut self, anchor: usize, caret: usize) -> Self {
        if let After::Select {
            anchor: at,
            caret: to,
            ..
        } = &mut self.after
        {
            (*at, *to) = (anchor, caret);
        }
        self
    }

    /// Leaves the caret at UTF-16 `caret` of the text after.
    pub(super) fn caret_at(self, caret: usize) -> Self {
        self.selecting(caret, caret)
    }

    /// Leaves `from..caret` selected, the caret where it was to go.
    #[cfg(test)]
    #[allow(dead_code, reason = "used by the tests of some builds only")]
    pub(super) fn selecting_from(mut self, from: usize) -> Self {
        if let After::Select { anchor, .. } = &mut self.after {
            *anchor = from;
        }
        self
    }

    /// Leaves the caret where the change takes UTF-16 `at` of the text
    /// before, keeping to `assoc`'s side of text written right there.
    pub(super) fn keeping_caret(self, at: usize, assoc: Assoc) -> Self {
        let caret = self.map_units(at, assoc);
        self.caret_at(caret)
    }

    /// [`Self::keeping_caret`] for byte `at` of the text before.
    pub(super) fn keeping_byte(self, at: usize, assoc: Assoc) -> Self {
        let caret = units(&self.text, self.made.map_pos(at, assoc).pos);
        self.caret_at(caret)
    }

    /// Leaves `anchor..caret` selected, as byte offsets of the text after.
    pub(super) fn selecting_bytes(self, anchor: usize, caret: usize) -> Self {
        let (anchor, caret) = (units(&self.text, anchor), units(&self.text, caret));
        self.selecting(anchor, caret)
    }

    /// Where the change takes UTF-16 `at` of the text before, in UTF-16 of
    /// the text after.
    pub(super) fn map_units(&self, at: usize, assoc: Assoc) -> usize {
        let byte = utf16_to_byte_index(&self.before, at);
        units(&self.text, self.made.map_pos(byte, assoc).pos)
    }

    /// Where the text the change wrote ends (UTF-16 of the text after);
    /// with nothing written, the caret it was made at.
    fn written_to(&self) -> usize {
        match self.made.hull() {
            Some((_, written)) => units(&self.text, written.end),
            None => self.restore.1,
        }
    }

    /// Leaves exactly `folds` closed after it (those still folding in the
    /// new text), instead of following the change.
    pub(super) fn folding(mut self, folds: Vec<FoldRange>) -> Self {
        self.folds = Some(folds);
        self
    }

    /// The edit `splices` (bytes of the text before, sorted and apart) make,
    /// when they fit it.
    #[cfg(test)]
    #[allow(dead_code, reason = "used by the tests of some builds only")]
    pub(super) fn splicing(mut self, splices: Option<Vec<Splice>>) -> Self {
        if let Some(change) = splices.and_then(|splices| change_of_splices(&self.before, &splices))
            && let Some(text) = change.apply(&self.before)
        {
            self.made = change;
            self.told = true;
            self.text = text;
        }
        self
    }

    /// What the edit did, exactly, as the code making it said: a command's
    /// change (a line command's lines moved with them), browser input's
    /// from what the browser said of it, an undo or redo's as its step
    /// recorded it; each replacement without the text it writes back as it
    /// was at its ends.
    /// `None` for an edit whose change was read off the two texts, whose
    /// followers read it so too: input the browser did not say enough of
    /// (no `beforeinput`, a spelling fix: see `browser`), an undo or redo
    /// of a step such input made, and the host's own texts (a value, a
    /// reload, an Edit-as switch), which no check or follower judges.
    #[cfg(feature = "protect")]
    pub(super) fn change(&self) -> Option<Change> {
        let judged = matches!(
            self.origin,
            Origin::Command | Origin::Input { .. } | Origin::Undo | Origin::Redo
        );
        (self.told && judged).then(|| {
            let made = self.made.clone();
            made.clone().narrowed(&self.before).unwrap_or(made)
        })
    }

    /// What the edit does to the text before, said or read off the texts.
    pub(super) fn made(&self) -> &Change {
        &self.made
    }

    /// Whether the code making the edit said its change (see
    /// [`Self::change`]).
    pub(super) fn told(&self) -> bool {
        self.told
    }

    /// Makes the edit `change` instead, as said (an edit widened over a
    /// placeholder it cut).
    #[cfg(feature = "protect")]
    pub(super) fn rewrite(&mut self, change: Change) {
        if let Some(text) = change.apply(&self.before) {
            self.made = change;
            self.told = true;
            self.text = text;
        }
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

    /// Input the browser applied: `before` became `text`, made on
    /// `selection` ((anchor, caret), or [`UNKNOWN_SELECTION`] when the
    /// browser did not say), the selection left at `typed`. Its change is
    /// what the browser said, else read off the texts by the caret.
    pub(super) fn input(
        before: String,
        selection: Selection,
        text: String,
        typed: Selection,
        kind: String,
        time: f64,
    ) -> Self {
        let said = browser::said(&before, &text, selection, typed, &kind);
        let (made, told) = match said {
            Some(change) => (change, true),
            None => (Change::diffed_at(&before, &text, typed.0), false),
        };
        // Not said: undo puts back the selection over what it replaced.
        let restore = if told {
            selection
        } else {
            made.hull().map_or((typed.0, typed.0), |(range, _)| {
                (units(&before, range.start), units(&before, range.end))
            })
        };
        let after = After::Typed {
            selection: typed,
            forced: false,
        };
        let mut edit = Self::making(before, made, told, restore, after);
        edit.origin = Origin::Input { kind, time };
        edit
    }

    /// Input the browser applied to a document the host replaced meanwhile,
    /// made on that document instead (`change`, rebased over the host's;
    /// `told` when the browser said the input's own change): the caret
    /// after what it wrote, undo putting back the selection over what it
    /// replaced.
    pub(super) fn replayed(
        before: String,
        change: Change,
        told: bool,
        kind: String,
        time: f64,
    ) -> Self {
        let hull = change.hull();
        let restore = hull.as_ref().map_or((0, 0), |(range, _)| {
            (units(&before, range.start), units(&before, range.end))
        });
        let mut edit = Self::making(before, change, told, restore, After::Kept);
        let caret = hull.map_or(restore.1, |(_, written)| units(&edit.text, written.end));
        edit.after = After::Typed {
            selection: (caret, caret),
            forced: false,
        };
        edit.origin = Origin::Input { kind, time };
        edit
    }

    /// Undo (or redo, with `redo`) from `current`, made with `selection`
    /// selected, as its step recorded it; `None` when the step's change
    /// is not of this text.
    pub(super) fn step(
        redo: bool,
        current: String,
        selection: Selection,
        to: Stepped,
    ) -> Option<Self> {
        let text = to.change.apply(&current)?;
        let (anchor, caret) = (units(&text, to.selection.0), units(&text, to.selection.1));
        Some(Self {
            before: current,
            made: to.change,
            told: to.told,
            text,
            restore: selection,
            after: After::Select {
                anchor,
                caret,
                reveal: true,
            },
            origin: if redo { Origin::Redo } else { Origin::Undo },
            language: to.language,
            folds: to.folds,
        })
    }

    /// A text the host replaced the document with: a controlled `value`
    /// (`Origin::HostValue`) or the document converted by an Edit-as switch
    /// (`Origin::Switch`). Its change is read off the texts: the host says
    /// only what the text is.
    pub(super) fn replacing(before: String, text: String, origin: Origin) -> Self {
        let after = if origin == Origin::Switch {
            After::Kept
        } else {
            After::Mapped
        };
        let made = Change::diffed(&before, &text);
        Self {
            before,
            made,
            told: false,
            text,
            // Not checked, and no step of its own.
            restore: (0, 0),
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
        edit.restore = selection;
        if let Some(caret) = caret {
            edit.after = After::Select {
                anchor: caret,
                caret,
                reveal: true,
            };
        }
        edit
    }

    /// The selection the edit leaves, as (start, end) UTF-16 offsets of the
    /// text after; a caret at the start for one the textarea keeps.
    pub(super) fn selected(&self) -> Selection {
        match self.after {
            After::Select { anchor, caret, .. } => (anchor.min(caret), anchor.max(caret)),
            After::Typed { selection, .. } => selection,
            After::Mapped | After::Kept => (0, 0),
        }
    }

    /// The caret the edit leaves (where the browser or the command put it).
    #[cfg(any(test, feature = "protect"))]
    pub(super) fn caret(&self) -> usize {
        match self.after {
            After::Select { caret, .. } => caret,
            After::Typed { selection, .. } => selection.0,
            After::Mapped | After::Kept => 0,
        }
    }

    /// Where the text the edit wrote ends, which places the region it
    /// replaced among alike text when the change is read off the texts:
    /// the caret it leaves, but for an undo or redo, whose caret is the
    /// selection it puts back.
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
}

/// The UTF-16 offset of byte `at` of `text`.
pub(super) fn units(text: &str, at: usize) -> usize {
    utf16_len(text.get(..at).unwrap_or(text))
}

/// The change `splices` (sorted, apart) make of `before`, each one's
/// moved stretches carried to where its inserted text writes them.
pub(super) fn change_of_splices(
    before: &str,
    splices: &[super::bridge::links::Splice],
) -> Option<Change> {
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
        let next = self
            .doc()
            .history
            .with_untracked(|history| history.next(redo))?;
        let selection = self.textarea().map_or_else(
            || {
                let at = self.doc().caret.get_untracked();
                (at, at)
            },
            |area| directed_selection(&area),
        );
        let stepped = Stepped {
            change: next.change,
            told: next.told,
            selection: next.to.selection,
            language: next.to.language,
            folds: next.to.folds,
        };
        Edit::step(redo, self.doc().text.get_untracked(), selection, stepped)
    }
}
