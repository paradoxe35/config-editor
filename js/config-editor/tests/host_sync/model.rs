//! What the simulated views share: the actions of a run, the host's lock
//! as it works it out in the text it names, the kinds of failure, and the
//! editor's core as far as locks go (`Core`).
//!
//! The core's record of its host (`js_support::HostChannel`: the echo
//! engine, the host log and the lists given) runs as it is; around it, an
//! edit touching a lock is refused (the guard) and every edit is recorded
//! in the host log, as the edit pipeline does. The locks are what the
//! channel places: the given ranges mapped from the text they name to the
//! document, a range whose text the document does not hold there locking
//! nothing. Texts are ASCII: bytes are UTF-16 units.

use std::ops::Range;

use leptos_config_editor::js_support::HostChannel;
use leptos_config_editor::{TextPosition, TextRange};

/// The locked text: the host locks it wherever it is in the text it names.
pub(crate) const LOCKED: &str = "secret: keep";

/// The documents the runs start from: the lock first, in the middle, on
/// the last line with no line break after it, and inside braces.
pub(crate) const BASES: &[&str] = &[
    "name: a\nport: 80\nsecret: keep\nhost: h\n",
    "secret: keep\nname: a\nport: 80\n",
    "name: a\nport: 80\nsecret: keep",
    "{\n  name: a,\n  port: 80,\n  secret: keep,\n  host: h\n}\n",
];

/// How the host writes its ranges: by lines and columns, or by string
/// index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    Lines,
    Index,
}

/// What a reset gives.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ResetKind {
    /// A newer version of the host's copy (a line changed, added or
    /// removed, never the locked text).
    NewVersion(u16),
    /// The text the editor holds.
    Same,
    /// The text the host listed last.
    LastListed,
    /// The text the document started from (a discard).
    Original,
    /// Two newer versions, one after the other in the same task.
    Twice(u16),
}

/// How a reset is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Via {
    /// `replaceValue`; the host lists the text later.
    ReplaceValue,
    /// A binding's `setValue`: `replaceValue`, then `onChange` with the
    /// reset's revision.
    SetValue,
    /// `setValue` and the parent's own state set to the text (with
    /// revisions only).
    Both,
}

/// Which text a host's list names (`rangesFor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Anchor {
    /// None: the value it comes with.
    None,
    /// The revision of the text it was worked out on.
    Revision,
    /// That text itself.
    Text,
}

/// One step of a run. Positions are taken modulo the text's length, a
/// pick modulo the host's queue, so any list of steps can be replayed
/// (and shrunk).
#[derive(Clone, Debug)]
pub(crate) enum Act {
    /// The user types `ch` at `at` in the shown document.
    Type { at: u16, ch: char },
    /// The user deletes `len` characters from `at`.
    Delete { at: u16, len: u8 },
    /// The user presses Enter at `at`.
    Enter { at: u16 },
    /// The user swaps line `line` with the next.
    MoveLine { line: u16 },
    /// The host hands back the text queued at `pick` for document `doc`
    /// (`dup`: twice in a row), with its ranges worked out in it
    /// (`ranges`), naming it by `anchor`.
    Deliver {
        doc: u8,
        pick: u16,
        dup: bool,
        ranges: bool,
        same_task: bool,
        anchor: Anchor,
    },
    /// The host fails to hand back the text queued at `pick` now, and
    /// hands it back after the others.
    Drop { doc: u8, pick: u16 },
    /// The host resets the document.
    Reset {
        doc: u8,
        kind: ResetKind,
        same_task: bool,
        via: Via,
    },
    /// The host gives a text of its own (`update({ value })`: another
    /// version of the file), its pending echoes dropped.
    Own {
        doc: u8,
        edit: u16,
        ranges: bool,
        same_task: bool,
    },
    /// The host gives ranges worked out on a text it holds or still has
    /// on its way (`worked_on`: a pick of its queue, else its store), a
    /// late server check, naming that text by `anchor`.
    Ranges {
        doc: u8,
        anchor: Anchor,
        worked_on: u16,
        same_task: bool,
    },
    /// The pause after an echo is over.
    Settle,
    /// The user shows document `doc` (a documents view only).
    Select { doc: u8 },
    /// The binding re-mounts the editor (a new `fileName`): the channel
    /// goes on, the props are rendered again.
    Remount { doc: u8 },
    /// The component is hidden and shown: a fresh editor on the parent's
    /// state, no channel; the binding hands it the gone editor's texts on
    /// their way (`HostChannel::handoff`).
    FreshMount { doc: u8 },
}

impl Act {
    pub(crate) fn same_task(&self) -> bool {
        match self {
            Act::Deliver { same_task, .. }
            | Act::Reset { same_task, .. }
            | Act::Own { same_task, .. }
            | Act::Ranges { same_task, .. } => *same_task,
            _ => false,
        }
    }

