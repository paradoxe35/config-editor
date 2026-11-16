//! How the suggestion list was opened, one state for what used to be read
//! off several flags: closed, opened by typing, or asked for.

/// How the suggestion list was opened. It shows when it is open and has
/// options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ListMode {
    /// Not asked for.
    #[default]
    Closed,
    /// Opened by typing (or a typed pair starting an entry): it offers what
    /// matches the typed prefix.
    Typed {
        /// It showed options since it was last asked for: Backspace asks
        /// for it again, as taking back what the filter emptied it with
        /// can bring them back; a list that showed none has none to bring
        /// back, and is not worked out again on each Backspace.
        showed: bool,
        /// The user moved in it (see [`ListMode::heeded`]).
        heeded: bool,
    },
    /// Asked for with `Ctrl+Space`, or opened again after an accept: it
    /// offers more than what matches a typed prefix.
    Asked {
        /// Asked for over a selected word: it offers every key, and
        /// accepting one replaces the word.
        replace_word: bool,
        /// See [`ListMode::heeded`].
        heeded: bool,
    },
}

/// What the list's options are worked out for: changes of the rest of the
/// mode leave the options as they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ListQuery {
    /// Offers more than what matches a typed prefix (see [`ListMode::Asked`]).
    pub(crate) explicit: bool,
    pub(crate) replace_word: bool,
}

impl ListMode {
    /// Opened by typing, nothing shown yet.
    pub(crate) const TYPED: Self = Self::Typed {
        showed: false,
        heeded: false,
    };

    /// What its options are worked out for; `None` when closed.
    pub(crate) fn query(self) -> Option<ListQuery> {
        match self {
            Self::Closed => None,
            Self::Typed { .. } => Some(ListQuery {
                explicit: false,
                replace_word: false,
            }),
            Self::Asked { replace_word, .. } => Some(ListQuery {
                explicit: true,
                replace_word,
            }),
        }
    }

    pub(crate) fn is_open(self) -> bool {
        self != Self::Closed
    }

    /// Asked for (see [`ListMode::Asked`]).
    pub(crate) fn is_explicit(self) -> bool {
        matches!(self, Self::Asked { .. })
    }

    /// Asked for over a selected word.
    pub(crate) fn replaces_word(self) -> bool {
        matches!(
            self,
            Self::Asked {
                replace_word: true,
                ..
            }
        )
    }

    /// The user opened the open list (`Ctrl+Space`) or moved in it: typing
    /// that empties it says so; a list that opened by itself closes
    /// silently.
    pub(crate) fn heeded(self) -> bool {
        match self {
            Self::Closed => false,
            Self::Typed { heeded, .. } | Self::Asked { heeded, .. } => heeded,
        }
    }

    /// The same, `heeded` as given (see [`ListMode::heeded`]).
    pub(crate) fn heeding(mut self, to: bool) -> Self {
        if let Self::Typed { heeded, .. } | Self::Asked { heeded, .. } = &mut self {
            *heeded = to;
        }
        self
    }

    /// Whether Backspace asks for it again (see [`ListMode::Typed`]): it is
    /// open, and showed options (now, with `showing`, or since it was
    /// asked for), or was asked for.
    pub(crate) fn brings_back(self, showing: bool) -> bool {
        match self {
            Self::Closed => false,
            Self::Typed { showed, .. } => showing || showed,
            Self::Asked { .. } => true,
        }
    }

    /// The list after typing, which filters it again: it stays open as
    /// typing opens it (`auto_trigger`) or as Backspace brings it back
    /// (`refilter`), else closes; a word it would replace is gone.
    pub(crate) fn after_typing(self, auto_trigger: bool, refilter: bool, showing: bool) -> Self {
        let showed = self.brings_back(showing);
        let heeded = self.heeded() && showing;
        match self {
            Self::Asked { .. } if refilter => Self::Asked {
                replace_word: false,
                heeded,
            },
            _ if refilter || auto_trigger => Self::Typed { showed, heeded },
            _ => Self::Closed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_keeps_an_asked_list_only_for_a_backspace() {
        let asked = ListMode::Asked {
            replace_word: true,
            heeded: true,
        };
        assert_eq!(
            asked.after_typing(false, true, true),
            ListMode::Asked {
                replace_word: false,
                heeded: true
            }
        );
        assert_eq!(
            asked.after_typing(true, false, true),
            ListMode::Typed {
                showed: true,
                heeded: true
            }
        );
        assert_eq!(asked.after_typing(false, false, true), ListMode::Closed);
    }

    #[test]
    fn a_typed_list_that_showed_nothing_is_not_brought_back() {
        assert!(!ListMode::TYPED.brings_back(false));
        assert!(ListMode::TYPED.brings_back(true));
        assert_eq!(
            ListMode::TYPED.after_typing(true, false, false),
            ListMode::TYPED
        );
        assert!(!ListMode::Closed.brings_back(true));
    }
}
