//! The fields held (see `Protected::held`), each with why it was held,
//! recorded where the hold is decided: by the edit that made the field
//! read otherwise (what it replaced, what it wrote, where the lock went),
//! or by the step of placing that holds it. Later edits carry the cause as
//! it was recorded; nothing reads it back off the text.

use super::super::super::text::{TextRange, TextSize, slice};

use super::super::super::language::ConfigLanguage;
use super::super::super::paths::{key_range, with_spans};
use super::super::carry::Piece;
use super::super::locate::LineIndex;
use super::super::scan;
use super::super::texts::Reason;
use super::{Lock, Protected};

/// Why a locked field is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoldCause {
    /// The edit wrote its key, or a key holding it, again, and the other
    /// copy is read in its place; or its path names another copy (a key
    /// typed again ahead of it).
    Duplicate,
    /// The edit changed its own key or a key holding it (an undo or redo
    /// included).
    Renamed,
    /// An edit elsewhere left its text under another parent.
    Moved,
    /// A list item whose index now names another item (an edit before it
    /// in its list).
    ListItem,
    /// The edit commented its line out.
    Commented,
    /// The text no longer has it, and no lock says where it went (the host
    /// named its path after it was cut).
    Removed,
    /// The edit wrote text into it or beside it (a key in its block, a line
    /// taken into its value).
    Added,
    /// A field the language cannot locate (no spans) whose value reads
    /// otherwise.
    Spanless,
    /// A field a host range covers whose text moved where its list no
    /// longer reads it, or reads it otherwise for no cause above.
    RangeMoved,
    /// The host named its path after it was changed: the text reads it
    /// otherwise than the host's text does.
    ChangedBeforeLock,
    /// The edit's place says nothing of why (an edit far from it changed
    /// how the text around it reads).
    Unclear,
}

/// The fields held, in the order held, each once, with its cause.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Holds(Vec<(String, HoldCause)>);

impl Holds {
    pub(super) fn new(holds: Vec<(String, HoldCause)>) -> Self {
        Holds(holds)
    }

    /// The pointers held.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &String> {
        self.0.iter().map(|(pointer, _)| pointer)
    }

    /// Each pointer held with its cause.
    pub(crate) fn causes(&self) -> impl Iterator<Item = (&String, HoldCause)> {
        self.0.iter().map(|(pointer, cause)| (pointer, *cause))
    }

    /// The cause `pointer` is held for, when it is.
    pub(crate) fn cause(&self, pointer: &str) -> Option<HoldCause> {
        self.0
            .iter()
            .find(|(known, _)| known == pointer)
            .map(|(_, cause)| *cause)
    }

    pub(crate) fn contains(&self, pointer: &str) -> bool {
        self.cause(pointer).is_some()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<const N: usize> PartialEq<[&str; N]> for Holds {
    fn eq(&self, other: &[&str; N]) -> bool {
        self.0.len() == N && self.iter().zip(other).all(|(held, other)| held == other)
    }
}

/// What the edit that made a field read otherwise did, in the text before
/// and the text after.
pub(super) struct EditMade<'e> {
    /// The text before's locks.
    pub(super) before: &'e Protected,
    /// The language, the text after and its lines.
    pub(super) behavior: &'e dyn ConfigLanguage,
    pub(super) after: &'e str,
    pub(super) after_lines: &'e LineIndex<'e>,
    /// Each range the edit replaced, with what it wrote there, in order.
    pub(super) pieces: &'e [Piece],
    /// Whether a byte of the text after is in a comment.
    pub(super) in_comment: &'e dyn Fn(TextSize) -> bool,
}

