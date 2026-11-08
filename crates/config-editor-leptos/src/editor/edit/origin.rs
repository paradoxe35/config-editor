//! What made an edit, and what each stage does for it: the pipeline's
//! policy, in one table.
//!
//! | Origin | Widened | Checked | Followed | History | `on_change` | Found again |
//! |---|---|---|---|---|---|---|
//! | `Command` | yes | yes | yes | its own step | yes | – |
//! | `Input` | yes | yes | yes | grouped by kind | yes | indent, after a paste or drop |
//! | `Undo`, `Redo` | – | yes | yes | the step moves | yes | placeholders |
//! | `Reload` | – | – | yes | its own step | yes | the host's placeholders |
//! | `HostValue` | – | – | – | carried into the steps before | – | indent, ranges, the host's placeholders |
//! | `Switch` | – | – | – | its own step | – | indent, ranges, placeholders |
//!
//! Widened: an edit cutting a secret placeholder takes it whole. Checked:
//! refused while read-only or when it changes a read-only field. Followed:
//! what is kept by position (placeholders, the host's ranges) moves with
//! the edit; texts replaced whole find them again instead. An undo or redo
//! back across a switch goes through as a `Switch` (another language's text,
//! whole), its history moving as an undo's does.

/// What made an edit.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::editor) enum Origin {
    /// A command: a key the editor handles itself, an accepted suggestion,
    /// a replace, a Revert, a line copied, cut or pasted whole.
    Command,
    /// Input the browser applied itself (typing, IME, paste, cut, drop),
    /// of `kind` (its `inputType`) at `time` (the event's time stamp).
    Input {
        kind: String,
        time: f64,
    },
    Undo,
    Redo,
    /// The host's text `host` the user chose to reload (the baseline
    /// notice): a read-only field does not hold it back, and its secret
    /// placeholders are the host's.
    Reload {
        host: String,
    },
    /// A new controlled `value` from the host.
    HostValue,
    /// The document converted by an `edit_as` switch: an undo step back to
    /// the language and text it left.
    Switch,
}

impl Origin {
    /// Whether an edit cutting a secret placeholder takes it whole.
    pub(super) fn widens(&self) -> bool {
        matches!(self, Origin::Command | Origin::Input { .. })
    }

    /// Whether the edit is checked: refused while read-only, or when it
    /// changes a read-only field. The host's own texts are not.
    pub(super) fn is_checked(&self) -> bool {
        matches!(
            self,
            Origin::Command | Origin::Input { .. } | Origin::Undo | Origin::Redo
        )
    }

    /// Whether what is kept by position follows the edit; a text replaced
    /// whole finds it again instead (see [`Self::finds_again`]).
    pub(super) fn follows(&self) -> bool {
        !matches!(self, Origin::HostValue | Origin::Switch)
    }

    /// Whether `on_change` hears of the edit: the user's edits do; a text
    /// from the host is its own already, and a switch reports through the
    /// conversion (see `sync`).
    pub(super) fn notifies(&self) -> bool {
        !matches!(self, Origin::HostValue | Origin::Switch)
    }

    /// Whether the edit may bring its own indentation, so the indent unit
    /// is detected again.
    pub(super) fn redetects_indent(&self) -> bool {
        match self {
            Origin::Input { kind, .. } => {
                matches!(kind.as_str(), "insertFromPaste" | "insertFromDrop")
            }
            Origin::HostValue | Origin::Switch => true,
            _ => false,
        }
    }
}