    /// The kind of step, as failures are grouped.
    pub(crate) fn kind(&self) -> String {
        let name = format!("{self:?}");
        let mut name = name
            .split([' ', '{'])
            .next()
            .unwrap_or_default()
            .to_string();
        if let Act::Reset { kind, .. } = self {
            let kind = format!("{kind:?}");
            name = format!("{name} {}", kind.split('(').next().unwrap_or_default());
        }
        if self.same_task() {
            format!("{name} in the same task")
        } else {
            name
        }
    }
}

/// The kinds of failure, each a class of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Class {
    /// The locked text changed, or the editor's lock no longer covers it.
    LockOff,
    /// A lock covers text the host did not give it for (beside the locked
    /// text), in a run whose host names its texts (revisions, anchors).
    LockOnOtherText,
    /// The lock covers text beside the locked text while texts are on
    /// their way (a host naming no text).
    OverLockInFlight,
    /// The same once every text came back.
    OverLockAtRest,
    /// A list naming a revision the editor can no longer name moved a
    /// lock.
    AnchorOutOfDateTaken,
    /// A list naming a text was refused (a revision the editor never
    /// sent).
    AnchorRefused,
    /// A text the host handed back replaced newer typing.
    TypingLost,
    /// The same, after a hide and show, by a late text of the editor that
    /// is gone.
    TypingLostAfterHideShow,
    /// The same, the text a listing of one the host reset to.
    RevertedByListing,
    /// The same, a reset to the text the editor held then.
    RevertedBySameListing,
    /// Once every text came back, the host and the editor differ.
    NotConverged,
    /// The same, the host holding a text of its own that is the one it
    /// gave last, given again.
    NotConvergedOnGivenAgain,
    /// Ranges past the end of the text they are read in.
    PastEnd,
    /// A panic.
    Panic,
}

#[derive(Clone, Debug)]
pub(crate) struct Failure {
    pub(crate) class: Class,
    /// The kind of step it failed at (`drain` once every text comes back).
    pub(crate) step: String,
    pub(crate) detail: String,
}

impl Class {
    /// The class of a host and an editor that differ at rest; `again`:
    /// the host holds a text of its own that is the one it gave last.
    pub(crate) fn not_converged(again: bool) -> Self {
        if again {
            Class::NotConvergedOnGivenAgain
        } else {
            Class::NotConverged
        }
    }
}

impl Failure {
    pub(crate) fn new(class: Class, detail: impl Into<String>) -> Self {
        Self {
            class,
            step: "drain".to_string(),
            detail: detail.into(),
        }
    }
}

pub(crate) type Outcome = Result<(), Failure>;

/// The host's lock in `text`, the text it works it out in: by lines, the
/// string indices, and its JSON in `form` (what a list is compared by).
#[derive(Clone, Debug)]
pub(crate) struct HostLock {
    pub(crate) range: TextRange,
    pub(crate) index: Range<usize>,
    pub(crate) print: String,
}

/// The host's lock in `text`, written in `form`; `None` when the locked
/// text is not there.
pub(crate) fn host_lock(text: &str, form: Form) -> Option<HostLock> {
    let start = text.find(LOCKED)?;
    let end = start + LOCKED.len();
    let range = TextRange::new(
        TextPosition::from_offset(text, start),
        TextPosition::from_offset(text, end),
    );
    let print = match form {
        Form::Lines => format!(
            "L{}:{}-{}:{}",
            range.start.line, range.start.column, range.end.line, range.end.column
        ),
        Form::Index => format!("I{start}-{end}"),
    };
    Some(HostLock {
        range,
        index: start..end,
        print,
    })
}

/// `lock` read in `text` as `ranges_from` reads it (a string index turned
/// into lines and columns in `text`); `None` when it names a place past
/// `text`'s end (`ranges_from` throws).
pub(crate) fn read_in(lock: &HostLock, text: &str, form: Form) -> Option<TextRange> {
    let range = match form {
        Form::Lines => lock.range,
        Form::Index => {
            if lock.index.end > text.len() {
                return None;
            }
            TextRange::new(
                TextPosition::from_offset(text, lock.index.start),
                TextPosition::from_offset(text, lock.index.end),
            )
        }
    };
    fits(range, text).then_some(range)
}

/// Whether `range` is inside `text`.
fn fits(range: TextRange, text: &str) -> bool {
    let lines: Vec<&str> = text.split('\n').collect();
    let inside = |at: TextPosition| {
        at.line >= 1
            && at.column >= 1
            && lines
                .get(at.line - 1)
                .is_some_and(|line| at.column <= line.len() + 1)
    };
    inside(range.start) && inside(range.end) && range.start <= range.end
}