impl EditMade<'_> {
    /// Why the field at `pointer`, whose lock went to `followed` in the
    /// text after (`None`: the edit changed its text's place beyond
    /// following), is held, the parse no longer having it when `gone`;
    /// `now` gives the stretch the parse writes it over in the text after.
    pub(super) fn cause(
        &self,
        pointer: &str,
        followed: Option<&Lock>,
        gone: bool,
        now: &dyn Fn() -> Option<TextRange>,
    ) -> HoldCause {
        let before = self.before;
        if before.unplaced.iter().any(|(known, _)| known == pointer) {
            return HoldCause::Spanless;
        }
        let Some(lock) = before
            .locks
            .iter()
            .find(|lock| lock.pointer.as_deref() == Some(pointer))
        else {
            return if gone {
                HoldCause::Removed
            } else {
                HoldCause::Unclear
            };
        };
        let anchor = followed.and_then(|lock| {
            lock.key
                .as_ref()
                .or(lock.ranges.first())
                .map(|range| range.start())
        });
        if anchor.is_some_and(|anchor| (self.in_comment)(anchor)) {
            return HoldCause::Commented;
        }
        let keys: Vec<&TextRange> = lock.key.iter().chain(&lock.guards).collect();
        if keys
            .iter()
            .any(|key| self.pieces.iter().any(|piece| renames(piece, key)))
        {
            return HoldCause::Renamed;
        }
        if self.writes_again(&keys) {
            return HoldCause::Duplicate;
        }
        let Some(hull) = lock
            .ranges
            .iter()
            .map(|range| range.start())
            .min()
            .zip(lock.ranges.iter().map(|range| range.end()).max())
        else {
            return HoldCause::Unclear;
        };
        let item = pointer
            .rsplit('/')
            .next()
            .is_some_and(|last| last.parse::<usize>().is_ok());
        if item
            && self
                .pieces
                .iter()
                .any(|piece| piece.replaced.end() <= hull.0)
        {
            return HoldCause::ListItem;
        }
        // Its text kept and no longer read at its path: under other keys.
        if gone {
            HoldCause::Moved
        } else if self.beside(hull) || self.wrote_into(now()) {
            HoldCause::Added
        } else if lock.reason == Reason::Range {
            HoldCause::RangeMoved
        } else {
            HoldCause::Unclear
        }
    }

    /// Whether text the edit wrote starts inside `now`, the stretch the
    /// field is written over in the text after, or at its end (a key typed
    /// into its block one character at a time, held at the last).
    fn wrote_into(&self, now: Option<TextRange>) -> bool {
        now.is_some_and(|now| {
            self.pieces
                .iter()
                .any(|piece| now.contains_inclusive(piece.wrote.start()))
        })
    }

    /// Whether the edit replaced text inside the stretch `(start, end)` of
    /// the text before, or next to it with only whitespace between.
    fn beside(&self, (start, end): (TextSize, TextSize)) -> bool {
        let text: &str = &self.before.text;
        let blank = |from: TextSize, to: TextSize| {
            slice::slice(text, TextRange::new(from, to))
                .is_some_and(|between| between.chars().all(char::is_whitespace))
        };
        self.pieces.iter().any(|piece| {
            let replaced = &piece.replaced;
            (replaced.start() <= end && replaced.end() >= start)
                || (replaced.start() > end && blank(end, replaced.start()))
                || (replaced.end() < start && blank(replaced.end(), start))
        })
    }

    /// Whether the edit wrote one of `keys` (key spans of the text before)
    /// again: the language reads a key in the text after that one of the
    /// edit's pieces wrote whole, with the same text.
    fn writes_again(&self, keys: &[&TextRange]) -> bool {
        let before: &str = &self.before.text;
        let names: Vec<&str> = keys
            .iter()
            .filter_map(|key| slice::slice(before, **key))
            .filter(|name| !name.is_empty())
            .collect();
        if names.is_empty() {
            return false;
        }
        let (after, after_lines) = (self.after, self.after_lines);
        // Only the lines a piece wrote on that hold one of the names are
        // asked for the key they start with.
        let mut naming: Vec<usize> = self
            .pieces
            .iter()
            .filter(|piece| !piece.wrote.is_empty())
            .flat_map(|piece| {
                after_lines.line_of(piece.wrote.start())..=after_lines.line_of(piece.wrote.end())
            })
            .filter(|line| {
                let text = after_lines.line_text(*line);
                names.iter().any(|name| text.contains(name))
            })
            .collect();
        naming.dedup();
        if naming.is_empty() {
            return false;
        }
        let pointers: Vec<String> =
            scan::written(self.behavior, after, after_lines, Some(&naming), &[])
                .into_iter()
                .map(|written| written.pointer)
                .collect();
        let written_whole = |key: &TextRange| {
            self.pieces
                .iter()
                .any(|piece| piece.wrote.contains_range(*key))
        };
        with_spans(self.behavior, after, &pointers, || {
            pointers.iter().any(|pointer| {
                key_range(self.behavior, after, pointer)
                    .map(|range| after_lines.range(range))
                    .is_some_and(|key| {
                        written_whole(&key)
                            && slice::slice(after, key).is_some_and(|name| names.contains(&name))
                    })
            })
        })
    }
}

/// Whether `piece` changed the key at `key`: it replaced text inside it,
/// or typed on one line at one of its ends.
fn renames(piece: &Piece, key: &TextRange) -> bool {
    let replaced = &piece.replaced;
    let inside = replaced.start() < key.end() && replaced.end() > key.start();
    let at_end = replaced.is_empty()
        && (replaced.start() == key.start() || replaced.start() == key.end())
        && !piece.written.is_empty()
        && !piece.written.contains('\n');
    inside || at_end
}