/// A newer version of `text`: by `edit`, a line other than the locked one
/// changed, a line added, or one removed.
pub(crate) fn new_version(text: &str, edit: u16) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let free: Vec<usize> = (0..lines.len())
        .filter(|at| !lines[*at].contains(LOCKED))
        .collect();
    let pick = |n: usize| free[(edit as usize / 3) % n];
    match edit % 3 {
        0 if !free.is_empty() => {
            let at = pick(free.len());
            lines[at].push_str(&format!("v{}", edit % 10));
        }
        1 if free.len() > 1 => {
            let at = pick(free.len());
            lines.remove(at);
        }
        _ => {
            let at = (edit as usize / 3) % (lines.len() + 1);
            lines.insert(at, format!("n{}: 1", edit % 10));
        }
    }
    lines.join("\n")
}

/// The editor's core for one document, as far as locks go: its text and
/// its record of its host.
#[derive(Clone)]
pub(crate) struct Core {
    pub(crate) text: String,
    pub(crate) channel: HostChannel,
}

impl Core {
    /// The locks: the given ranges where the channel places them, a range
    /// whose text the document does not hold there locking nothing.
    pub(crate) fn locks(&self) -> Vec<Range<usize>> {
        self.channel
            .placed()
            .into_iter()
            .map(|(range, locked)| {
                let end = range.end.min(self.text.len());
                let start = range.start.min(end);
                if self.text.get(start..end) == Some(locked.as_str()) {
                    start..end
                } else {
                    start..start
                }
            })
            .collect()
    }

    /// An edit replacing `start..end` with `insert`: refused when it
    /// touches a lock (the guard), else made and recorded in the host log.
    pub(crate) fn edit(&mut self, start: usize, end: usize, insert: &str) -> bool {
        let touches = self.locks().iter().any(|lock| {
            !lock.is_empty()
                && if start == end {
                    lock.start < start && start < lock.end
                } else {
                    start < lock.end && end > lock.start
                }
        });
        if touches {
            return false;
        }
        self.channel.edited(start..end, insert);
        self.text.replace_range(start..end, insert);
        true
    }

    /// Checks the locked text is there once and locked, and (`exact`)
    /// that nothing beside it is.
    pub(crate) fn check(&self, what: &str, exact: Option<Class>) -> Outcome {
        if *self.channel.document() != *self.text {
            return Err(Failure::new(
                Class::Panic,
                format!(
                    "{what}: the channel's document {:?} is not the editor's {:?}",
                    self.channel.document(),
                    self.text
                ),
            ));
        }
        check_locks(&self.text, &self.locks(), what, exact)
    }
}

/// See [`Core::check`]; `locks` are offsets of `text`.
pub(crate) fn check_locks(
    text: &str,
    locks: &[Range<usize>],
    what: &str,
    exact: Option<Class>,
) -> Outcome {
    if text.matches(LOCKED).count() != 1 {
        return Err(Failure::new(
            Class::LockOff,
            format!("{what}: the locked text changed: {text:?}"),
        ));
    }
    let start = text.find(LOCKED).unwrap_or_default();
    let end = start + LOCKED.len();
    let covered = (start..end).all(|at| locks.iter().any(|lock| lock.contains(&at)));
    if !covered {
        return Err(Failure::new(
            Class::LockOff,
            format!("{what}: locks {locks:?} miss {start}..{end} of {text:?}"),
        ));
    }
    if let Some(class) = exact {
        let over = locks
            .iter()
            .any(|lock| !lock.is_empty() && (lock.start < start || lock.end > end));
        if over {
            return Err(Failure::new(
                class,
                format!("{what}: locks {locks:?} beyond {start}..{end} of {text:?}"),
            ));
        }
    }
    Ok(())
}

/// Where a user action edits `text`: the span replaced and the text put
/// there; `None` when it does nothing.
pub(crate) fn user_edit(act: &Act, text: &str) -> Option<(usize, usize, String)> {
    let at = |at: u16| at as usize % (text.len() + 1);
    match act {
        Act::Type { at: place, ch } => Some((at(*place), at(*place), ch.to_string())),
        Act::Enter { at: place } => Some((at(*place), at(*place), "\n".to_string())),
        Act::Delete { at: place, len } => {
            let start = at(*place).min(text.len().saturating_sub(1));
            let end = (start + (*len as usize % 3) + 1).min(text.len());
            (start < end).then(|| (start, end, String::new()))
        }
        Act::MoveLine { line } => {
            let lines: Vec<&str> = text.split('\n').collect();
            if lines.len() < 2 {
                return None;
            }
            let first = *line as usize % (lines.len() - 1);
            let start: usize = lines[..first].iter().map(|line| line.len() + 1).sum();
            let end = start + lines[first].len() + 1 + lines[first + 1].len();
            Some((
                start,
                end,
                format!("{}\n{}", lines[first + 1], lines[first]),
            ))
        }
        _ => None,
    }
}
